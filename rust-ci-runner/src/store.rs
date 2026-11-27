//! Reads the four CI settings from Bitwarden with the pinned `bws` binary (FR-009 to FR-011).
//! Errors name keys and counts, never a value and never the output of `bws`.

use crate::engine::read_capped;
use std::fmt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const KEYS: [&str; 4] = ["FACTORY_CI_TOKEN", "FACTORY_CI_LOG_URL", "FACTORY_CI_LOG_USER", "FACTORY_CI_LOG_TOKEN"];

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
    let rows: Vec<serde_json::Value> = serde_json::from_str(json).map_err(|_| StoreError::Unreachable)?;
    let mut keyed = Vec::with_capacity(rows.len());
    for r in &rows {
        keyed.push((r.get("key").and_then(|k| k.as_str()).ok_or(StoreError::Unreachable)?, r));
    }
    let mut vals = Vec::with_capacity(KEYS.len());
    for key in KEYS {
        let hits: Vec<_> = keyed.iter().filter(|(k, _)| *k == key).collect();
        if hits.len() != 1 {
            return Err(StoreError::Count(key, hits.len()));
        }
        let v = hits[0].1.get("value").and_then(|v| v.as_str()).filter(|v| usable(v)).ok_or(StoreError::Unusable(key))?;
        vals.push(v.to_string());
    }
    let [token, log_url, log_user, log_token]: [String; 4] = vals.try_into().map_err(|_| StoreError::Unreachable)?;
    Ok(Secrets { token, log_url, log_user, log_token })
}

fn allowed_server(url: &str) -> bool {
    let host = url.strip_prefix("https://").map(|r| r.strip_suffix('/').unwrap_or(r));
    matches!(host, Some("vault.bitwarden.com" | "vault.bitwarden.eu"))
}

fn rejected(stderr: &str) -> bool {
    let e = stderr.to_lowercase();
    ["400", "401", "403", "invalid_client", "unauthorized", "forbidden", "invalid access token"].iter().any(|m| e.contains(m))
}

/// One call. The child gets an empty environment plus `PATH`, `HOME`, the token and the server URL.
fn attempt(bws_bin: &str, token: &str, server_url: Option<&str>, limit: Duration) -> Result<String, StoreError> {
    let mut c = Command::new(bws_bin);
    c.args(["secret", "list", "-o", "json"]).env_clear();
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for k in ["PATH", "HOME"] {
        if let Ok(v) = std::env::var(k) {
            c.env(k, v);
        }
    }
    c.env("BWS_ACCESS_TOKEN", token);
    if let Some(u) = server_url {
        c.env("BWS_SERVER_URL", u);
    }
    let mut child = c.spawn().map_err(|_| StoreError::Unreachable)?;
    let (out, err) = (child.stdout.take(), child.stderr.take());
    let end = Instant::now() + limit;
    std::thread::scope(|sc| {
        let o = sc.spawn(move || out.map(|r| read_capped(r, 16 << 20)).unwrap_or_default());
        let e = sc.spawn(move || err.map(|r| read_capped(r, 64 << 10)).unwrap_or_default());
        let status = loop {
            match child.try_wait() {
                Ok(Some(st)) => break Some(st),
                Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(25)),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        };
        let (o, e) = (o.join().unwrap_or_default(), e.join().unwrap_or_default());
        match status {
            Some(st) if st.success() => Ok(o),
            Some(_) if rejected(&e) => Err(StoreError::Rejected),
            _ => Err(StoreError::Unreachable),
        }
    })
}

/// Runs `bws secret list -o json` (no project argument: one project per machine account). `limit` is
/// per attempt. A rejected token is final; any other failure is tried once more.
pub fn fetch(bws_bin: &str, token: &str, server_url: Option<&str>, limit: Duration) -> Result<Secrets, StoreError> {
    if server_url.is_some_and(|u| !allowed_server(u)) {
        return Err(StoreError::Unreachable);
    }
    for _ in 0..2 {
        match attempt(bws_bin, token, server_url, limit) {
            Ok(out) => return parse_list(&out),
            Err(StoreError::Rejected) => return Err(StoreError::Rejected),
            Err(_) => {}
        }
    }
    Err(StoreError::Unreachable)
}
