//! 012 T013, T025 (test-first, plan step S2). Row fields and routing for `docker/<name>` projects: `allow_docker`
//! and `secrets_ok` only for an image project on `main` (`image::publishes`), the plain matrix for everything
//! else, the retry rule for `error` against `failure` (FR-007), and the plan cases of FR-005 and FR-019.
//! FR-023; I7.
//!
//! Not yet registered: add `#[cfg(test)] mod plan_image_tests;` to `lib.rs` (after `plan_tests`, whose helpers
//! it imports), then remove each `#[ignore = "test-first"]` as its test passes (T013, T025). The two tests
//! without the marker pin behaviour that holds today and must stay green.
//!
//! `plan_hardening_tests.rs:89` asserts `!r.secrets_ok` for a fixture with only a `.` project. It must get no
//! `docker/` project; `the_hardening_fixture_has_no_docker_project` fails if one is added.

use crate::fake::FakeApi;
use crate::plan_tests::{entries, fake, opts, plan_all, rows, sha};

const R: &str = "o/r";
const BOTH: &str = "allow_docker = true\nsecrets = [\"DOCKERHUB_USERNAME\", \"DOCKERHUB_TOKEN\"]";

/// Registers `tips` (`(branch, sha number, commit age)`); each tip's tree holds the `folders` and, under a
/// `docker` folder, the `images`, every one with a `.factory-ci.toml`.
fn add(api: &FakeApi, tips: &[(&str, u32, i64)], folders: &[&str], images: &[&str]) {
    for (branch, n, age) in tips {
        api.set_branch(R, branch, &sha(*n));
        let mut t: Vec<(String, &str)> = vec![];
        for f in folders {
            t.push((f.to_string(), "tree"));
            t.push((format!("{f}/.factory-ci.toml"), "blob"));
        }
        if !images.is_empty() {
            t.push(("docker".to_string(), "tree"));
        }
        for i in images {
            t.push((format!("docker/{i}"), "tree"));
            t.push((format!("docker/{i}/.factory-ci.toml"), "blob"));
        }
        let r: Vec<(&str, &str)> = t.iter().map(|(p, k)| (p.as_str(), *k)).collect();
        api.set_tree(R, &sha(*n), &r);
        api.set_commits(R, &sha(*n), vec![(sha(*n), vec![], *age)]);
    }
}

#[test]
#[ignore = "test-first"]
fn an_image_row_on_main_is_a_docker_row_with_secrets_ok() {
    // 012 T013; S2; FR-023, I7. The `rust-ci-runner` row of the same entry stays plain.
    let api = fake();
    add(
        &api,
        &[("main", 1, 100)],
        &["rust-ci-runner"],
        &["hello-static"],
    );
    let (_, fin) = plan_all(&api, &entries(&[(R, BOTH)]), &opts());
    assert_eq!(fin.matrix_docker.len(), 1, "{:?}", fin.matrix_docker);
    let d = &fin.matrix_docker[0];
    assert_eq!(d.project, "docker/hello-static");
    assert_eq!(d.context, "factory-ci/docker/hello-static");
    assert!(d.allow_docker && d.secrets_ok, "{d:?}");
    assert_eq!(d.secrets, "DOCKERHUB_USERNAME DOCKERHUB_TOKEN");
    assert_eq!(fin.matrix.len(), 1, "{:?}", fin.matrix);
    let p = &fin.matrix[0];
    assert_eq!(p.project, "rust-ci-runner");
    assert!(!p.allow_docker && !p.secrets_ok, "{p:?}");
}

#[test]
#[ignore = "test-first"]
fn plain_rows_stay_in_the_plain_matrix() {
    // 012 T013; FR-023: a `rust-ci-runner` or `ci-canary` row on `main` under an entry with `allow_docker`
    let api = fake();
    add(
        &api,
        &[("main", 1, 100)],
        &["rust-ci-runner", "ci-canary"],
        &[],
    );
    let (_, fin) = plan_all(&api, &entries(&[(R, BOTH)]), &opts());
    assert!(fin.matrix_docker.is_empty(), "{:?}", fin.matrix_docker);
    assert_eq!(fin.matrix.len(), 2);
    for r in &fin.matrix {
        assert!(!r.allow_docker && !r.secrets_ok, "{r:?}");
    }
}

