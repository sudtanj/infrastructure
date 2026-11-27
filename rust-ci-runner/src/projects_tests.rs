//! T013. Targets `projects::{find, discover, owns, folder_name_ok, context}` and `Found`
//! (data-model.md Project). FR-008; AS 3.1, 3.4. I6.

use crate::fake::FakeApi;
use crate::github::TreeEntry;
use crate::projects::{context, discover, find, folder_name_ok, owns};

const TOML: &str = ".factory-ci.toml";

fn e(path: &str, kind: &str) -> TreeEntry {
    TreeEntry { path: path.to_string(), kind: kind.to_string() }
}

fn paths(tree: &[TreeEntry]) -> Vec<(String, bool)> {
    let mut v: Vec<_> = find(tree).into_iter().map(|f| (f.path, f.valid)).collect();
    v.sort();
    v
}

#[test]
fn root_and_two_folders_give_three_contexts() {
    // AS 3.1
    let t = [
        e(TOML, "blob"),
        e("a", "tree"),
        e(&format!("a/{TOML}"), "blob"),
        e("b", "tree"),
        e(&format!("b/{TOML}"), "blob"),
        e("README.md", "blob"),
    ];
    let found = paths(&t);
    assert_eq!(found, [(".".into(), true), ("a".into(), true), ("b".into(), true)]);
    assert_eq!(context("."), "factory-ci");
    assert_eq!(context("a"), "factory-ci/a");
    assert_eq!(context("b"), "factory-ci/b");
}

#[test]
fn no_root_toml_means_no_root_project() {
    let t = [e("a", "tree"), e(&format!("a/{TOML}"), "blob")];
    assert_eq!(paths(&t), [("a".to_string(), true)]);
    assert!(paths(&[e("src", "tree"), e("src/main.rs", "blob")]).is_empty());
}

#[test]
fn nested_toml_is_not_a_project() {
    let t = [e("a", "tree"), e("a/b", "tree"), e(&format!("a/b/{TOML}"), "blob")];
    assert!(paths(&t).is_empty());
}

#[test]
fn submodule_or_file_named_like_a_folder_is_ignored() {
    // data-model.md: submodules and symlinked folders are not projects
    let t = [
        e("sub", "commit"),
        e(&format!("sub/{TOML}"), "blob"),
        e("file", "blob"),
        e(&format!("file/{TOML}"), "blob"),
    ];
    assert!(paths(&t).is_empty(), "{:?}", paths(&t));
}

#[test]
fn a_toml_blob_of_any_mode_counts_as_a_project() {
    // A symlinked .factory-ci.toml is still a blob in the tree; the job refuses it later.
    assert_eq!(paths(&[e(TOML, "blob")]), [(".".to_string(), true)]);
}

#[test]
fn invalid_folder_names_are_flagged_not_dropped() {
    // FR-008: they get an error status from the plan and no job
    let long = "x".repeat(101);
    for name in ["a b", "-x", "..", long.as_str()] {
        let t = [e(name, "tree"), e(&format!("{name}/{TOML}"), "blob")];
        let found = paths(&t);
        assert_eq!(found, [(name.to_string(), false)], "{name:?}");
        assert!(!folder_name_ok(name));
    }
}

#[test]
fn dot_folders_and_odd_but_legal_names_are_valid() {
    for name in [".github", "go-invoices", "a.b_c", "A1"] {
        assert!(folder_name_ok(name), "{name}");
    }
    for name in ["", ".", "..", "-a", "a/b", "a b", "é"] {
        assert!(!folder_name_ok(name), "{name:?}");
    }
    assert!(folder_name_ok(&"x".repeat(100)));
}

fn set(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn ownership_matches_whole_first_segment() {
    let s = set(&[".", "foo"]);
    assert!(owns("foo", "foo/x", &s));
    assert!(!owns("foo", "foobar/x", &s), "foo/ must not own foobar/x");
    assert!(owns(".", "foobar/x", &s));
    assert!(!owns(".", "foo/x", &s));
    assert!(owns(".", "README.md", &s));
}

#[test]
fn a_removed_folder_falls_to_the_root() {
    // AS 3.4
    assert!(owns(".", "a/x", &set(&[".", "b"])));
    assert!(!owns(".", "a/x", &set(&[".", "a"])));
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, m: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % m
    }
}

#[test]
fn every_path_has_exactly_one_owner_or_none_without_a_root() {
    // I6
    let names = ["a", "ab", "b", "foo", "foobar", ".github"];
    let mut rng = Lcg(7);
    for _ in 0..300 {
        let mut s: Vec<String> = names.iter().filter(|_| rng.next(2) == 0).map(|n| n.to_string()).collect();
        let root = rng.next(2) == 0;
        if root {
            s.push(".".into());
        }
        let top = ["a", "ab", "b", "foo", "foobar", ".github", "zzz", "foo.x"][rng.next(8) as usize];
        let path = if rng.next(4) == 0 { top.to_string() } else { format!("{top}/{}/f", rng.next(3)) };
        let owners = s.iter().filter(|p| owns(p, &path, &s)).count();
        // a bare top-level file (no slash) can only belong to the root
        assert_eq!(owners, if root || s.iter().any(|p| p != "." && path.starts_with(&format!("{p}/"))) { 1 } else { 0 }, "{s:?} {path}");
    }
}

#[test]
fn truncated_tree_falls_back_to_root_tree_plus_one_call_per_folder() {
    // FR-008 (spec edge case), contracts/github-api.md
    let api = FakeApi::new("ci-bot", 1_000_000);
    api.set_tree_truncated(
        "o/r",
        "c0",
        &[(TOML, "blob", "b0"), ("a", "tree", "t1"), ("b", "tree", "t2"), ("src", "tree", "t3"), ("m", "commit", "s4")],
        &[("t1", &[(TOML, "blob")]), ("t2", &[("x", "blob")]), ("t3", &[("y", "blob")])],
    );
    let found = discover(&api, "o/r", "c0").unwrap();
    let mut got: Vec<_> = found.into_iter().map(|f| (f.path, f.valid)).collect();
    got.sort();
    assert_eq!(got, [(".".to_string(), true), ("a".to_string(), true)]);
    let calls = api.calls();
    for want in ["tree o/r c0", "tree o/r t1", "tree o/r t2", "tree o/r t3"] {
        assert!(calls.iter().any(|c| c.starts_with(want)), "{want} not in {calls:?}");
    }
    assert!(!calls.iter().any(|c| c.starts_with("tree o/r s4")), "submodule entries are not followed");
}

#[test]
fn untruncated_tree_costs_one_call() {
    let api = FakeApi::new("ci-bot", 1_000_000);
    api.set_tree("o/r", "c0", &[(TOML, "blob"), ("a", "tree"), (&format!("a/{TOML}"), "blob")]);
    assert_eq!(discover(&api, "o/r", "c0").unwrap().len(), 2);
    assert_eq!(api.reads(), 1);
}
