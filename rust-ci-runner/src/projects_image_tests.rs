//! 012 T008, T014 (test-first, plan step S2). Targets `projects::{find, discover, owns, project_path_ok}` for
//! the project shape `docker/<name>` (data-model.md "Image project"; FR-001, FR-002, FR-004; I9).
//!
//! Not yet registered: add `#[cfg(test)] mod projects_image_tests;` to `lib.rs`, then remove each
//! `#[ignore = "test-first"]` as its test passes (T008, T014). `project_path_ok(path: &str) -> bool` is assumed:
//! the plan names the function and its use in `job.rs::target()` but not its signature. `Found.valid` for a
//! `docker/<name>` project follows `image::name_ok` (2 to 64 characters, kebab case), not `folder_name_ok`.

use crate::github::{
    Api, ApiError, Branch, ChangedFile, Commit, Repo, StatusEntry, Tree, TreeEntry,
};
use crate::projects::{discover, find, owns, project_path_ok};
use std::collections::HashMap;
use std::sync::Mutex;

const TOML: &str = ".factory-ci.toml";

fn e(path: &str, kind: &str) -> TreeEntry {
    TreeEntry {
        path: path.to_string(),
        kind: kind.to_string(),
    }
}

fn found(tree: &[TreeEntry]) -> Vec<(String, bool)> {
    let mut v: Vec<_> = find(tree).into_iter().map(|f| (f.path, f.valid)).collect();
    v.sort();
    v
}

fn pair(path: &str, valid: bool) -> (String, bool) {
    (path.to_string(), valid)
}

fn set(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn find_docker_projects() {
    // 012 T008; S2; FR-001: a folder under `docker/` with `.factory-ci.toml`
    let t = [
        e("docker", "tree"),
        e("docker/hello-static", "tree"),
        e(&format!("docker/hello-static/{TOML}"), "blob"),
        e("docker/hello-static/Dockerfile", "blob"),
    ];
    assert_eq!(found(&t), [pair("docker/hello-static", true)]);
}

#[test]
fn a_folder_with_a_dockerfile_only_is_a_project() {
    // 012 T008; data-model.md "A folder with `.factory-ci.toml` or `Dockerfile` is a project"
    let t = [
        e("docker", "tree"),
        e("docker/x1", "tree"),
        e("docker/x1/Dockerfile", "blob"),
    ];
    assert_eq!(found(&t), [pair("docker/x1", true)]);
}

#[test]
fn a_folder_with_neither_file_is_ignored() {
    // 012 T008; spec A12
    let t = [
        e("docker", "tree"),
        e("docker/x1", "tree"),
        e("docker/x1/README.md", "blob"),
        e("docker/x2", "tree"),
    ];
    assert!(found(&t).is_empty());
}

#[test]
fn deeper_paths_a_docker_file_and_a_submodule_are_ignored() {
    // 012 T008; I9: only `docker/<name>` counts, never `docker/<a>/<b>`
    let deeper = [
        e("docker", "tree"),
        e("docker/x1", "tree"),
        e("docker/x1/y", "tree"),
        e(&format!("docker/x1/y/{TOML}"), "blob"),
        e("docker/x1/y/Dockerfile", "blob"),
    ];
    assert!(found(&deeper).is_empty());
    let file = [e("docker", "blob"), e("docker/x1/Dockerfile", "blob")];
    assert!(found(&file).is_empty(), "`docker` is a file, not a folder");
    let module = [
        e("docker", "tree"),
        e("docker/x1", "commit"),
        e(&format!("docker/x1/{TOML}"), "blob"),
    ];
    assert!(found(&module).is_empty(), "a submodule is no project");
}

#[test]
fn docker_names_follow_the_image_name_rule() {
    // 012 T008; FR-002: `^[a-z0-9]+(-[a-z0-9]+)*$`, length 2 to 64, flagged not dropped
    let long64 = "a".repeat(64);
    let long65 = "a".repeat(65);
    let names: Vec<(String, bool)> = [
        ("ab", true),
        ("a1", true),
        ("hello-static", true),
        (long64.as_str(), true),
        ("a", false),
        (long65.as_str(), false),
        ("Hello", false),
        ("a_b", false),
        ("a.b", false),
        ("a--b", false),
        ("-ab", false),
        ("ab-", false),
    ]
    .iter()
    .map(|(n, ok)| (n.to_string(), *ok))
    .collect();
    for (name, ok) in names {
        let t = [
            e("docker", "tree"),
            e(&format!("docker/{name}"), "tree"),
            e(&format!("docker/{name}/{TOML}"), "blob"),
        ];
        assert_eq!(found(&t), [pair(&format!("docker/{name}"), ok)], "{name}");
    }
}

#[test]
fn root_a_folder_and_docker_projects_live_together() {
    // 012 T008; I9: the shapes `.`, `<folder>` and `docker/<name>` and nothing else
    let t = [
        e(TOML, "blob"),
        e("svc", "tree"),
        e(&format!("svc/{TOML}"), "blob"),
        e("docker", "tree"),
        e("docker/ab", "tree"),
        e(&format!("docker/ab/{TOML}"), "blob"),
        e("docker/cd", "tree"),
        e("docker/cd/Dockerfile", "blob"),
    ];
    assert_eq!(
        found(&t),
        [
            pair(".", true),
            pair("docker/ab", true),
            pair("docker/cd", true),
            pair("svc", true)
        ]
    );
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, m: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % m
    }
}

