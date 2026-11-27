//! GitHub REST client behind the `Api` trait (contracts/github-api.md, research.md D3, D10, D12).
//! Synchronous; `ureq` is used here and in `ship.rs` only.
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

/// Reads are refused locally below this many remaining requests; writes still go out (D3).
pub const RATE_RESERVE: u32 = 60;
const TRIES: u32 = 3;
const SUFFIX: &str = " (logs not delivered)";

/// The token never enters any message: text is the path, the status code and a fixed phrase.
#[derive(Clone, Debug, PartialEq)]
pub enum ApiError {
    RateLimited(String),
    Http(u16, String),
    Net(String),
}

/// `GET /repos/{r}`: identity, visibility and default branch.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Repo {
    pub id: u64,
    pub private: bool,
    pub default_branch: String,
}

/// data-model.md Branch: `sha` is 40 lowercase hex, checked on receipt.
#[derive(Clone, Debug, PartialEq)]
pub struct Branch {
    pub name: String,
    pub sha: String,
}

/// data-model.md Commit: `parents[0]` is the first parent, `date` the committer date (RFC 3339 `Z`).
#[derive(Clone, Debug, PartialEq)]
pub struct Commit {
    pub sha: String,
    pub parents: Vec<String>,
    pub date: String,
}

/// data-model.md StatusEntry: `updated_at` is RFC 3339 `Z`; `creator_login` is empty when GitHub sends none.
#[derive(Clone, Debug, PartialEq)]
pub struct StatusEntry {
    pub context: String,
    pub state: String,
    pub updated_at: String,
    pub description: String,
    pub creator_login: String,
}

/// data-model.md TreeEntry: `kind` is `blob`, `tree` or another git type (`commit` for a submodule).
#[derive(Clone, Debug, PartialEq)]
pub struct TreeEntry {
    pub path: String,
    pub kind: String,
}

/// A tree answer. `shas` maps an entry path to its object sha, so a caller can walk a
/// non-recursive tree one folder at a time when `truncated` is true.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tree {
    pub entries: Vec<TreeEntry>,
    pub truncated: bool,
    pub shas: HashMap<String, String>,
}

/// data-model.md ChangedFile: a rename counts for both paths.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ChangedFile {
    pub filename: String,
    #[serde(default)]
    pub previous_filename: Option<String>,
}

pub trait Api {
    /// Login of the token's user.
    fn user(&self) -> Result<String, ApiError>;
    fn repo(&self, r: &str) -> Result<Repo, ApiError>;
    /// Every branch; an error when the repo has more than `MAX_LISTED_BRANCHES`.
    fn branches(&self, r: &str) -> Result<Vec<Branch>, ApiError>;
    fn tree(&self, r: &str, sha: &str, recursive: bool) -> Result<Tree, ApiError>;
    /// Latest status per context of a commit.
    fn status(&self, r: &str, sha: &str) -> Result<Vec<StatusEntry>, ApiError>;
    fn commits(&self, r: &str, tip: &str) -> Result<Vec<Commit>, ApiError>;
    /// `Ok(None)`: still full after 3 pages, so the changed set is unknown.
    fn compare(&self, r: &str, base: &str, head: &str) -> Result<Option<Vec<ChangedFile>>, ApiError>;
    fn post_status(
        &self,
        r: &str,
        sha: &str,
        context: &str,
        state: &str,
        description: &str,
        url: Option<&str>,
    ) -> Result<(), ApiError>;
}

/// `branches()` refuses a longer list: five full pages of 100.
pub const MAX_LISTED_BRANCHES: usize = 500;

pub struct HttpApi {
    base: String,
    token: String,
    pub(crate) remaining: AtomicI64,
    backoff: Duration,
    agent: ureq::Agent,
}

