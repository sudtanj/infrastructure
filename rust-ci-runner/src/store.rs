//! Reads the four CI settings from Bitwarden with the pinned `bws` binary (FR-009 to FR-011).
//! Errors name keys and counts, never a value and never the output of `bws`.

use crate::engine::run_capped;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::process::Command;
use std::time::Duration;

pub const REQUIRED_KEYS: [&str; 4] = [
    "FACTORY_CI_TOKEN",
    "FACTORY_CI_LOG_URL",
    "FACTORY_CI_LOG_USER",
    "FACTORY_CI_LOG_TOKEN",
];

/// The four values. `Debug` prints no value.
pub struct Secrets {
    pub token: String,
    pub log_url: String,
    pub log_user: String,
    pub log_token: String,
}

impl fmt::Debug for Secrets {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Secrets").finish_non_exhaustive()
    }
}

/// `Display` is the fault text of the public log, without the `[runner] ` prefix.
#[derive(Debug, PartialEq)]
pub enum StoreError {
    /// No answer, a failed call, output that is not a secret list, or a refused server URL.
    Unreachable,
    Rejected,
    Count(&'static str, usize),
    Unusable(&'static str),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StoreError::Unreachable => write!(f, "secret store unreachable"),
            StoreError::Rejected => write!(f, "secret store rejected the access token"),
            StoreError::Count(k, n) => write!(f, "secret {k}: expected 1, found {n}"),
            StoreError::Unusable(k) => write!(f, "secret {k}: unusable value"),
        }
    }
}

fn usable(v: &str) -> bool {
    v.chars().count() >= 4 && !v.contains(['\r', '\n', '\0', '\t']) && v.trim() == v
}

/// Takes the JSON of `bws secret list -o json`. Each of the four keys must match exactly one
/// secret; other keys are ignored.
pub fn parse_list(json: &str) -> Result<Secrets, StoreError> {
    parse_list_with(json, &[]).map(|f| f.secrets)
}

/// Like `parse_list`, and keeps the rows whose whole key equals a name of `allowed`. Nothing else
/// outlives the call.
pub fn parse_list_with(json: &str, allowed: &[String]) -> Result<Fetched, StoreError> {
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|_| StoreError::Unreachable)?;
    let mut keyed = Vec::with_capacity(rows.len());
    for r in &rows {
        keyed.push((
            r.get("key")
                .and_then(|k| k.as_str())
                .ok_or(StoreError::Unreachable)?,
            r,
        ));
    }
    let mut vals = Vec::with_capacity(REQUIRED_KEYS.len());
    for key in REQUIRED_KEYS {
        let hits: Vec<_> = keyed.iter().filter(|(k, _)| *k == key).collect();
        if hits.len() != 1 {
            return Err(StoreError::Count(key, hits.len()));
        }
        let v = hits[0]
            .1
            .get("value")
            .and_then(|v| v.as_str())
            .filter(|v| usable(v))
            .ok_or(StoreError::Unusable(key))?;
        vals.push(v.to_string());
    }
    let [token, log_url, log_user, log_token]: [String; 4] =
        vals.try_into().map_err(|_| StoreError::Unreachable)?;
    let mut held: HashMap<String, Vec<String>> = HashMap::new();
    for (key, row) in &keyed {
        if allowed.iter().any(|a| a == *key) {
            // A value that is not a string is kept as "", which `usable_secret` refuses.
            let v = row.get("value").and_then(|v| v.as_str()).unwrap_or("");
            held.entry(key.to_string()).or_default().push(v.to_string());
        }
    }
    Ok(Fetched {
        secrets: Secrets {
            token,
            log_url,
            log_user,
            log_token,
        },
        held: Held(held),
    })
}

/// The four values and the rows of the allowlisted keys, from one list.
pub struct Fetched {
    pub secrets: Secrets,
    pub held: Held,
}

/// Values of the allowlisted keys, by key. Validated only when a name is taken.
#[derive(Default)]
pub struct Held(HashMap<String, Vec<String>>);

impl fmt::Debug for Held {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Held").finish_non_exhaustive()
    }
}

/// Name to value, one per requested name. `Debug` prints names only.
pub struct StepSecrets(pub(crate) BTreeMap<String, String>);

impl fmt::Debug for StepSecrets {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entries(self.0.keys()).finish()
    }
}

