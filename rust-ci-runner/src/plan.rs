//! The plan in two phases (contracts/plan-output.md, data-model.md Item and PlanResult).
//! Phase A (`candidates`) only reads. Phase C (`finish`) refuses, caps, posts, and returns the rows.
//! No network here: everything goes through `Api`.
use crate::github::{Api, ApiError, Branch};
use crate::projects::{self, Project};
use crate::redact::Redactor;
use crate::util::{random_id, short_sha};
use crate::walk::{self, Decision};
use crate::watch::WatchEntry;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

const MAX_BRANCHES: usize = 100;
/// A rate-limited write waits this long once; the answer carries no `retry-after` seconds.
const LIMIT_WAIT: Duration = Duration::from_secs(30);
/// A tick that starts in the first 10 minutes of a UTC hour may fail (A12).
const FAIL_WINDOW: i64 = 600;

/// Sends one line to the log store; phase C ignores its error.
pub type Heartbeat = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub struct Apis<'a> {
    pub plain: &'a dyn Api,
    /// The token of the `allow_docker` entries (FR-047).
    pub docker: &'a dyn Api,
}

impl<'a> Apis<'a> {
    pub fn same(api: &'a dyn Api) -> Apis<'a> {
        Apis {
            plain: api,
            docker: api,
        }
    }

    fn pick(&self, docker: bool) -> &'a dyn Api {
        if docker {
            self.docker
        } else {
            self.plain
        }
    }
}

pub struct Opts {
    pub cap: usize,
    pub write_cap: usize,
    pub pace_ms: u64,
    pub now: i64,
    pub b_private: bool,
    pub log_store: bool,
    /// Values the redactor removes from every problem and summary line.
    pub secrets: Vec<String>,
    pub sleep: fn(Duration),
    /// Called once from phase C when `log_store` is set.
    pub heartbeat: Option<Heartbeat>,
    /// The Actions run id, for the heartbeat line.
    pub run_id: String,
}

impl Default for Opts {
    fn default() -> Opts {
        Opts {
            cap: 20,
            write_cap: 60,
            pace_ms: 750,
            now: crate::util::now() as i64,
            b_private: true,
            log_store: false,
            secrets: Vec::new(),
            sleep: std::thread::sleep,
            heartbeat: None,
            run_id: String::new(),
        }
    }
}

/// The matrix row (data-model.md Item). `run_id` and `n` are set by phase C.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub repo: String,
    pub repo_id: String,
    pub sha: String,
    pub branch: String,
    pub project: String,
    pub context: String,
    pub prev_sha: String,
    pub allow_docker: bool,
    pub fail_closed: bool,
    pub retry: bool,
    pub secrets_ok: bool,
    pub secrets: String,
    pub run_id: String,
    pub n: usize,
    /// Committer date of the tip, the sort key; never in the final rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<i64>,
}

/// A status post phase C still has to make.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Status {
    pub repo: String,
    pub sha: String,
    pub context: String,
    pub state: String,
    pub description: String,
    /// The repo is an `allow_docker` entry: post with the docker token.
    #[serde(default)]
    pub docker: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Candidates {
    pub rows: Vec<Item>,
    pub unchanged: Vec<Status>,
    /// Folder-name errors and closing errors.
    pub invalid: Vec<Status>,
    pub problems: Vec<String>,
    pub unchecked: Vec<String>,
    /// Repos reached, whether or not they gave a problem.
    pub checked: usize,
}

#[derive(Debug)]
pub struct Final {
    pub matrix: Vec<Item>,
    pub matrix_docker: Vec<Item>,
    pub problems: Vec<String>,
    pub unchecked: Vec<String>,
    pub waiting: usize,
    pub deferred_writes: usize,
    pub summary: String,
    pub fail_tick: bool,
}

enum Stop {
    Limit,
    Problem(String),
}

impl From<ApiError> for Stop {
    fn from(e: ApiError) -> Stop {
        match e {
            ApiError::RateLimited(_) => Stop::Limit,
            e => Stop::Problem(reason(&e)),
        }
    }
}