#[test]
fn find_never_returns_a_path_of_another_shape() {
    // 012 T008; I9 over generated trees
    let tops = ["docker", "svc", "a", "docker2"];
    let mids = ["ab", "x", "Up", "deep"];
    let leaves = [TOML, "Dockerfile", "README.md"];
    let mut rng = Lcg(11);
    for _ in 0..400 {
        let mut t: Vec<TreeEntry> = Vec::new();
        for top in tops {
            if rng.next(3) > 0 {
                t.push(e(top, if rng.next(5) == 0 { "blob" } else { "tree" }));
            }
            for mid in mids {
                if rng.next(3) == 0 {
                    t.push(e(
                        &format!("{top}/{mid}"),
                        if rng.next(5) == 0 { "commit" } else { "tree" },
                    ));
                }
                for leaf in leaves {
                    if rng.next(4) == 0 {
                        t.push(e(&format!("{top}/{mid}/{leaf}"), "blob"));
                    }
                    if rng.next(8) == 0 {
                        t.push(e(&format!("{top}/{mid}/z/{leaf}"), "blob"));
                    }
                }
            }
            if rng.next(4) == 0 {
                t.push(e(&format!("{top}/{TOML}"), "blob"));
            }
        }
        if rng.next(3) == 0 {
            t.push(e(TOML, "blob"));
        }
        for f in find(&t) {
            let segs: Vec<&str> = f.path.split('/').collect();
            let ok = f.path == "." || segs.len() == 1 || (segs.len() == 2 && segs[0] == "docker");
            assert!(ok, "{} from {t:?}", f.path);
            if segs.len() == 2 {
                assert!(
                    t.iter().any(|x| x.path == "docker" && x.kind == "tree"),
                    "{} needs a `docker` folder: {t:?}",
                    f.path
                );
            }
        }
    }
}

#[test]
fn owns_prefix_boundary() {
    // 012 T008; S2: `docker/x` owns `docker/x/...`, not `docker/xy/...`
    let s = set(&[".", "docker/x", "docker/xy"]);
    assert!(owns("docker/x", "docker/x/Dockerfile", &s));
    assert!(owns("docker/x", "docker/x/sub/file", &s));
    assert!(!owns("docker/x", "docker/xy/Dockerfile", &s));
    assert!(
        !owns("docker/x", "docker/x", &s),
        "the folder itself is not a file of it"
    );
    assert!(owns("docker/xy", "docker/xy/Dockerfile", &s));
    assert!(!owns("docker/xy", "docker/x/Dockerfile", &s));
    assert!(
        !owns(".", "docker/x/Dockerfile", &s),
        "the root does not own an image folder"
    );
    assert!(
        owns(".", "docker/other/Dockerfile", &s),
        "a file of an unlisted folder falls to the root"
    );
    assert!(owns(".", "README.md", &s));
    let no_root = set(&["docker/x"]);
    assert!(!owns("docker/x", "docker/README.md", &no_root));
}

#[test]
fn a_removed_image_folder_falls_to_the_root() {
    // 012 T008; FR-019 at the ownership level
    let gone = set(&["."]);
    assert!(owns(".", "docker/x/Dockerfile", &gone));
}

// ---- discovery over a truncated tree ----

/// An `Api` that answers `tree` from a table keyed by object sha. Unlike `FakeApi::set_tree_truncated`, a
/// folder's entries carry their own object shas, which one more level under `docker` needs.
struct Trees {
    root: String,
    by_sha: HashMap<String, Tree>,
    calls: Mutex<Vec<String>>,
}

fn tree(entries: &[(&str, &str, Option<&str>)]) -> Tree {
    let mut t = Tree::default();
    for (p, k, sha) in entries {
        t.entries.push(e(p, k));
        if let Some(s) = sha {
            t.shas.insert(p.to_string(), s.to_string());
        }
    }
    t
}

