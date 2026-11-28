//! 010 T007. Facts about committed files: FR-014, FR-016 and, from T027, FR-017 (the README sentence).
//! These tests read `../.github` and `../README.md`, so they need the whole repository checkout. They run in
//! the selftest (`--test '*'`), not in the `rust-ci-runner` pipeline step, which mounts only this directory
//! (do not add this file to the integration command of `rust-ci-runner/.factory-ci.toml`).

use rust_ci_runner::watch::{parse, WatchEntry};
use std::path::PathBuf;

fn root() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the project has a parent directory")
        .to_path_buf();
    assert!(
        dir.join(".github").is_dir(),
        "repo_files needs the full repository checkout: {} has no .github",
        dir.display()
    );
    dir
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("cannot read {rel}: {e}"))
}

fn entries() -> Vec<WatchEntry> {
    parse(&read(".github/factory-ci-repos.toml")).unwrap_or_else(|p| panic!("watch list: {p:?}"))
}

/// Secret names that appear in more than one entry, each once.
fn repeated(list: &[WatchEntry]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, e) in list.iter().enumerate() {
        for name in &e.secrets {
            let elsewhere = list
                .iter()
                .enumerate()
                .any(|(j, o)| j != i && o.secrets.contains(name));
            if elsewhere && !out.contains(name) {
                out.push(name.clone());
            }
        }
    }
    out
}

#[test]
fn no_secret_name_in_two_committed_entries() {
    let r = repeated(&entries());
    assert!(r.is_empty(), "shared across entries: {r:?}");
}

#[test]
fn the_repeat_check_finds_a_name_in_two_entries() {
    // the check above would pass for any input if `repeated` never found anything
    let two = "[[watch]]\nrepo = \"a/b\"\nid = 1\nbranches = [\"main\"]\nsecrets = [\"X_KEY\"]\n\n\
               [[watch]]\nrepo = \"a/c\"\nid = 2\nbranches = [\"main\"]\nsecrets = [\"X_KEY\", \"Y_KEY\"]\n\n\
               [[watch]]\nrepo = \"a/d\"\nid = 3\nbranches = [\"main\"]\nsecrets = [\"Z_KEY\"]\n";
    assert_eq!(repeated(&parse(two).unwrap()), ["X_KEY"]);
    let apart = two.replace("\"X_KEY\", ", "");
    assert!(repeated(&parse(&apart).unwrap()).is_empty());
    // case is part of the name: two keys that differ in case are two keys
    let cased = two.replace("[\"X_KEY\", \"Y_KEY\"]", "[\"x_key\"]");
    assert!(repeated(&parse(&cased).unwrap()).is_empty());
}

#[test]
fn infrastructure_entry_has_no_secrets() {
    for e in entries()
        .iter()
        .filter(|e| e.repo.eq_ignore_ascii_case("sudtanj/infrastructure"))
    {
        assert!(e.secrets.is_empty(), "sudtanj/infrastructure lists {:?}", e.secrets);
    }
}

fn package_version(manifest: &str) -> String {
    let t: toml::Table = manifest.parse().expect("Cargo.toml is TOML");
    t["package"]["version"]
        .as_str()
        .expect("package.version is a string")
        .to_string()
}

fn locked_version(lock: &str) -> String {
    let t: toml::Table = lock.parse().expect("Cargo.lock is TOML");
    let mine: Vec<&toml::Value> = t["package"]
        .as_array()
        .expect("Cargo.lock has packages")
        .iter()
        .filter(|p| p["name"].as_str() == Some("rust-ci-runner"))
        .collect();
    assert_eq!(mine.len(), 1, "Cargo.lock has {} rust-ci-runner entries", mine.len());
    mine[0]["version"].as_str().expect("version is a string").to_string()
}

#[test]
fn cargo_toml_and_lock_versions_match() {
    // `cargo --locked` fails when they differ, and the release workflow compares the tag with Cargo.toml
    let (m, l) = (
        package_version(&read("rust-ci-runner/Cargo.toml")),
        locked_version(&read("rust-ci-runner/Cargo.lock")),
    );
    assert_eq!(m, l, "Cargo.toml says {m}, Cargo.lock says {l}");
    let parts: Vec<&str> = m.split('.').collect();
    assert!(
        parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
        "{m} is not X.Y.Z"
    );
}

#[test]
fn the_version_check_reads_the_right_entry() {
    let lock = "[[package]]\nname = \"other\"\nversion = \"9.9.9\"\n\n\
                [[package]]\nname = \"rust-ci-runner\"\nversion = \"0.3.0\"\n";
    assert_eq!(locked_version(lock), "0.3.0");
    assert_eq!(package_version("[package]\nname = \"x\"\nversion = \"1.2.3\"\n"), "1.2.3");
}

// T027 (FR-017) adds the README text. Until then the sentence is still there: test-first.
#[test]
#[ignore = "test-first"]
fn readme_has_no_stale_secrets_sentence() {
    let text = read("README.md");
    assert!(
        !text.contains("Steps get no pipeline secrets here"),
        "README.md still says steps get no pipeline secrets"
    );
}
