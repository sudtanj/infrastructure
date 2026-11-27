//! Project discovery and ownership (data-model.md Project, FR-008).
use crate::github::{Api, ApiError, TreeEntry};

const TOML: &str = ".factory-ci.toml";

/// A project at one tip: `path` is `.` or a top-level folder name; `valid` is false when the
/// folder name breaks the `folder_name_ok` rule (the plan posts an error for it, no job).
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub path: String,
    pub valid: bool,
}

/// What the plan and the walk pass around: a path with its status context.
#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub path: String,
    pub context: String,
}

pub fn context(path: &str) -> String {
    if path == "." {
        "factory-ci".to_string()
    } else {
        format!("factory-ci/{path}")
    }
}

/// 1 to 100 chars of `[A-Za-z0-9._-]`, not `.` or `..`, not starting with `-`.
pub fn folder_name_ok(name: &str) -> bool {
    (1..=100).contains(&name.len())
        && name != "."
        && name != ".."
        && !name.starts_with('-')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Projects in a recursive tree listing: the root file, and `<folder>/.factory-ci.toml` for a
/// top-level tree entry. Submodules, files and deeper paths never count.
pub fn find(tree: &[TreeEntry]) -> Vec<Found> {
    let is_blob = |p: &str| tree.iter().any(|e| e.kind == "blob" && e.path == p);
    let mut out = Vec::new();
    if is_blob(TOML) {
        out.push(Found { path: ".".to_string(), valid: true });
    }
    for e in tree.iter().filter(|e| e.kind == "tree" && !e.path.contains('/')) {
        if is_blob(&format!("{}/{TOML}", e.path)) {
            out.push(Found { path: e.path.clone(), valid: folder_name_ok(&e.path) });
        }
    }
    out
}

/// `find` on the tree of `sha`. When the recursive answer is truncated, reads the root tree and
/// each top-level folder with one non-recursive call (a submodule has no entry to follow).
pub fn discover(api: &dyn Api, repo: &str, sha: &str) -> Result<Vec<Found>, ApiError> {
    let t = api.tree(repo, sha, true)?;
    if !t.truncated {
        return Ok(find(&t.entries));
    }
    let root = api.tree(repo, sha, false)?;
    let mut entries = root.entries.clone();
    for e in root.entries.iter().filter(|e| e.kind == "tree") {
        let Some(obj) = root.shas.get(&e.path) else { continue };
        for s in api.tree(repo, obj, false)?.entries {
            entries.push(TreeEntry { path: format!("{}/{}", e.path, s.path), kind: s.kind });
        }
    }
    Ok(find(&entries))
}

/// A folder project owns `<folder>/...`; the root owns what no folder of `set` owns. The match
/// is on the whole first segment, so `foo/` does not own `foobar/x`.
pub fn owns(project: &str, file: &str, set: &[String]) -> bool {
    let under = |p: &str| file.strip_prefix(p).is_some_and(|r| r.starts_with('/'));
    if project == "." {
        !set.iter().any(|p| p != "." && under(p))
    } else {
        under(project)
    }
}