/// Status code or the last clause of the message: the path in front may hold a sha.
fn reason(e: &ApiError) -> String {
    match e {
        ApiError::Http(c, _) => format!("HTTP {c}"),
        ApiError::Net(m) | ApiError::RateLimited(m) => {
            m.rsplit(": ").next().unwrap_or(m).to_string()
        }
    }
}

fn problem(red: &Redactor, repo: &str, why: &str) -> String {
    let s = red.apply(&format!("{repo}: {why}"));
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// Phase A: reads only. Per-repo results are atomic; a rate limit stops all reads.
pub fn candidates(apis: &Apis, list: &[WatchEntry], o: &Opts) -> Candidates {
    let red = Redactor::new(o.secrets.clone());
    let mut c = Candidates::default();
    let names = |from: usize| {
        list[from..]
            .iter()
            .map(|e| e.repo.clone())
            .collect::<Vec<_>>()
    };
    // One cache and one `GET /user` per token, made at the first entry that uses it (FR-050).
    let mut caches: [Option<walk::Cache>; 2] = [None, None];
    let mut failed = [false; 2];
    for (i, e) in list.iter().enumerate() {
        let k = usize::from(e.allow_docker);
        let api = apis.pick(e.allow_docker);
        if caches[k].is_none() && !failed[k] {
            match api.user() {
                Ok(u) => caches[k] = Some(walk::Cache::new(&u)),
                Err(ApiError::RateLimited(_)) => {
                    c.unchecked = names(i);
                    break;
                }
                Err(err) => {
                    failed[k] = true;
                    c.problems.push(problem(
                        &red,
                        "list",
                        &format!("cannot read the token's user ({})", reason(&err)),
                    ));
                }
            }
        }
        let Some(cache) = caches[k].as_mut() else {
            continue;
        };
        match read_repo(api, e, o, cache) {
            Ok(p) => {
                c.rows.extend(p.rows);
                c.unchanged.extend(p.unchanged);
                c.invalid.extend(p.invalid);
            }
            Err(Stop::Problem(why)) => c.problems.push(problem(&red, &e.repo, &why)),
            Err(Stop::Limit) => {
                c.unchecked = names(i);
                break;
            }
        }
    }
    c.checked = list.len() - c.unchecked.len();
    c
}

fn read_repo(
    api: &dyn Api,
    e: &WatchEntry,
    o: &Opts,
    cache: &mut walk::Cache,
) -> Result<Candidates, Stop> {
    let repo = api.repo(&e.repo)?;
    if let Some(id) = e.id.filter(|id| *id != repo.id) {
        return Err(Stop::Problem(format!(
            "id {id} in the list, but the API says {}",
            repo.id
        )));
    }
    let mut tips = api.branches(&e.repo)?;
    tips.retain(|b| e.matches_branch(&b.name));
    if tips.len() > MAX_BRANCHES {
        return Err(Stop::Problem(format!(
            "more than {MAX_BRANCHES} matching branches"
        )));
    }
    // Branches sharing a sha plan once (FR-012, R2): under the default branch if it is among
    // them, else the alphabetically first name. The tick passes no secrets (feature 010, T017b).
    tips.sort_by(|a, b| a.name.cmp(&b.name));
    let mut groups: Vec<Branch> = Vec::new();
    for b in tips {
        match groups.iter_mut().find(|g| g.sha == b.sha) {
            Some(g) if b.name == repo.default_branch => g.name = b.name,
            Some(_) => {}
            None => groups.push(b),
        }
    }
    let mut out = Candidates::default();
    for b in &groups {
        let found = projects::discover(api, &e.repo, &b.sha)?;
        let projs: Vec<Project> = found
            .iter()
            .map(|f| Project {
                path: f.path.clone(),
                context: projects::context(&f.path),
            })
            .collect();
        if projs.is_empty() {
            continue;
        }
        let d = walk::decide(api, &e.repo, &b.sha, &projs, o.now, cache)?;
        for (f, p) in found.iter().zip(&projs) {
            let Some(dec) = d.by_context.get(&p.context) else {
                continue;
            };
            let post = |state: &str, description: String| Status {
                repo: e.repo.clone(),
                sha: b.sha.clone(),
                context: p.context.clone(),
                state: state.to_string(),
                description,
                docker: e.allow_docker,
            };
            match dec {
                Decision::Done => {}
                Decision::Close => out
                    .invalid
                    .push(post("error", "job did not finish (retry)".into())),
                Decision::Unchanged { since } => out.unchanged.push(post(
                    "success",
                    format!("unchanged since {}", short_sha(since)),
                )),
                // The retry mark keeps a bad folder from being posted again every tick.
                Decision::Run { retry, .. } if !f.valid => {
                    let mark = if *retry { " (retry)" } else { "" };
                    out.invalid
                        .push(post("error", format!("invalid project folder name{mark}")))
                }
                Decision::Run { prev, retry } => out.rows.push(Item {
                    repo: e.repo.clone(),
                    repo_id: if e.id == Some(repo.id) {
                        repo.id.to_string()
                    } else {
                        String::new()
                    },
                    sha: b.sha.clone(),
                    branch: b.name.clone(),
                    project: p.path.clone(),
                    context: p.context.clone(),
                    prev_sha: prev.clone().unwrap_or_default(),
                    allow_docker: e.allow_docker,
                    fail_closed: repo.private && !o.b_private,
                    retry: *retry,
                    secrets_ok: e.allow_docker && crate::image::publishes(&p.path, &b.name),
                    secrets: e.secrets.join(" "),
                    run_id: String::new(),
                    n: 0,
                    date: d.tip_date,
                }),
            }
        }
    }
    Ok(out)
}

/// Oldest commit first within a repo, then branch, then project; repos round-robin, the repo with
/// the oldest row first. Returns the capped list and how many rows were left out.
fn order(mut rows: Vec<Item>, cap: usize) -> (Vec<Item>, usize) {
    let total = rows.len();
    rows.sort_by(|a, b| {
        (a.date.unwrap_or(i64::MAX), &a.branch, &a.project).cmp(&(
            b.date.unwrap_or(i64::MAX),
            &b.branch,
            &b.project,
        ))
    });
    // After the global sort a repo's first appearance is its oldest row.
    let mut queues: Vec<VecDeque<Item>> = Vec::new();
    for r in rows {
        match queues.iter_mut().find(|q| q[0].repo == r.repo) {
            Some(q) => q.push_back(r),
            None => queues.push(VecDeque::from([r])),
        }
    }
    let mut out = Vec::new();
    loop {
        let before = out.len();
        for q in &mut queues {
            if out.len() < cap {
                out.extend(q.pop_front());
            }
        }
        if out.len() == before || out.len() >= cap {
            break;
        }
    }
    let waiting = total - out.len();
    (out, waiting)
}

/// Paced, capped status writes. A rate limit waits once, then defers everything that is left.
struct Writer<'a> {
    apis: &'a Apis<'a>,
    o: &'a Opts,
    red: &'a Redactor,
    used: usize,
    waited: bool,
    stopped: bool,
    deferred: usize,
    problems: Vec<String>,
}