impl HttpApi {
    pub fn new(base: &str, token: &str) -> HttpApi {
        HttpApi {
            base: base.trim_end_matches('/').to_string(),
            token: token.trim().to_string(),
            remaining: AtomicI64::new(i64::MAX),
            backoff: Duration::from_secs(1),
            // No redirects: the token must not follow a Location header.
            agent: ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).redirects(0).build(),
        }
    }

    /// `GITHUB_API_URL` (default `https://api.github.com`) and the token in `token_var`.
    pub fn from_env(token_var: &str) -> Result<HttpApi, String> {
        let token = std::env::var(token_var).unwrap_or_default();
        if token.trim().is_empty() {
            return Err(format!("{token_var} is not set"));
        }
        let base = std::env::var("GITHUB_API_URL").unwrap_or_default();
        let base = if base.is_empty() { "https://api.github.com" } else { &base };
        Ok(HttpApi::new(base, &token))
    }

    /// Base delay of the retries: one delay, then two.
    pub fn with_backoff(mut self, d: Duration) -> HttpApi {
        self.backoff = d;
        self
    }

    fn note(&self, r: &ureq::Response) {
        if let Some(n) = r.header("x-ratelimit-remaining").and_then(|v| v.trim().parse::<i64>().ok()) {
            self.remaining.store(n, Ordering::Relaxed);
        }
    }

    fn classify(&self, r: ureq::Response, what: &str) -> ApiError {
        let code = r.status();
        let zero = r.header("x-ratelimit-remaining").map(str::trim) == Some("0");
        let retry = r.header("retry-after").is_some();
        let text = r.into_string().unwrap_or_default().to_lowercase();
        let limited = code == 429
            || (code == 403 && (zero || retry || text.contains("rate limit") || text.contains("abuse")));
        if limited {
            ApiError::RateLimited(format!("{what}: rate limited ({code})"))
        } else {
            ApiError::Http(code, format!("{what} answered {code}"))
        }
    }

    /// One request with up to `TRIES` attempts; a 5xx and a transport failure are retried.
    fn send(&self, method: &str, path: &str, body: Option<&str>) -> Result<String, ApiError> {
        let url = format!("{}{}", self.base, path);
        let what = format!("{method} {}", path.split('?').next().unwrap_or(path));
        let mut last = ApiError::Net(format!("{what}: no attempt made"));
        for attempt in 0..TRIES {
            if attempt > 0 {
                std::thread::sleep(self.backoff * attempt);
            }
            let req = self
                .agent
                .request(method, &url)
                .set("Authorization", &format!("Bearer {}", self.token))
                .set("Accept", "application/vnd.github+json")
                .set("X-GitHub-Api-Version", "2022-11-28")
                .set("User-Agent", "rust-ci-runner");
            let res = match body {
                Some(b) => req.set("Content-Type", "application/json").send_string(b),
                None => req.call(),
            };
            match res {
                Ok(r) if r.status() < 300 => {
                    self.note(&r);
                    return r.into_string().map_err(|_| ApiError::Net(format!("{what}: unreadable answer")));
                }
                Ok(r) | Err(ureq::Error::Status(_, r)) => {
                    self.note(&r);
                    last = self.classify(r, &what);
                    if !matches!(last, ApiError::Http(c, _) if c >= 500) {
                        return Err(last);
                    }
                }
                Err(ureq::Error::Transport(_)) => last = ApiError::Net(format!("{what}: transport failure")),
            }
        }
        Err(last)
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        if self.remaining.load(Ordering::Relaxed) < i64::from(RATE_RESERVE) {
            return Err(ApiError::RateLimited("local rate reserve reached".into()));
        }
        let text = self.send("GET", path, None)?;
        serde_json::from_str(&text).map_err(|_| malformed(path))
    }

    /// Reads `{path}&page=N` for N in 1..=max. `split` gives one answer's items and its total, if
    /// any. The flag is true when the last allowed page was still full.
    fn pages<P: DeserializeOwned, T>(
        &self,
        path: &str,
        max: u32,
        split: impl Fn(P) -> (Vec<T>, Option<usize>),
    ) -> Result<(Vec<T>, bool), ApiError> {
        let mut all = Vec::new();
        for page in 1..=max {
            let (items, total) = split(self.get(&format!("{path}&page={page}"))?);
            let short = items.len() < 100;
            all.extend(items);
            if short || total.is_some_and(|t| all.len() >= t) {
                return Ok((all, false));
            }
        }
        Ok((all, true))
    }
}

fn malformed(path: &str) -> ApiError {
    ApiError::Http(0, format!("{}: malformed answer", path.split('?').next().unwrap_or(path)))
}

/// 40 lowercase hex characters.
pub fn is_sha(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn check_sha(s: &str) -> Result<(), ApiError> {
    if is_sha(s) {
        Ok(())
    } else {
        Err(ApiError::Http(0, "not a 40-character lowercase hex sha".into()))
    }
}

#[derive(Deserialize)]
struct WSha {
    sha: String,
}
#[derive(Deserialize)]
struct WBranch {
    name: String,
    commit: WSha,
}
#[derive(Deserialize)]
struct WCommit {
    sha: String,
    #[serde(default)]
    parents: Vec<WSha>,
    commit: WInner,
}
#[derive(Deserialize)]
struct WInner {
    committer: WWho,
}
#[derive(Deserialize)]
struct WWho {
    date: String,
}
#[derive(Deserialize)]
struct WTree {
    #[serde(default)]
    tree: Vec<WEntry>,
    #[serde(default)]
    truncated: bool,
}
#[derive(Deserialize)]
struct WEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    sha: Option<String>,
}
#[derive(Deserialize)]
struct WStatuses {
    #[serde(default)]
    total_count: usize,
    #[serde(default)]
    statuses: Vec<WStatus>,
}
#[derive(Deserialize)]
struct WStatus {
    context: String,
    state: String,
    updated_at: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    creator: Option<WLogin>,
}
#[derive(Deserialize)]
struct WLogin {
    login: String,
}
#[derive(Deserialize)]
struct WFiles {
    #[serde(default)]
    files: Vec<ChangedFile>,
}