#[test]
#[ignore = "test-first"]
fn an_image_row_on_another_branch_has_neither_flag() {
    // 012 T013; S2: a `docker/x` row on `dev` is a plain row
    let api = fake();
    add(
        &api,
        &[("main", 1, 100), ("dev", 2, 50)],
        &[],
        &["hello-static"],
    );
    let (_, fin) = plan_all(&api, &entries(&[(R, BOTH)]), &opts());
    let all = rows(&fin);
    assert_eq!(all.len(), 2);
    for r in all {
        let main = r.branch == "main";
        assert_eq!((r.allow_docker, r.secrets_ok), (main, main), "{r:?}");
    }
    assert_eq!(fin.matrix_docker.len(), 1);
    assert_eq!(fin.matrix_docker[0].branch, "main");
    assert_eq!(fin.matrix.len(), 1);
    assert_eq!(fin.matrix[0].branch, "dev");
}

#[test]
#[ignore = "test-first"]
fn secrets_ok_true_only_for_main_image_rows() {
    // 012 T013; I7 over the grid: entry flag x branch x project
    for flag in ["", "allow_docker = true\n"] {
        let api = fake();
        add(
            &api,
            &[("main", 1, 100), ("dev", 2, 50), ("release/1", 3, 20)],
            &["svc", "rust-ci-runner"],
            &["hello-static", "other"],
        );
        let extra = format!("{flag}secrets = [\"DOCKERHUB_USERNAME\", \"DOCKERHUB_TOKEN\"]");
        let (_, fin) = plan_all(&api, &entries(&[(R, extra.as_str())]), &opts());
        assert_eq!(rows(&fin).len(), 12, "3 branches x 4 projects");
        for r in rows(&fin) {
            let image = r.project.starts_with("docker/");
            let main = r.branch == "main";
            assert_eq!(
                r.allow_docker,
                !flag.is_empty() && image && main,
                "{flag:?} {r:?}"
            );
            if r.secrets_ok {
                assert!(
                    image && main,
                    "secrets_ok outside an image row on main: {r:?}"
                );
            }
            if !flag.is_empty() && image && main {
                assert!(r.secrets_ok, "{r:?}");
            }
        }
        assert!(fin.matrix_docker.iter().all(|r| r.allow_docker));
        assert!(fin.matrix.iter().all(|r| !r.allow_docker));
    }
}

#[test]
fn secrets_ok_needs_the_entry_flag_too() {
    // 012 T013 against I7. CONFLICT in the plan, reported to the lead: I7 says "only when the entry has
    // `allow_docker`", the T013 formula (`secrets_ok = publishes(path, branch)`) does not look at the flag.
    // This test pins I7, the stricter reading: without the flag no row gets the names.
    let api = fake();
    add(&api, &[("main", 1, 100)], &[], &["hello-static"]);
    let extra = "secrets = [\"DOCKERHUB_USERNAME\", \"DOCKERHUB_TOKEN\"]";
    let (_, fin) = plan_all(&api, &entries(&[(R, extra)]), &opts());
    let all = rows(&fin);
    assert_eq!(all.len(), 1);
    assert!(!all[0].allow_docker && !all[0].secrets_ok, "{:?}", all[0]);
    assert!(fin.matrix_docker.is_empty());
}

#[test]
fn an_invalid_image_name_gets_an_error_status_and_no_row() {
    // 012 T013; FR-002: `docker/Hello` is flagged like a bad folder name, no job
    let api = fake();
    add(&api, &[("main", 1, 100)], &[], &["Hello"]);
    let (_, fin) = plan_all(&api, &entries(&[(R, BOTH)]), &opts());
    assert!(rows(&fin).is_empty(), "{:?}", rows(&fin));
    let p = api.posts();
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].state, "error");
    assert_eq!(p[0].context, "factory-ci/docker/Hello");
    assert!(
        p[0].description.contains("invalid project folder name"),
        "{}",
        p[0].description
    );
}