/// Keyed by position in the requested list, so it holds no name and no value.
#[derive(Debug)]
pub enum NamedError {
    Count { index: usize, found: usize },
    Unusable { index: usize },
}

impl fmt::Display for NamedError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NamedError::Count { index, found } => {
                write!(f, "secret #{index}: expected 1, found {found}")
            }
            NamedError::Unusable { index } => write!(f, "secret #{index}: unusable value"),
        }
    }
}

impl Held {
    pub fn take(&self, names: &[String]) -> Result<StepSecrets, NamedError> {
        let mut out = BTreeMap::new();
        for (index, name) in names.iter().enumerate() {
            let rows = self.0.get(name).map_or(&[][..], Vec::as_slice);
            let [v] = rows else {
                return Err(NamedError::Count {
                    index,
                    found: rows.len(),
                });
            };
            if !usable_secret(v) {
                return Err(NamedError::Unusable { index });
            }
            out.insert(name.clone(), v.clone());
        }
        Ok(StepSecrets(out))
    }
}

/// At most 32 KiB, not blank, no control character but `\n` and `\r\n`, no surrounding whitespace
/// on a single line, and every non-blank line at least 8 characters once trimmed.
fn usable_secret(v: &str) -> bool {
    let t = v.trim();
    v.len() <= 32768
        && !t.is_empty()
        && !v
            .replace("\r\n", "\n")
            .chars()
            .any(|c| (c.is_control() && c != '\n') || matches!(c, '\u{2028}' | '\u{2029}'))
        && (t.contains('\n') || t == v)
        && v.lines()
            .map(str::trim)
            .all(|l| l.is_empty() || l.chars().count() >= 8)
}

fn allowed_server(url: &str) -> bool {
    let host = url
        .strip_prefix("https://")
        .map(|r| r.strip_suffix('/').unwrap_or(r));
    matches!(host, Some("vault.bitwarden.com" | "vault.bitwarden.eu"))
}

fn rejected(stderr: &str) -> bool {
    let e = stderr.to_lowercase();
    // Status codes match as whole words, so a port such as 40123 does not.
    e.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|t| matches!(t, "400" | "401" | "403"))
        || [
            "invalid_client",
            "unauthorized",
            "forbidden",
            "invalid access token",
        ]
        .iter()
        .any(|m| e.contains(m))
}

/// One call. The child gets an empty environment plus `PATH`, `HOME`, the token and the server URL.
fn attempt(
    bws_bin: &str,
    token: &str,
    server_url: Option<&str>,
    limit: Duration,
) -> Result<String, StoreError> {
    let mut c = Command::new(bws_bin);
    c.args(["secret", "list", "-o", "json"]).env_clear();
    for k in ["PATH", "HOME"] {
        if let Ok(v) = std::env::var(k) {
            c.env(k, v);
        }
    }
    c.env("BWS_ACCESS_TOKEN", token);
    if let Some(u) = server_url {
        c.env("BWS_SERVER_URL", u);
    }
    match run_capped(c, limit) {
        Some((true, out, _)) => Ok(out),
        Some((false, _, err)) if rejected(&err) => Err(StoreError::Rejected),
        _ => Err(StoreError::Unreachable),
    }
}

/// Runs `bws secret list -o json` (no project argument: one project per machine account). `limit` is
/// per attempt. A rejected token is final; any other failure is tried once more.
pub fn fetch(
    bws_bin: &str,
    token: &str,
    server_url: Option<&str>,
    limit: Duration,
) -> Result<Secrets, StoreError> {
    fetch_with(bws_bin, token, server_url, limit, &[]).map(|f| f.secrets)
}

/// `fetch`, and the rows of the allowlisted keys.
pub fn fetch_with(
    bws_bin: &str,
    token: &str,
    server_url: Option<&str>,
    limit: Duration,
    allowed: &[String],
) -> Result<Fetched, StoreError> {
    if server_url.is_some_and(|u| !allowed_server(u)) {
        return Err(StoreError::Unreachable);
    }
    for _ in 0..2 {
        match attempt(bws_bin, token, server_url, limit) {
            Ok(out) => return parse_list_with(&out, allowed),
            Err(StoreError::Rejected) => return Err(StoreError::Rejected),
            Err(_) => {}
        }
    }
    Err(StoreError::Unreachable)
}
