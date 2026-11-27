//! The history walk: Done, Close, Unchanged or Run per project (data-model.md Decision per
//! project; FR-009 to FR-011). Only statuses made by the API user count (FR-049).
use crate::github::{Api, ApiError, ChangedFile, StatusEntry};
use crate::projects::{owns, Project};
use crate::util::parse_rfc3339;
use std::collections::{BTreeMap, HashMap};

const STALE_PENDING: i64 = 7200;
const OLD_ERROR: i64 = 1800;
const MAX_STEPS: usize = 50;
const MARK: &str = " (retry)";

#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    Done,
    /// A stale `pending` that already used its retry: the plan closes it with an `error` post.
    Close,
    Unchanged { since: String },
    Run { prev: Option<String>, retry: bool },
}

#[derive(Debug)]
pub struct Decisions {
    pub by_context: BTreeMap<String, Decision>,
    /// Committer date of the tip in Unix seconds; `None` when it was not read or not parsable.
    pub tip_date: Option<i64>,
}

/// Per-run memory, shared across tips and projects so no sha is asked twice.
pub struct Cache {
    user: String,
    statuses: HashMap<(String, String), Vec<StatusEntry>>,
    changed: HashMap<(String, String, String), Option<Vec<ChangedFile>>>,
}

impl Cache {
    pub fn new(user: &str) -> Cache {
        Cache { user: user.to_string(), statuses: HashMap::new(), changed: HashMap::new() }
    }
}

/// The latest status of `context` on `sha` made by the cache's user, if any.
fn own(api: &dyn Api, repo: &str, sha: &str, ctx: &str, cache: &mut Cache) -> Result<Option<StatusEntry>, ApiError> {
    let k = (repo.to_string(), sha.to_string());
    if !cache.statuses.contains_key(&k) {
        let v = api.status(repo, sha)?;
        cache.statuses.insert(k.clone(), v);
    }
    Ok(cache.statuses[&k].iter().rev().find(|s| s.context == ctx && s.creator_login == cache.user).cloned())
}

/// Changed files `base...head`; `None` when the compare stayed full and the set is unknown.
fn changed<'a>(
    api: &dyn Api,
    repo: &str,
    base: &str,
    head: &str,
    cache: &'a mut Cache,
) -> Result<&'a Option<Vec<ChangedFile>>, ApiError> {
    let k = (repo.to_string(), base.to_string(), head.to_string());
    if !cache.changed.contains_key(&k) {
        let v = api.compare(repo, base, head)?;
        cache.changed.insert(k.clone(), v);
    }
    Ok(&cache.changed[&k])
}

/// Decision from a status on the tip itself. An unparsable time is stale for a `pending` and
/// young for an `error`, so a bad clock never causes a retry loop.
fn at_tip(s: &StatusEntry, now: i64) -> Decision {
    let age = parse_rfc3339(&s.updated_at).map(|t| now - t);
    let marked = s.description.ends_with(MARK);
    let retry = Decision::Run { prev: None, retry: true };
    match s.state.as_str() {
        "pending" if age.is_none_or(|a| a > STALE_PENDING) => {
            if marked {
                Decision::Close
            } else {
                retry
            }
        }
        "error" if !marked && age.is_some_and(|a| a > OLD_ERROR) => retry,
        _ => Decision::Done,
    }
}

fn run(prev: Option<&str>) -> Decision {
    Decision::Run { prev: prev.map(str::to_string), retry: false }
}

/// Decides every project of one (repo, tip). `RateLimited` is returned; any other API error
/// makes the affected projects `Run`, never `Unchanged`.
pub fn decide(
    api: &dyn Api,
    repo: &str,
    tip: &str,
    projects: &[Project],
    now: i64,
    cache: &mut Cache,
) -> Result<Decisions, ApiError> {
    let mut by_context = BTreeMap::new();
    let mut open = Vec::new();
    for p in projects {
        match own(api, repo, tip, &p.context, cache) {
            Ok(Some(s)) => {
                by_context.insert(p.context.clone(), at_tip(&s, now));
            }
            Ok(None) => open.push(p),
            Err(e @ ApiError::RateLimited(_)) => return Err(e),
            Err(_) => {
                by_context.insert(p.context.clone(), run(None));
            }
        }
    }
    let needs_row = !open.is_empty() || by_context.values().any(|d| !matches!(d, Decision::Done | Decision::Close));
    if !needs_row {
        return Ok(Decisions { by_context, tip_date: None });
    }
    let commits = match api.commits(repo, tip) {
        Ok(c) => c,
        Err(e @ ApiError::RateLimited(_)) => return Err(e),
        Err(_) => {
            for p in &open {
                by_context.insert(p.context.clone(), run(None));
            }
            return Ok(Decisions { by_context, tip_date: None });
        }
    };
    let tip_date = commits.iter().find(|c| c.sha == tip).and_then(|c| parse_rfc3339(&c.date));
    // First-parent chain inside the window, at most MAX_STEPS ancestors, no recursion.
    let by_sha: HashMap<&str, _> = commits.iter().map(|c| (c.sha.as_str(), c)).collect();
    let mut chain: Vec<&str> = Vec::new();
    let mut cur = tip;
    while chain.len() < MAX_STEPS {
        let Some(parent) = by_sha.get(cur).and_then(|c| c.parents.first()) else { break };
        chain.push(parent);
        cur = parent;
    }
    let set: Vec<String> = projects.iter().map(|p| p.path.clone()).collect();
    for p in open {
        let d = walk_one(api, repo, tip, p, &chain, &set, cache)?;
        by_context.insert(p.context.clone(), d);
    }
    Ok(Decisions { by_context, tip_date })
}

fn walk_one(
    api: &dyn Api,
    repo: &str,
    tip: &str,
    p: &Project,
    chain: &[&str],
    set: &[String],
    cache: &mut Cache,
) -> Result<Decision, ApiError> {
    for &anc in chain {
        let s = match own(api, repo, anc, &p.context, cache) {
            Ok(Some(s)) => s,
            Ok(None) => continue,
            Err(e @ ApiError::RateLimited(_)) => return Err(e),
            Err(_) => return Ok(run(None)),
        };
        if s.state != "success" {
            return Ok(run(Some(anc)));
        }
        return match changed(api, repo, anc, tip, cache) {
            Ok(Some(files)) => {
                let touched = files.iter().any(|f| {
                    owns(&p.path, &f.filename, set)
                        || f.previous_filename.as_deref().is_some_and(|x| owns(&p.path, x, set))
                });
                Ok(if touched { run(Some(anc)) } else { Decision::Unchanged { since: anc.to_string() } })
            }
            Ok(None) => Ok(run(Some(anc))),
            Err(e @ ApiError::RateLimited(_)) => Err(e),
            Err(_) => Ok(run(Some(anc))),
        };
    }
    Ok(run(None))
}
