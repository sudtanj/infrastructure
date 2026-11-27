//! T009: the in-memory `Api`, test-only. Unlike `HttpApi` it does not check sha shapes (tests use
//! short names like `c0`). A post replaces the status of the same context on that sha.
//!
//! Semantics the tests rely on:
//! - `branches` fails like `HttpApi` for a repo with more than `MAX_LISTED_BRANCHES` branches.
//! - Every `Api` call appends one string to `calls()`: `user`, `repo <r>`, `branches <r>`,
//!   `tree <r> <sha>`, `status <r> <sha>`, `commits <r> <tip>`, `compare <r> <base> <head>`,
//!   `post <r> <sha> <context> <state>`. `fail(prefix, err)` makes every later call whose string
//!   starts with `prefix` return `err` (it is still recorded in `calls()`, but not in `posts()`).
//! - `reads()` counts every call except `post`; `writes()` counts successful posts.
//! - A successful post is appended to the statuses of that sha (creator = the fake's user,
//!   `updated_at` = the fake's `now`), so a second plan sees it.
//! - `set_status*` ages are seconds before `now`; `set_commits` dates are ages the same way.
//! - Unset answers: `repo` gives id 1, not private, default branch `main`; `branches`, `status`,
//!   `commits` give empty lists; `tree` gives an empty tree; `compare` gives no files.
//! - `set_tree_truncated`: the recursive call answers `truncated: true` with no entries; the
//!   non-recursive call on `commit_sha` answers `root` as `(name, kind, object sha)`; a call on an
//!   object sha in `sub` answers that folder's `(path, kind)` entries.
use crate::github::{
    Api, ApiError, Branch, ChangedFile, Commit, Repo, StatusEntry, Tree, TreeEntry,
    MAX_LISTED_BRANCHES,
};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

#[derive(Clone, Debug, PartialEq)]
pub struct Post {
    pub repo: String,
    pub sha: String,
    pub context: String,
    pub state: String,
    pub description: String,
}

type Key = (String, String);

struct State {
    user: String,
    now: i64,
    repos: HashMap<String, Repo>,
    branches: HashMap<String, Vec<Branch>>,
    trees: HashMap<Key, Tree>,
    roots: HashMap<Key, Tree>,
    subs: HashMap<Key, Tree>,
    statuses: HashMap<Key, Vec<StatusEntry>>,
    commits: HashMap<Key, Vec<Commit>>,
    changed: HashMap<(String, String, String), Option<Vec<ChangedFile>>>,
    fails: Vec<(String, ApiError)>,
    calls: Vec<String>,
    reads: usize,
    writes: usize,
    posts: Vec<Post>,
}

pub struct FakeApi(Mutex<State>);

fn key(r: &str, s: &str) -> Key {
    (r.to_string(), s.to_string())
}

