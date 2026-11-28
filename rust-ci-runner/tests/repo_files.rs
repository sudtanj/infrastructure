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

const DOCKERHUB_NAMES: [&str; 2] = ["DOCKERHUB_USERNAME", "DOCKERHUB_TOKEN"];

/// Why the `sudtanj/infrastructure` entry breaks the shape 012 allows, or nothing. The shape holds before the
/// follow-up PR that turns docker on (no `secrets`) and after it (`allow_docker` and exactly the two names).
/// 010 FR-016 said "no secrets" for this entry; the frozen job script resolves a shared name only when the
/// entry lists it, so the two Docker Hub names must be there. No `secret_branches` keeps the manual `ci`
/// command from ever passing them to a step.
fn entry_problems(e: &WatchEntry) -> Vec<String> {
    let mut v = Vec::new();
    if e.branches != ["main"] {
        v.push(format!("branches is {:?}", e.branches));
    }
    if e.secret_branches.is_some() {
        v.push("secret_branches is set".to_string());
    }
    if !e.secrets.is_empty() && e.secrets != DOCKERHUB_NAMES {
        v.push(format!("secrets is {:?}", e.secrets));
    }
    if !e.secrets.is_empty() && !e.allow_docker {
        v.push("secrets without allow_docker".to_string());
    }
    v
}

#[test]
fn infrastructure_entry_lists_exactly_the_two_dockerhub_names_and_no_secret_branches() {
    // 012 T022; FR-023, 010 FR-010. Passes in both shapes: before and after the follow-up PR (T020).
    let list = entries();
    let mine: Vec<&WatchEntry> = list
        .iter()
        .filter(|e| e.repo.eq_ignore_ascii_case("sudtanj/infrastructure"))
        .collect();
    assert!(
        !mine.is_empty(),
        "the watch list has no sudtanj/infrastructure entry"
    );
    for e in mine {
        let p = entry_problems(e);
        assert!(p.is_empty(), "sudtanj/infrastructure: {p:?}");
    }
}

#[test]
fn the_entry_check_accepts_both_shapes_and_refuses_the_rest() {
    // 012 T022: the check above would pass for any input if `entry_problems` never found anything
    let base = "repo = \"sudtanj/infrastructure\"\nid = 1\n";
    let names = "secrets = [\"DOCKERHUB_USERNAME\", \"DOCKERHUB_TOKEN\"]\n";
    let ok = [
        format!("{base}branches = [\"main\"]\n"),
        format!("{base}branches = [\"main\"]\nallow_docker = true\n{names}"),
        format!("{base}branches = [\"main\"]\nallow_docker = true\n"),
    ];
    let bad = [
        format!("{base}branches = [\"main\", \"dev\"]\n"),
        format!("{base}branches = [\"*\"]\n"),
        format!("{base}\n"),
        format!("{base}branches = [\"main\"]\nsecret_branches = [\"main\"]\n"),
        format!("{base}branches = [\"main\"]\nallow_docker = true\nsecrets = [\"DOCKERHUB_TOKEN\"]\n"),
        format!("{base}branches = [\"main\"]\nallow_docker = true\nsecrets = [\"DOCKERHUB_TOKEN\", \"DOCKERHUB_USERNAME\"]\n"),
        format!("{base}branches = [\"main\"]\nallow_docker = true\nsecrets = [\"DOCKERHUB_USERNAME\", \"DOCKERHUB_TOKEN\", \"X_KEY\"]\n"),
        format!("{base}branches = [\"main\"]\n{names}"),
        format!("{base}branches = [\"main\"]\nallow_docker = false\n{names}"),
        format!("{base}branches = [\"main\"]\nsecrets = [\"NPM_TOKEN\"]\n"),
    ];
    for t in ok {
        let e = &parse(&format!("[[watch]]\n{t}")).unwrap()[0];
        assert!(entry_problems(e).is_empty(), "{t}");
    }
    for t in bad {
        let e = &parse(&format!("[[watch]]\n{t}")).unwrap()[0];
        assert!(!entry_problems(e).is_empty(), "{t}");
    }
}