impl Api for HttpApi {
    fn user(&self) -> Result<String, ApiError> {
        Ok(self.get::<WLogin>("/user")?.login)
    }

    fn repo(&self, r: &str) -> Result<Repo, ApiError> {
        self.get(&format!("/repos/{r}"))
    }

    fn branches(&self, r: &str) -> Result<Vec<Branch>, ApiError> {
        let path = format!("/repos/{r}/branches?per_page=100");
        // A sixth page tells exactly 500 branches from more; more is an error (R9).
        let (v, full) = self.pages(&path, 6, |p: Vec<WBranch>| {
            (p.into_iter().map(|b| Branch { name: b.name, sha: b.commit.sha }).collect(), None)
        })?;
        if full || v.len() > MAX_LISTED_BRANCHES {
            return Err(ApiError::Net(format!("{r}: more than {MAX_LISTED_BRANCHES} branches")));
        }
        v.iter().try_for_each(|b| check_sha(&b.sha).map_err(|_| malformed(&path)))?;
        Ok(v)
    }

    fn tree(&self, r: &str, sha: &str, recursive: bool) -> Result<Tree, ApiError> {
        check_sha(sha)?;
        let q = if recursive { "?recursive=1" } else { "" };
        let w: WTree = self.get(&format!("/repos/{r}/git/trees/{sha}{q}"))?;
        let mut t = Tree { truncated: w.truncated, ..Tree::default() };
        for e in w.tree {
            if let Some(s) = e.sha {
                t.shas.insert(e.path.clone(), s);
            }
            t.entries.push(TreeEntry { path: e.path, kind: e.kind });
        }
        Ok(t)
    }

    fn status(&self, r: &str, sha: &str) -> Result<Vec<StatusEntry>, ApiError> {
        check_sha(sha)?;
        let path = format!("/repos/{r}/commits/{sha}/status?per_page=100");
        let (v, _) = self.pages(&path, 5, |p: WStatuses| {
            let items = p
                .statuses
                .into_iter()
                .map(|s| StatusEntry {
                    context: s.context,
                    state: s.state,
                    updated_at: s.updated_at,
                    description: s.description.unwrap_or_default(),
                    creator_login: s.creator.map(|c| c.login).unwrap_or_default(),
                })
                .collect();
            (items, Some(p.total_count))
        })?;
        Ok(v)
    }

    fn commits(&self, r: &str, tip: &str) -> Result<Vec<Commit>, ApiError> {
        check_sha(tip)?;
        let path = format!("/repos/{r}/commits?sha={tip}&per_page=100");
        let w: Vec<WCommit> = self.get(&path)?;
        let ok = |s: &str| check_sha(s).map_err(|_| malformed(&path));
        w.into_iter()
            .map(|c| -> Result<Commit, ApiError> {
                ok(&c.sha)?;
                c.parents.iter().try_for_each(|p| ok(&p.sha))?;
                Ok(Commit {
                    sha: c.sha,
                    parents: c.parents.into_iter().map(|p| p.sha).collect(),
                    date: c.commit.committer.date,
                })
            })
            .collect()
    }

    fn compare(&self, r: &str, base: &str, head: &str) -> Result<Option<Vec<ChangedFile>>, ApiError> {
        check_sha(base)?;
        check_sha(head)?;
        let path = format!("/repos/{r}/compare/{base}...{head}?per_page=100");
        let (v, full) = self.pages(&path, 3, |p: WFiles| (p.files, None))?;
        Ok(if full { None } else { Some(v) })
    }

    fn post_status(
        &self,
        r: &str,
        sha: &str,
        context: &str,
        state: &str,
        description: &str,
        url: Option<&str>,
    ) -> Result<(), ApiError> {
        check_sha(sha)?;
        let mut body = serde_json::json!({"state": state, "context": context, "description": description});
        if let Some(u) = url {
            body["target_url"] = u.into();
        }
        self.send("POST", &format!("/repos/{r}/statuses/{sha}"), Some(&body.to_string())).map(|_| ())
    }
}

fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// A status description of at most 140 characters. The reason is shortened first; the suffix of
/// an undelivered log is always kept.
pub fn compose_description(reason: &str, delivered: bool) -> String {
    if delivered {
        return shorten(reason, 140);
    }
    format!("{}{SUFFIX}", shorten(reason, 140 - SUFFIX.chars().count()))
}
