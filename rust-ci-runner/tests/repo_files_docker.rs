//! 012 T022, plan step S10: the static check over every folder under `docker/` (FR-006).
//!
//! The selftest paths filter lacks `docker/**` (plan K4): a push that changes only `docker/` does not start it,
//! so this file runs when `rust-ci-runner/**` changes; the tick on `main` runs the same check before a publish.
//! The tests for the content of `docker/hello-static` that need no `image` module are in `repo_files.rs`.
//!
//! Like `repo_files.rs`, this file reads the whole checkout, so it runs in the selftest (`--test '*'`) and stays
//! out of the integration command of `rust-ci-runner/.factory-ci.toml`.

use rust_ci_runner::image::{check, name_ok};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the project has a parent directory")
        .to_path_buf();
    assert!(
        dir.join(".github").is_dir(),
        "repo_files_docker needs the full repository checkout: {} has no .github",
        dir.display()
    );
    dir
}

/// Every folder directly under `docker/`, by name. A plain file there (`docker/README.md`) is not an image.
fn image_folders() -> Vec<(String, PathBuf)> {
    let dir = root().join("docker");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf)> = rd
        .filter_map(|e| e.ok())
        .filter(|e| std::fs::symlink_metadata(e.path()).is_ok_and(|m| m.is_dir()))
        .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
        .collect();
    out.sort();
    out
}

fn plain_file(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file())
}

#[test]
fn docker_folders_pass_the_static_check() {
    // 012 T022; S10, FR-006: every image folder has a kebab name, the three plain files and an empty check
    for (name, path) in image_folders() {
        assert!(name_ok(&name), "docker/{name}: not a valid image name");
        for f in ["Dockerfile", "README.md", ".factory-ci.toml"] {
            assert!(
                plain_file(&path.join(f)),
                "docker/{name}/{f} is missing or not a plain file"
            );
        }
        let ids = check(&path, false);
        assert!(ids.is_empty(), "docker/{name} fails the check: {ids:?}");
    }
}

#[test]
fn hello_static_is_the_first_image() {
    // 012 T018; S3: the folder exists and is one of the folders the test above walks
    let names: Vec<String> = image_folders().into_iter().map(|(n, _)| n).collect();
    assert!(
        names.iter().any(|n| n == "hello-static"),
        "docker/ holds {names:?}"
    );
}

#[test]
fn the_walk_would_catch_a_bad_folder() {
    // 012 T022: the test above passes for an empty `docker/`; this proves `check` and `name_ok` can fail
    let tmp = std::env::temp_dir().join(format!(
        "fci-docker-walk-{}",
        rust_ci_runner::util::random_id()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    assert!(
        !check(&tmp, false).is_empty(),
        "an empty folder must fail the layout check"
    );
    assert!(!name_ok("Bad_Name") && !name_ok("a") && name_ok("ab"));
    let _ = std::fs::remove_dir_all(&tmp);
}