/// A path the 012 change may not touch (FR-022).
fn frozen(path: &str) -> bool {
    path.starts_with(".github/workflows/") || path.starts_with(".github/scripts/")
}

#[test]
fn no_workflow_or_script_changes_against_the_base() {
    // 012 T022; FR-022, K6: `git diff --name-only <base>...HEAD`, `<base>` from FACTORY_CI_BASE or origin/main.
    // Without git or the base (a shallow checkout) it prints a skip line: that is not a pass.
    let base = std::env::var("FACTORY_CI_BASE").unwrap_or_else(|_| "origin/main".to_string());
    let out = std::process::Command::new("git")
        .args(["diff", "--name-only", &format!("{base}...HEAD")])
        .current_dir(root())
        .output();
    let out = match out {
        Ok(o) if o.status.success() => o,
        _ => {
            println!(
                "SKIPPED: git or the base {base} is not available; FR-022 is not checked here"
            );
            return;
        }
    };
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let bad: Vec<&str> = text.lines().filter(|p| frozen(p)).collect();
    assert!(
        bad.is_empty(),
        "frozen paths changed against {base}: {bad:?}"
    );
}

#[test]
fn the_frozen_path_rule_matches_the_two_directories_only() {
    // 012 T022
    for p in [
        ".github/workflows/factory-ci-tick.yml",
        ".github/scripts/factory-ci-job.sh",
        ".github/workflows/new/x.yml",
    ] {
        assert!(frozen(p), "{p}");
    }
    for p in [
        ".github/factory-ci-repos.toml",
        ".github/workflowsx/a",
        "rust-ci-runner/.github/workflows/a.yml",
        "docker/hello-static/Dockerfile",
        "README.md",
    ] {
        assert!(!frozen(p), "{p}");
    }
}

// ---- docker/hello-static (S3, T018) ----

#[test]
fn hello_static_pipeline_builds_and_saves_both_platforms() {
    // 012 T018; S3, FR-003, FR-012: the last command is the `chmod 0644` of both archives
    let t: toml::Table = read("docker/hello-static/.factory-ci.toml")
        .parse()
        .expect("the pipeline is TOML");
    let steps = t["steps"].as_table().expect("steps table");
    let mut all: Vec<String> = Vec::new();
    for (name, step) in steps {
        let cmds = step["commands"]
            .as_array()
            .unwrap_or_else(|| panic!("step {name} has no commands"));
        assert!(
            step["image"]
                .as_str()
                .is_some_and(|i| i.contains("@sha256:")),
            "step {name} image is not pinned by digest"
        );
        all.extend(cmds.iter().map(|c| c.as_str().unwrap_or("").to_string()));
    }
    let text = all.join("\n");
    for want in [
        "linux/amd64",
        "linux/arm64",
        "image-amd64.tar",
        "image-arm64.tar",
    ] {
        assert!(
            text.contains(want),
            "{want} is not in the commands:\n{text}"
        );
    }
    assert_eq!(
        all.iter().filter(|c| c.contains("docker save")).count(),
        2,
        "{text}"
    );
    let last = all.last().expect("commands");
    assert!(
        last.starts_with("chmod 0644")
            && last.contains("image-amd64.tar")
            && last.contains("image-arm64.tar"),
        "the last command is {last:?}"
    );
}

