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
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// The shapes a job may name: `.`, a top-level folder, or `docker/<name>` with a valid image name.
pub fn project_path_ok(path: &str) -> bool {
    path == "."
        || folder_name_ok(path)
        || path
            .strip_prefix("docker/")
            .is_some_and(crate::image::name_ok)
}

/// Projects in a recursive tree listing: the root file, `<folder>/.factory-ci.toml` for a
/// top-level tree entry, and `docker/<name>` holding `.factory-ci.toml` or `Dockerfile`.
/// Submodules, files and deeper paths never count.
pub fn find(tree: &[TreeEntry]) -> Vec<Found> {
    let is_blob = |p: &str| tree.iter().any(|e| e.kind == "blob" && e.path == p);
    let mut out = Vec::new();
    if is_blob(TOML) {
        out.push(Found {
            path: ".".to_string(),
            valid: true,
        });
    }
    for e in tree
        .iter()
        // `docker` is the image parent, never a project itself (fail closed).
        .filter(|e| e.kind == "tree" && !e.path.contains('/') && e.path != "docker")
    {
        if is_blob(&format!("{}/{TOML}", e.path)) {
            out.push(Found {
                path: e.path.clone(),
                valid: folder_name_ok(&e.path),
            });
        }
    }
    if tree.iter().any(|e| e.kind == "tree" && e.path == "docker") {
        for e in tree.iter().filter(|e| {
            e.kind == "tree"
                && e.path
                    .strip_prefix("docker/")
                    .is_some_and(|n| !n.contains('/'))
        }) {
            if is_blob(&format!("{}/{TOML}", e.path)) || is_blob(&format!("{}/Dockerfile", e.path))
            {
                out.push(Found {
                    path: e.path.clone(),
                    valid: crate::image::name_ok(&e.path["docker/".len()..]),
                });
            }
        }
    }
    out
}

/// `find` on the tree of `sha`. When the recursive answer is truncated, reads the root tree and
/// each top-level folder with one non-recursive call (a submodule has no entry to follow), and
/// one more level under `docker`.
pub fn discover(api: &dyn Api, repo: &str, sha: &str) -> Result<Vec<Found>, ApiError> {
    let t = api.tree(repo, sha, true)?;
    if !t.truncated {
        return Ok(find(&t.entries));
    }
    let root = api.tree(repo, sha, false)?;
    let mut entries = root.entries.clone();
    for e in root.entries.iter().filter(|e| e.kind == "tree") {
        let Some(obj) = root.shas.get(&e.path) else {
            continue;
        };
        let sub = api.tree(repo, obj, false)?;
        for s in &sub.entries {
            entries.push(TreeEntry {
                path: format!("{}/{}", e.path, s.path),
                kind: s.kind.clone(),
            });
            // `docker/<name>` files sit one level further down.
            if e.path == "docker" && s.kind == "tree" {
                let Some(o) = sub.shas.get(&s.path) else {
                    continue;
                };
                for f in api.tree(repo, o, false)?.entries {
                    entries.push(TreeEntry {
                        path: format!("docker/{}/{}", s.path, f.path),
                        kind: f.kind,
                    });
                }
            }
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