impl Api for Trees {
    fn user(&self) -> Result<String, ApiError> {
        Ok("bot".to_string())
    }

    fn repo(&self, _r: &str) -> Result<Repo, ApiError> {
        Err(ApiError::Net("unused".to_string()))
    }

    fn branches(&self, _r: &str) -> Result<Vec<Branch>, ApiError> {
        Ok(vec![])
    }

    fn tree(&self, _r: &str, sha: &str, recursive: bool) -> Result<Tree, ApiError> {
        self.calls.lock().unwrap().push(sha.to_string());
        if recursive {
            return Ok(Tree {
                truncated: true,
                ..Tree::default()
            });
        }
        let key = if sha == self.root { "root" } else { sha };
        Ok(self.by_sha.get(key).cloned().unwrap_or_default())
    }

    fn status(&self, _r: &str, _sha: &str) -> Result<Vec<StatusEntry>, ApiError> {
        Ok(vec![])
    }

    fn commits(&self, _r: &str, _tip: &str) -> Result<Vec<Commit>, ApiError> {
        Ok(vec![])
    }

    fn compare(&self, _r: &str, _b: &str, _h: &str) -> Result<Option<Vec<ChangedFile>>, ApiError> {
        Ok(Some(vec![]))
    }

    fn post_status(
        &self,
        _r: &str,
        _s: &str,
        _c: &str,
        _st: &str,
        _d: &str,
        _u: Option<&str>,
    ) -> Result<(), ApiError> {
        Ok(())
    }
}

#[test]
fn discover_truncated_tree_reads_docker() {
    // 012 T008; plan failure-mode table "Discovery": a truncated tree reads one more level under `docker`
    let mut by_sha = HashMap::new();
    by_sha.insert(
        "root".to_string(),
        tree(&[
            (TOML, "blob", Some("b0")),
            ("docker", "tree", Some("tD")),
            ("src", "tree", Some("tS")),
        ]),
    );
    by_sha.insert(
        "tD".to_string(),
        tree(&[
            ("hello-static", "tree", Some("tH")),
            ("plain", "tree", Some("tP")),
            ("README.md", "blob", None),
        ]),
    );
    by_sha.insert(
        "tH".to_string(),
        tree(&[("Dockerfile", "blob", None), (TOML, "blob", None)]),
    );
    by_sha.insert("tP".to_string(), tree(&[("README.md", "blob", None)]));
    by_sha.insert("tS".to_string(), tree(&[("main.rs", "blob", None)]));
    let api = Trees {
        root: "c0".to_string(),
        by_sha,
        calls: Mutex::new(vec![]),
    };
    let mut got: Vec<_> = discover(&api, "o/r", "c0")
        .unwrap()
        .into_iter()
        .map(|f| (f.path, f.valid))
        .collect();
    got.sort();
    assert_eq!(got, [pair(".", true), pair("docker/hello-static", true)]);
    let calls = api.calls.lock().unwrap().clone();
    for want in ["tD", "tH", "tP"] {
        assert!(
            calls.iter().any(|c| c == want),
            "{want} was not read: {calls:?}"
        );
    }
    assert_eq!(calls.iter().filter(|c| *c == "tS").count(), 1, "{calls:?}");
}

// ---- the job boundary ----

#[test]
fn project_path_ok_accepts_the_three_shapes_and_nothing_else() {
    // 012 T014; S2: `target()` accepts `docker/hello-static` and still rejects `a/b`, `docker/Hello`, `../x`
    for ok in [
        ".",
        "svc",
        "a.b_c-d",
        "docker/hello-static",
        "docker/ab",
        "docker/a1-b2",
    ] {
        assert!(project_path_ok(ok), "{ok}");
    }
    let long65 = format!("docker/{}", "a".repeat(65));
    for bad in [
        "",
        "..",
        "../x",
        "a/b",
        "a/b/c",
        "docker/Hello",
        "docker/a",
        "docker",
        "docker/",
        "docker//x",
        "docker/x/y",
        "docker/../x",
        "docker/-x",
        "docker/x-",
        "docker/a_b",
        "/docker/x",
        "docker/x ",
        "-x",
        long65.as_str(),
    ] {
        assert!(!project_path_ok(bad), "{bad:?}");
    }
}

#[test]
fn docker_itself_is_never_a_project() {
    // the `docker` parent folder with its own `.factory-ci.toml` makes nothing
    let t = [e("docker", "tree"), e(&format!("docker/{TOML}"), "blob")];
    assert!(found(&t).is_empty());
}