#[test]
fn hello_static_dockerfile_is_pinned_and_keeps_the_toolchain_local() {
    // 012 T018; S3, FR-028, A13: builder from the mirror by index digest, base by digest, no toolchain download
    let df = read("docker/hello-static/Dockerfile");
    assert!(
        df.contains("GOTOOLCHAIN=local"),
        "GOTOOLCHAIN=local is missing"
    );
    let from: Vec<&str> = df
        .lines()
        .filter(|l| l.trim_start().to_ascii_uppercase().starts_with("FROM "))
        .collect();
    assert!(from.len() >= 2, "{from:?}");
    assert!(
        from.iter()
            .any(|l| l.contains("mirror.gcr.io/library/golang:")),
        "the builder is not from the mirror: {from:?}"
    );
    for l in &from {
        assert!(l.contains("@sha256:"), "{l} is not pinned by digest");
    }
    assert!(
        df.contains("gcr.io/distroless/static"),
        "the base is not distroless static"
    );
}

#[test]
fn docker_readme_names_the_digest_refresh_routine() {
    // 012 T018; S3
    let t = read("docker/README.md").to_lowercase();
    assert!(
        t.contains("digest") && t.contains("refresh"),
        "docker/README.md has no digest refresh routine"
    );
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
    assert_eq!(
        mine.len(),
        1,
        "Cargo.lock has {} rust-ci-runner entries",
        mine.len()
    );
    mine[0]["version"]
        .as_str()
        .expect("version is a string")
        .to_string()
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
        parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
        "{m} is not X.Y.Z"
    );
}

#[test]
fn the_version_check_reads_the_right_entry() {
    let lock = "[[package]]\nname = \"other\"\nversion = \"9.9.9\"\n\n\
                [[package]]\nname = \"rust-ci-runner\"\nversion = \"0.3.0\"\n";
    assert_eq!(locked_version(lock), "0.3.0");
    assert_eq!(
        package_version("[package]\nname = \"x\"\nversion = \"1.2.3\"\n"),
        "1.2.3"
    );
}

// FR-017: the README no longer says steps get no pipeline secrets (T031).
#[test]
fn readme_has_no_stale_secrets_sentence() {
    let text = read("README.md");
    assert!(
        !text.contains("Steps get no pipeline secrets here"),
        "README.md still says steps get no pipeline secrets"
    );
}

fn commands(step: &str) -> Vec<String> {
    let t: toml::Table = read("rust-ci-runner/.factory-ci.toml")
        .parse()
        .expect("the runner pipeline is TOML");
    t["steps"][step]["commands"]
        .as_array()
        .unwrap_or_else(|| panic!("step {step} has no commands"))
        .iter()
        .map(|c| c.as_str().unwrap_or("").to_string())
        .collect()
}

#[test]
fn runner_pipeline_gives_hub_and_publish_tests_their_own_commands() {
    // 012 T011, T019; MINOR-1, QA-9: the step container has `--pids-limit 512`, and the fake Hub and the fake
    // GitHub of these tests start threads, so each group runs in its own process
    let unit = commands("unit");
    assert!(
        unit[0].contains("--skip hub_tests::") && unit[0].contains("--skip publish_tests::"),
        "the first unit command still runs hub_tests or publish_tests: {}",
        unit[0]
    );
    for group in ["hub_tests::", "publish_tests::"] {
        let own: Vec<&String> = unit
            .iter()
            .filter(|c| c.contains("--lib") && c.ends_with(group) && !c.contains("--skip"))
            .collect();
        assert_eq!(
            own.len(),
            1,
            "{group} needs one command of its own: {unit:?}"
        );
        assert!(own[0].contains("--locked"), "{}", own[0]);
    }
}

#[test]
fn runner_pipeline_integration_names_job_publish_and_not_the_repo_file_tests() {
    // 012 T019; S8: `job_publish` joins the integration command; `repo_files*` stay out (their own comment)
    let all = commands("integration").join(" ");
    let tests: Vec<&str> = all
        .split("--test")
        .skip(1)
        .filter_map(|s| s.split_whitespace().next())
        .collect();
    assert!(tests.contains(&"job_publish"), "{tests:?}");
    assert!(
        !tests.iter().any(|t| t.starts_with("repo_files")),
        "{tests:?}"
    );
}