#[test]
fn an_error_on_an_image_row_is_retried_and_a_failure_is_final() {
    // 012 T025; FR-007: the walk retries `error` once, after 30 minutes, and never a `failure`. An image row
    // now ends `error` for a failed step, so the next tick plans it again.
    let ctx = "factory-ci/docker/hello-static";
    let table = [
        ("error", 2000, "step build failed: exit code 1", 1, true),
        ("error", 2000, "run timed out", 1, true),
        ("error", 100, "step build failed: exit code 1", 0, false),
        (
            "error",
            2000,
            "step build failed: exit code 1 (retry)",
            0,
            false,
        ),
        ("failure", 5000, "step build failed: exit code 1", 0, false),
        ("failure", 5000, "check failed: instruction", 0, false),
    ];
    for (state, age, text, want, retry) in table {
        let api = fake();
        add(&api, &[("main", 1, 100)], &[], &["hello-static"]);
        api.set_status(R, &sha(1), ctx, state, age, text);
        let (_, fin) = plan_all(&api, &entries(&[(R, BOTH)]), &opts());
        let all = rows(&fin);
        assert_eq!(all.len(), want, "{state} {age} {text}");
        if want == 1 {
            assert_eq!(all[0].retry, retry, "{state} {text}");
            assert!(all[0].allow_docker && all[0].secrets_ok, "{:?}", all[0]);
        }
    }
}

#[test]
fn deleted_folder_makes_no_call() {
    // 012 T025; FR-019: the folder was published at the parent and is gone at the tip. Nothing is planned or
    // posted for it. (Passes before S2 for a trivial reason; it bites once `docker/x` is discovered.)
    let api = fake();
    api.set_branch(R, "main", &sha(2));
    api.set_tree(R, &sha(2), &[(".factory-ci.toml", "blob")]);
    api.set_commits(
        R,
        &sha(2),
        vec![(sha(2), vec![sha(1)], 10), (sha(1), vec![], 100)],
    );
    api.set_tree(
        R,
        &sha(1),
        &[
            ("docker", "tree"),
            ("docker/x1", "tree"),
            ("docker/x1/.factory-ci.toml", "blob"),
        ],
    );
    api.set_status(
        R,
        &sha(1),
        "factory-ci/docker/x1",
        "success",
        500,
        "published latest and sha-0123456789ab",
    );
    let (_, fin) = plan_all(&api, &entries(&[(R, BOTH)]), &opts());
    assert!(fin.problems.is_empty(), "{:?}", fin.problems);
    assert!(
        rows(&fin).iter().all(|r| !r.project.contains("docker")),
        "{:?}",
        rows(&fin)
    );
    assert!(
        api.posts().iter().all(|p| !p.context.contains("docker/x1")),
        "{:?}",
        api.posts()
    );
}

#[test]
fn unchanged_commit_builds_nothing() {
    // 012 T025; FR-005: an own status on the tip means no row and no new `pending`
    let api = fake();
    add(&api, &[("main", 1, 100)], &[], &["hello-static"]);
    api.set_status(
        R,
        &sha(1),
        "factory-ci/docker/hello-static",
        "success",
        500,
        "published latest and sha-0123456789ab",
    );
    let (_, fin) = plan_all(&api, &entries(&[(R, BOTH)]), &opts());
    assert!(rows(&fin).is_empty(), "{:?}", rows(&fin));
    assert!(
        api.posts().iter().all(|p| p.state != "pending"),
        "{:?}",
        api.posts()
    );
}

#[test]
fn the_hardening_fixture_has_no_docker_project() {
    // 012 T013; plan "Files": `plan_hardening_tests.rs:89` asserts `!r.secrets_ok` for a fixture that has only
    // a `.` project. A `docker/` project there would make that assertion false for the right reason.
    let text = include_str!("plan_hardening_tests.rs");
    assert!(
        !text.contains("docker/"),
        "plan_hardening_tests.rs names a docker/ project"
    );
    assert!(
        text.contains("assert!(!r.secrets_ok"),
        "the secrets_ok assertion is gone"
    );
}

#[test]
fn rows_without_the_entry_flag_stay_plain_today() {
    // 012 T013: the entry without `allow_docker` gives no docker row for any project (pins current behaviour)
    let api = fake();
    add(&api, &[("main", 1, 100)], &["svc"], &["hello-static"]);
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    assert!(fin.matrix_docker.is_empty());
    assert!(rows(&fin).iter().all(|r| !r.allow_docker));
}