/// Unix seconds to `YYYY-MM-DDTHH:MM:SSZ` (civil from days, Hinnant).
fn rfc3339(t: i64) -> String {
    let (days, rem) = (t.div_euclid(86_400), t.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn tree_of(entries: &[(&str, &str)]) -> Tree {
    let entries = entries
        .iter()
        .map(|(p, k)| TreeEntry {
            path: p.to_string(),
            kind: k.to_string(),
        })
        .collect();
    Tree {
        entries,
        ..Tree::default()
    }
}

impl FakeApi {
    pub fn new(user: &str, now: i64) -> FakeApi {
        FakeApi(Mutex::new(State {
            user: user.to_string(),
            now,
            repos: HashMap::new(),
            branches: HashMap::new(),
            trees: HashMap::new(),
            roots: HashMap::new(),
            subs: HashMap::new(),
            statuses: HashMap::new(),
            commits: HashMap::new(),
            changed: HashMap::new(),
            fails: Vec::new(),
            calls: Vec::new(),
            reads: 0,
            writes: 0,
            posts: Vec::new(),
        }))
    }

    fn st(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap()
    }

    /// Records the call; returns the state, or the injected error for the first matching prefix.
    fn begin(&self, call: String, read: bool) -> Result<MutexGuard<'_, State>, ApiError> {
        let mut s = self.st();
        if read {
            s.reads += 1;
        }
        let err = s
            .fails
            .iter()
            .find(|(p, _)| call.starts_with(p.as_str()))
            .map(|(_, e)| e.clone());
        s.calls.push(call);
        match err {
            Some(e) => Err(e),
            None => Ok(s),
        }
    }

    pub fn set_repo(&self, repo: &str, id: u64, private: bool, default_branch: &str) {
        let r = Repo {
            id,
            private,
            default_branch: default_branch.to_string(),
        };
        self.st().repos.insert(repo.to_string(), r);
    }
    pub fn set_branch(&self, repo: &str, name: &str, sha: &str) {
        let b = Branch {
            name: name.to_string(),
            sha: sha.to_string(),
        };
        self.st()
            .branches
            .entry(repo.to_string())
            .or_default()
            .push(b);
    }
    pub fn set_tree(&self, repo: &str, commit_sha: &str, entries: &[(&str, &str)]) {
        self.st()
            .trees
            .insert(key(repo, commit_sha), tree_of(entries));
    }
    pub fn set_tree_truncated(
        &self,
        repo: &str,
        commit_sha: &str,
        root: &[(&str, &str, &str)],
        sub: &[(&str, &[(&str, &str)])],
    ) {
        let mut s = self.st();
        let pairs: Vec<(&str, &str)> = root.iter().map(|(n, k, _)| (*n, *k)).collect();
        let mut t = tree_of(&pairs);
        t.shas = root
            .iter()
            .map(|(n, _, o)| (n.to_string(), o.to_string()))
            .collect();
        s.roots.insert(key(repo, commit_sha), t);
        for (obj, entries) in sub {
            s.subs.insert(key(repo, obj), tree_of(entries));
        }
    }
    pub fn set_status(
        &self,
        repo: &str,
        sha: &str,
        context: &str,
        state: &str,
        age_secs: i64,
        description: &str,
    ) {
        let user = self.st().user.clone();
        self.set_status_by(repo, sha, context, state, age_secs, description, &user);
    }
    pub fn set_status_raw(
        &self,
        repo: &str,
        sha: &str,
        context: &str,
        state: &str,
        updated_at: &str,
        description: &str,
    ) {
        let mut s = self.st();
        let e = StatusEntry {
            context: context.to_string(),
            state: state.to_string(),
            updated_at: updated_at.to_string(),
            description: description.to_string(),
            creator_login: s.user.clone(),
        };
        s.statuses.entry(key(repo, sha)).or_default().push(e);
    }
    #[allow(clippy::too_many_arguments)]
    pub fn set_status_by(
        &self,
        repo: &str,
        sha: &str,
        context: &str,
        state: &str,
        age_secs: i64,
        description: &str,
        creator: &str,
    ) {
        let mut s = self.st();
        let e = StatusEntry {
            context: context.to_string(),
            state: state.to_string(),
            updated_at: rfc3339(s.now - age_secs),
            description: description.to_string(),
            creator_login: creator.to_string(),
        };
        s.statuses.entry(key(repo, sha)).or_default().push(e);
    }
    pub fn set_commits(&self, repo: &str, tip: &str, list: Vec<(String, Vec<String>, i64)>) {
        let mut s = self.st();
        let now = s.now;
        let v = list
            .into_iter()
            .map(|(sha, parents, age)| Commit {
                sha,
                parents,
                date: rfc3339(now - age),
            })
            .collect();
        s.commits.insert(key(repo, tip), v);
    }
    pub fn set_changed(&self, repo: &str, base: &str, head: &str, files: &[(&str, Option<&str>)]) {
        let v = files
            .iter()
            .map(|(f, p)| ChangedFile {
                filename: f.to_string(),
                previous_filename: p.map(str::to_string),
            })
            .collect();
        self.st()
            .changed
            .insert((repo.into(), base.into(), head.into()), Some(v));
    }
    pub fn set_changed_unknown(&self, repo: &str, base: &str, head: &str) {
        self.st()
            .changed
            .insert((repo.into(), base.into(), head.into()), None);
    }
    pub fn fail(&self, prefix: &str, err: ApiError) {
        self.st().fails.push((prefix.to_string(), err));
    }
    pub fn calls(&self) -> Vec<String> {
        self.st().calls.clone()
    }
    pub fn reads(&self) -> usize {
        self.st().reads
    }
    pub fn writes(&self) -> usize {
        self.st().writes
    }
    pub fn posts(&self) -> Vec<Post> {
        self.st().posts.clone()
    }
}

impl Api for FakeApi {
    fn user(&self) -> Result<String, ApiError> {
        Ok(self.begin("user".into(), true)?.user.clone())
    }

    fn repo(&self, r: &str) -> Result<Repo, ApiError> {
        let s = self.begin(format!("repo {r}"), true)?;
        let default = Repo {
            id: 1,
            private: false,
            default_branch: "main".into(),
        };
        Ok(s.repos.get(r).cloned().unwrap_or(default))
    }

    fn branches(&self, r: &str) -> Result<Vec<Branch>, ApiError> {
        let list = self
            .begin(format!("branches {r}"), true)?
            .branches
            .get(r)
            .cloned()
            .unwrap_or_default();
        if list.len() > MAX_LISTED_BRANCHES {
            return Err(ApiError::Net(format!(
                "{r}: more than {MAX_LISTED_BRANCHES} branches"
            )));
        }
        Ok(list)
    }

    fn tree(&self, r: &str, sha: &str, recursive: bool) -> Result<Tree, ApiError> {
        let s = self.begin(format!("tree {r} {sha}"), true)?;
        let k = key(r, sha);
        if s.roots.contains_key(&k) {
            // truncated repo: the recursive call is cut off, the plain call answers the root
            return Ok(if recursive {
                Tree {
                    truncated: true,
                    ..Tree::default()
                }
            } else {
                s.roots[&k].clone()
            });
        }
        let t = s.subs.get(&k).or_else(|| s.trees.get(&k));
        Ok(t.cloned().unwrap_or_default())
    }

    fn status(&self, r: &str, sha: &str) -> Result<Vec<StatusEntry>, ApiError> {
        let s = self.begin(format!("status {r} {sha}"), true)?;
        Ok(s.statuses.get(&key(r, sha)).cloned().unwrap_or_default())
    }

    fn commits(&self, r: &str, tip: &str) -> Result<Vec<Commit>, ApiError> {
        let s = self.begin(format!("commits {r} {tip}"), true)?;
        Ok(s.commits.get(&key(r, tip)).cloned().unwrap_or_default())
    }

    fn compare(
        &self,
        r: &str,
        base: &str,
        head: &str,
    ) -> Result<Option<Vec<ChangedFile>>, ApiError> {
        let s = self.begin(format!("compare {r} {base} {head}"), true)?;
        let k = (r.to_string(), base.to_string(), head.to_string());
        Ok(s.changed.get(&k).cloned().unwrap_or(Some(Vec::new())))
    }

    fn post_status(
        &self,
        r: &str,
        sha: &str,
        context: &str,
        state: &str,
        description: &str,
        _url: Option<&str>,
    ) -> Result<(), ApiError> {
        let mut s = self.begin(format!("post {r} {sha} {context} {state}"), false)?;
        s.writes += 1;
        s.posts.push(Post {
            repo: r.into(),
            sha: sha.into(),
            context: context.into(),
            state: state.into(),
            description: description.into(),
        });
        let e = StatusEntry {
            context: context.into(),
            state: state.into(),
            updated_at: rfc3339(s.now),
            description: description.into(),
            creator_login: s.user.clone(),
        };
        let list = s.statuses.entry(key(r, sha)).or_default();
        list.retain(|x| x.context != context);
        list.push(e);
        Ok(())
    }
}
