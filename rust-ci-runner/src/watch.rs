//! The watch list file (`contracts/watch-list.md`, `data-model.md WatchEntry`): parsed and
//! validated completely, every problem reported at once.

use crate::pipeline::valid_secret_name;
use toml::{Table, Value};

pub const MAX_BYTES: usize = 64 * 1024;
const MAX_ENTRIES: usize = 100;
const MAX_LIST: usize = 20;
const ENTRY_KEYS: [&str; 6] = ["repo", "id", "branches", "secret_branches", "secrets", "allow_docker"];

#[derive(Debug, Clone, PartialEq)]
pub struct WatchEntry {
    pub repo: String,
    pub id: Option<u64>,
    pub branches: Vec<String>,
    /// `None`: the repo's default branch, known only after the repo lookup.
    pub secret_branches: Option<Vec<String>>,
    pub secrets: Vec<String>,
    pub allow_docker: bool,
}

impl WatchEntry {
    pub fn matches_branch(&self, name: &str) -> bool {
        self.branches.iter().any(|g| glob_match(g, name))
    }
}

/// `*` matches any run of bytes including `/`, `?` one byte. Two-pointer with one backtrack
/// point, so a hostile pattern costs at most pattern length times name length.
pub fn glob_match(glob: &str, name: &str) -> bool {
    let (g, n) = (glob.as_bytes(), name.as_bytes());
    let (mut gi, mut ni) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        if gi < g.len() && g[gi] == b'*' {
            star = Some((gi, ni));
            gi += 1;
        } else if gi < g.len() && (g[gi] == b'?' || g[gi] == n[ni]) {
            gi += 1;
            ni += 1;
        } else if let Some((sg, sn)) = star {
            gi = sg + 1;
            ni = sn + 1;
            star = Some((sg, sn + 1));
        } else {
            return false;
        }
    }
    g[gi..].iter().all(|&b| b == b'*')
}

fn valid_repo(r: &str) -> bool {
    let Some((owner, name)) = r.split_once('/') else { return false };
    let owner_ok = (1..=39).contains(&owner.len())
        && owner.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !owner.starts_with('-')
        && !owner.ends_with('-');
    let name_ok = (1..=100).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
        && name != "."
        && !name.contains("..");
    owner_ok && name_ok
}

fn glob_problem(g: &str) -> Option<String> {
    if g.is_empty() || g.len() > 100 {
        return Some("length must be 1 to 100".to_string());
    }
    if g.contains("..") {
        return Some("unsupported sequence ..".to_string());
    }
    let bad = g.chars().find(|c| !c.is_ascii_graphic() || "\\[]{}!^~:".contains(*c))?;
    Some(format!("unsupported character {}", bad.escape_debug()))
}

fn strings(v: &Value, key: &str, label: &str, bad: &mut Vec<String>) -> Vec<String> {
    let Some(a) = v.as_array() else {
        bad.push(format!("{label}: {key} must be a list of strings"));
        return Vec::new();
    };
    if a.len() > MAX_LIST {
        bad.push(format!("{label}: {key} has more than {MAX_LIST} items"));
    }
    a.iter()
        .filter_map(|x| {
            let s = x.as_str().map(str::to_string);
            if s.is_none() {
                bad.push(format!("{label}: {key} must be a list of strings"));
            }
            s
        })
        .collect()
}

fn globs(v: &Value, key: &str, label: &str, bad: &mut Vec<String>) -> Vec<String> {
    let list = strings(v, key, label, bad);
    for g in &list {
        if let Some(why) = glob_problem(g) {
            bad.push(format!("{label}: malformed glob {g:?} ({why})"));
        }
    }
    list
}

pub fn parse(text: &str) -> Result<Vec<WatchEntry>, Vec<String>> {
    if text.len() > MAX_BYTES {
        return Err(vec![format!("file: larger than {} KB", MAX_BYTES / 1024)]);
    }
    let root: Table = text.parse().map_err(|e: toml::de::Error| vec![format!("file: {}", e.message().replace('\n', " "))])?;
    let mut bad = Vec::new();
    for k in root.keys().filter(|k| *k != "watch") {
        bad.push(format!("file: unknown key {k:?}"));
    }
    let tables = match root.get("watch") {
        None => &[][..],
        Some(Value::Array(a)) => a.as_slice(),
        Some(_) => return Err(vec!["file: watch must be a list of [[watch]] tables".to_string()]),
    };
    if tables.len() > MAX_ENTRIES {
        bad.push(format!("file: more than {MAX_ENTRIES} entries"));
    }
    let mut out: Vec<WatchEntry> = Vec::new();
    let mut seen: Vec<(String, usize)> = Vec::new();
    for (i, t) in tables.iter().enumerate() {
        let n = i + 1;
        let Some(t) = t.as_table() else {
            bad.push(format!("entry {n}: must be a table"));
            continue;
        };
        let repo = t.get("repo").and_then(Value::as_str).unwrap_or("");
        let label = if valid_repo(repo) { repo.to_string() } else { format!("entry {n}") };
        for k in t.keys().filter(|k| !ENTRY_KEYS.contains(&k.as_str())) {
            bad.push(format!("{label}: unknown key {k:?}"));
        }
        match t.get("repo") {
            None => bad.push(format!("{label}: missing repo")),
            Some(v) if v.as_str().is_none() => bad.push(format!("{label}: repo must be a string")),
            Some(_) if label != repo => bad.push(format!("{label}: repo {repo:?} is not owner/name")),
            Some(_) => {}
        }
        if let Some((_, first)) = seen.iter().find(|(r, _)| r.eq_ignore_ascii_case(repo)).filter(|_| label == repo) {
            bad.push(format!("{label}: duplicate of entry {first}"));
        } else if label == repo {
            seen.push((repo.to_string(), n));
        }
        let id = match t.get("id") {
            None => None,
            Some(Value::Integer(v)) if *v >= 0 => Some(*v as u64),
            Some(v) => {
                let shown = v.as_str().map_or_else(|| v.to_string(), |s| format!("{s:?}"));
                bad.push(format!("{label}: id {shown} is not a number"));
                None
            }
        };
        let branches = match t.get("branches") {
            Some(v) => globs(v, "branches", &label, &mut bad),
            None => vec!["*".to_string()],
        };
        let secret_branches = t.get("secret_branches").map(|v| globs(v, "secret_branches", &label, &mut bad));
        let secrets = t.get("secrets").map_or_else(Vec::new, |v| strings(v, "secrets", &label, &mut bad));
        for (j, s) in secrets.iter().enumerate() {
            if !valid_secret_name(s) {
                bad.push(format!("{label}: secret {s:?} is reserved or malformed"));
            } else if secrets[..j].contains(s) {
                bad.push(format!("{label}: secret {s:?} is listed twice"));
            }
        }
        let allow_docker = match t.get("allow_docker") {
            None => false,
            Some(Value::Boolean(b)) => *b,
            Some(_) => {
                bad.push(format!("{label}: allow_docker must be true or false"));
                false
            }
        };
        out.push(WatchEntry { repo: repo.to_string(), id, branches, secret_branches, secrets, allow_docker });
    }
    if bad.is_empty() {
        Ok(out)
    } else {
        Err(bad)
    }
}