impl Writer<'_> {
    /// True when the post went through; a failure is recorded, a deferral counted.
    fn send(&mut self, s: &Status) -> bool {
        if self.stopped || self.used >= self.o.write_cap {
            self.deferred += 1;
            return false;
        }
        if self.used > 0 {
            (self.o.sleep)(Duration::from_millis(self.o.pace_ms));
        }
        self.used += 1;
        loop {
            match self.apis.pick(s.docker).post_status(
                &s.repo,
                &s.sha,
                &s.context,
                &s.state,
                &s.description,
                None,
            ) {
                Ok(()) => return true,
                Err(ApiError::RateLimited(_)) if !self.waited => {
                    self.waited = true;
                    (self.o.sleep)(LIMIT_WAIT);
                }
                Err(ApiError::RateLimited(_)) => {
                    self.stopped = true;
                    self.deferred += 1;
                    return false;
                }
                Err(e) => {
                    self.problems.push(problem(
                        self.red,
                        &s.repo,
                        &format!("status post failed ({})", reason(&e)),
                    ));
                    return false;
                }
            }
        }
    }
}

/// Phase C: refuse, cap, post `pending` for each row first, then the other posts; a row exists
/// only when its `pending` went through.
pub fn finish(apis: &Apis, cand: Candidates, o: &Opts) -> Final {
    let red = Redactor::new(o.secrets.clone());
    let mut problems = cand.problems.clone();
    let mut refused: Vec<String> = Vec::new();
    let mut rows = Vec::new();
    for r in cand.rows {
        if r.fail_closed && !o.log_store {
            if !refused.contains(&r.repo) {
                refused.push(r.repo);
            }
        } else {
            rows.push(r);
        }
    }
    for repo in &refused {
        problems.push(problem(
            &red,
            repo,
            "private repository, public runner repo and no log store",
        ));
    }
    let (capped, waiting) = order(rows, o.cap);
    let mut w = Writer {
        apis,
        o,
        red: &red,
        used: 0,
        waited: false,
        stopped: false,
        deferred: 0,
        problems: Vec::new(),
    };
    let mut kept = Vec::new();
    for r in capped {
        let description = if r.retry { "queued (retry)" } else { "queued" };
        let s = Status {
            repo: r.repo.clone(),
            sha: r.sha.clone(),
            context: r.context.clone(),
            state: "pending".into(),
            description: description.into(),
            docker: r.allow_docker,
        };
        if w.send(&s) {
            kept.push(r);
        }
    }
    let unchanged = cand.unchanged.iter().filter(|s| w.send(s)).count();
    for s in &cand.invalid {
        w.send(s);
    }
    problems.append(&mut w.problems);
    let deferred_writes = w.deferred;
    for (n, r) in kept.iter_mut().enumerate() {
        r.n = n;
        r.run_id = random_id()[..16].to_string();
        r.date = None;
    }
    // FR-056: a delivery failure is ignored; the line only marks a tick that had candidate rows.
    if let Some(hb) = o.heartbeat.as_ref().filter(|_| o.log_store) {
        let _ = hb(&format!(
            "run={} tick items={} problems={}",
            o.run_id,
            kept.len(),
            problems.len()
        ));
    }
    let summary = summary(
        &kept,
        &cand.unchecked,
        &problems,
        [cand.checked, unchanged, waiting, deferred_writes],
    );
    let fail_tick = (!problems.is_empty() || !cand.unchecked.is_empty())
        && o.now.rem_euclid(3600) < FAIL_WINDOW;
    let (matrix_docker, matrix): (Vec<Item>, Vec<Item>) =
        kept.into_iter().partition(|r| r.allow_docker);
    Final {
        matrix,
        matrix_docker,
        problems,
        unchecked: cand.unchecked,
        waiting,
        deferred_writes,
        summary,
        fail_tick,
    }
}

/// Markdown with repo names only (FR-058): no branch, sha, project path or secret name.
fn summary(kept: &[Item], unchecked: &[String], problems: &[String], counts: [usize; 4]) -> String {
    let [checked, unchanged, waiting, deferred] = counts;
    let mut s = String::from("## factory-ci tick\n\n");
    for r in kept {
        s.push_str(&format!("- item {}: {}\n", r.n, r.repo));
    }
    s.push_str(&format!(
        "\nchecked repos: {checked}, planned: {}, unchanged: {unchanged}, waiting: {waiting}, writes deferred: {deferred}\n",
        kept.len()
    ));
    if !problems.is_empty() {
        s.push_str("\nproblems:\n");
        problems
            .iter()
            .for_each(|p| s.push_str(&format!("- {p}\n")));
    }
    if !unchecked.is_empty() {
        s.push_str("\nnot checked (rate limit):\n");
        unchecked
            .iter()
            .for_each(|r| s.push_str(&format!("- {r}\n")));
    }
    s
}
