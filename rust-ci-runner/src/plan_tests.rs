//! T017. Targets `plan::{candidates, finish, Apis, Opts, Candidates, Final, Item}`
//! (contracts/plan-output.md, data-model.md Item and PlanResult). FR-007, FR-008, FR-009, FR-011,
//! FR-012; AS 1.1, 1.4, 3.1, 3.2, 9.1; SC-005; I2, I14.
//!
//! Assumed shapes (the other plan_*_tests files import the helpers below):
//! `Opts { cap, write_cap, pace_ms, now, b_private, log_store, secrets: Vec<String>,
//! sleep: fn(Duration), heartbeat: Option<Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>> }`
//! with `Default` (cap 20, write_cap 60, pace_ms 750, b_private true, log_store false);
//! `Apis::same(&dyn Api)` and `Apis { plain, docker }`; `Candidates { rows, unchanged, invalid,
//! problems, unchecked, checked }`; `Final { matrix, matrix_docker, problems, unchecked, waiting,
//! deferred_writes, summary, fail_tick }`; `Item` fields as in data-model.md.

use crate::fake::FakeApi;
use crate::plan::{candidates, finish, Apis, Candidates, Final, Item, Opts};
use crate::watch::{parse, WatchEntry};

pub(crate) const NOW: i64 = 1_795_651_200;

pub(crate) fn sha(n: u32) -> String {
    format!("{n:040x}")
}

pub(crate) fn fake() -> FakeApi {
    FakeApi::new("ci-bot", NOW)
}

pub(crate) fn opts() -> Opts {
    Opts {
        pace_ms: 0,
        now: NOW,
        ..Default::default()
    }
}

/// One `[[watch]]` per `(repo, extra toml lines)`.
pub(crate) fn entries(list: &[(&str, &str)]) -> Vec<WatchEntry> {
    let text: String = list
        .iter()
        .map(|(r, x)| format!("[[watch]]\nrepo = \"{r}\"\n{x}\n"))
        .collect();
    parse(&text).unwrap_or_else(|p| panic!("bad test list: {p:?}"))
}

/// Registers a repo: each tip is `(branch, sha number, commit age in seconds)`. Every tip's tree
/// holds a project at each of `projects` (`"."` for the root); each commit has no parent.
pub(crate) fn add_repo(api: &FakeApi, repo: &str, tips: &[(&str, u32, i64)], projects: &[&str]) {
    for (branch, n, age) in tips {
        api.set_branch(repo, branch, &sha(*n));
        let mut tree: Vec<(String, &str)> = vec![];
        for p in projects {
            if *p == "." {
                tree.push((".factory-ci.toml".into(), "blob"));
            } else {
                tree.push((p.to_string(), "tree"));
                tree.push((format!("{p}/.factory-ci.toml"), "blob"));
            }
        }
        let t: Vec<(&str, &str)> = tree.iter().map(|(p, k)| (p.as_str(), *k)).collect();
        api.set_tree(repo, &sha(*n), &t);
        api.set_commits(repo, &sha(*n), vec![(sha(*n), vec![], *age)]);
    }
}

pub(crate) fn plan_all(api: &FakeApi, list: &[WatchEntry], o: &Opts) -> (Candidates, Final) {
    let apis = Apis::same(api);
    let cand = candidates(&apis, list, o);
    let fin = finish(&apis, cand.clone(), o);
    (cand, fin)
}

pub(crate) fn rows(f: &Final) -> Vec<&Item> {
    f.matrix.iter().chain(f.matrix_docker.iter()).collect()
}

fn is_hex(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

const R: &str = "o/r";

#[test]
fn a_changed_project_gets_pending_queued_and_one_valid_row() {
    // AS 1.1, FR-007, FR-012, I14
    let api = fake();
    add_repo(&api, R, &[("main", 1, 100)], &["."]);
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    let rows = rows(&fin);
    assert_eq!(rows.len(), 1);
    let r = rows[0];
    assert_eq!(
        (
            r.repo.as_str(),
            r.branch.as_str(),
            r.project.as_str(),
            r.context.as_str()
        ),
        (R, "main", ".", "factory-ci")
    );
    assert!(is_hex(&r.sha, 40) && is_hex(&r.run_id, 16), "{r:?}");
    assert_eq!(r.sha, sha(1));
    assert_eq!(r.prev_sha, "");
    assert!(!r.retry && !r.allow_docker && !r.fail_closed);
    let posts = api.posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(
        (
            posts[0].state.as_str(),
            posts[0].context.as_str(),
            posts[0].description.as_str()
        ),
        ("pending", "factory-ci", "queued")
    );
    assert_eq!(posts[0].sha, sha(1));
}

#[test]
fn prev_sha_is_set_when_an_ancestor_was_found() {
    // FR-010
    let api = fake();
    api.set_branch(R, "main", &sha(1));
    api.set_tree(R, &sha(1), &[(".factory-ci.toml", "blob")]);
    api.set_commits(
        R,
        &sha(1),
        vec![(sha(1), vec![sha(2)], 0), (sha(2), vec![], 100)],
    );
    api.set_status(R, &sha(2), "factory-ci", "success", 500, "all steps passed");
    api.set_changed(R, &sha(2), &sha(1), &[("README.md", None)]);
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    assert_eq!(rows(&fin)[0].prev_sha, sha(2));
}

#[test]
fn a_tip_with_a_status_is_not_planned_so_a_second_tick_plans_nothing() {
    // AS 1.4, FR-009
    let api = fake();
    add_repo(&api, R, &[("main", 1, 100)], &["."]);
    let list = entries(&[(R, "")]);
    let (_, first) = plan_all(&api, &list, &opts());
    assert_eq!(rows(&first).len(), 1);
    let writes = api.writes();
    let (_, second) = plan_all(&api, &list, &opts());
    assert!(rows(&second).is_empty());
    assert_eq!(api.writes(), writes);
}

#[test]
fn branch_globs_pick_matching_branches_only() {
    // AS 9.1, FR-005
    let api = fake();
    add_repo(
        &api,
        R,
        &[("main", 1, 10), ("feature/x", 2, 20), ("release/1", 3, 30)],
        &["."],
    );
    let (_, fin) = plan_all(
        &api,
        &entries(&[(R, "branches = [\"main\", \"release/*\"]")]),
        &opts(),
    );
    let mut b: Vec<_> = rows(&fin).iter().map(|r| r.branch.clone()).collect();
    b.sort();
    assert_eq!(b, ["main", "release/1"]);
}

#[test]
fn two_branches_at_one_sha_plan_once_with_the_alphabetically_first_name() {
    // FR-012: the default branch (main) is not among them
    let api = fake();
    add_repo(&api, R, &[("zeta", 1, 10), ("alpha", 1, 10)], &["."]);
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    let rows = rows(&fin);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].branch, "alpha");
    assert!(!rows[0].secrets_ok);
    assert_eq!(api.posts().len(), 1);
}

#[test]
fn a_shared_sha_goes_to_the_default_branch_when_it_is_among_them() {
    // R2, FR-012; the tick passes no secrets (T017b)
    let api = fake();
    add_repo(
        &api,
        R,
        &[("main", 1, 10), ("alpha", 1, 10), ("zeta", 1, 10)],
        &["."],
    );
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    let rows = rows(&fin);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].branch, "main");
    assert!(!rows[0].secrets_ok);
}

#[test]
fn a_secret_branch_among_several_at_one_sha_still_gives_the_tick_no_secrets() {
    // R2: the row is named for the first branch; `secret_branches` no longer opens secrets on the tick
    let api = fake();
    add_repo(&api, R, &[("alpha", 1, 10), ("release/1", 1, 10)], &["."]);
    let list = entries(&[(R, "secret_branches = [\"release/*\"]")]);
    let (_, fin) = plan_all(&api, &list, &opts());
    let rows = rows(&fin);
    assert_eq!(
        (rows.len(), rows[0].branch.as_str(), rows[0].secrets_ok),
        (1, "alpha", false)
    );
}

#[test]
fn three_projects_give_three_independent_items() {
    // AS 3.1, 3.2
    let api = fake();
    add_repo(&api, R, &[("main", 1, 10)], &[".", "a", "b"]);
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    let mut ctx: Vec<_> = rows(&fin)
        .iter()
        .map(|r| (r.context.clone(), r.project.clone()))
        .collect();
    ctx.sort();
    assert_eq!(
        ctx,
        [
            ("factory-ci".into(), ".".into()),
            ("factory-ci/a".into(), "a".into()),
            ("factory-ci/b".into(), "b".into())
        ]
    );
}

#[test]
fn one_failing_item_does_not_touch_the_others_rows() {
    // AS 3.2, I2
    let api = fake();
    add_repo(&api, R, &[("main", 1, 10)], &[".", "a", "b"]);
    api.fail(
        &format!("post {R} {} factory-ci/b", sha(1)),
        crate::github::ApiError::Http(500, "no".into()),
    );
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    let mut ctx: Vec<_> = rows(&fin).iter().map(|r| r.context.clone()).collect();
    ctx.sort();
    assert_eq!(ctx, ["factory-ci", "factory-ci/a"]);
    assert_eq!(fin.problems.len(), 1);
}

#[test]
fn the_reference_fleet_costs_120_reads_and_no_writes_when_everything_is_done() {
    // SC-005: 10 repos x 5 tips x 3 projects, all done. 110 plus one repo lookup per entry;
    // the single GET /user per token is counted apart.
    let api = fake();
    let mut list = vec![];
    let names: Vec<String> = (0..10).map(|i| format!("o/r{i}")).collect();
    for (i, name) in names.iter().enumerate() {
        let tips: Vec<(String, u32, i64)> = (0..5)
            .map(|t| (format!("b{t}"), (i * 10 + t) as u32, 10))
            .collect();
        let t: Vec<(&str, u32, i64)> = tips.iter().map(|(b, n, a)| (b.as_str(), *n, *a)).collect();
        add_repo(&api, name, &t, &[".", "a", "b"]);
        for (_, n, _) in &tips {
            for ctx in ["factory-ci", "factory-ci/a", "factory-ci/b"] {
                api.set_status(name, &sha(*n), ctx, "success", 50, "all steps passed");
            }
        }
        list.push((name.as_str(), ""));
    }
    let (_, fin) = plan_all(&api, &entries(&list), &opts());
    assert!(rows(&fin).is_empty());
    let user_calls = api.calls().iter().filter(|c| c.starts_with("user")).count();
    assert_eq!(api.reads() - user_calls, 120, "{:?}", api.calls().len());
    assert_eq!(api.writes(), 0);
}

#[test]
fn phase_a_writes_nothing() {
    // contracts/plan-output.md, I28
    let api = fake();
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    let cand = candidates(&Apis::same(&api), &entries(&[(R, "")]), &opts());
    assert_eq!(cand.rows.len(), 1);
    assert_eq!(api.writes(), 0);
}

#[test]
fn rows_carry_repo_id_secrets_ok_and_retry() {
    // T048 / data-model.md Item
    let api = fake();
    api.set_repo(R, 123, false, "main");
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    let (_, fin) = plan_all(&api, &entries(&[(R, "id = 123")]), &opts());
    let r = rows(&fin)[0].clone();
    assert_eq!(r.repo_id, "123");
    assert!(!r.secrets_ok && !r.retry);
}

#[test]
fn a_truncated_tree_is_not_an_error() {
    // FR-008 edge case
    let api = fake();
    api.set_branch(R, "main", &sha(1));
    api.set_tree_truncated(
        R,
        &sha(1),
        &[(".factory-ci.toml", "blob", "b0"), ("a", "tree", "t1")],
        &[("t1", &[(".factory-ci.toml", "blob")])],
    );
    api.set_commits(R, &sha(1), vec![(sha(1), vec![], 10)]);
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    assert!(fin.problems.is_empty(), "{:?}", fin.problems);
    assert_eq!(rows(&fin).len(), 2);
}

#[test]
fn an_invalid_folder_name_gets_an_error_status_and_no_row() {
    // data-model.md Project: "invalid project folder name"
    let api = fake();
    api.set_branch(R, "main", &sha(1));
    api.set_tree(
        R,
        &sha(1),
        &[("a b", "tree"), ("a b/.factory-ci.toml", "blob")],
    );
    api.set_commits(R, &sha(1), vec![(sha(1), vec![], 10)]);
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    assert!(rows(&fin).is_empty());
    let p = api.posts();
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].state, "error");
    assert!(
        p[0].description.contains("invalid project folder name"),
        "{}",
        p[0].description
    );
}

#[test]
fn rows_hold_valid_fields_for_random_fleets() {
    // I14
    let mut x: u64 = 17;
    let mut next = |m: u64| {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (x >> 33) % m
    };
    for round in 0..20 {
        let api = fake();
        let tips: Vec<(String, u32, i64)> = (0..1 + next(6))
            .map(|t| (format!("b{t}"), t as u32 + 100 * round, next(5000) as i64))
            .collect();
        let t: Vec<(&str, u32, i64)> = tips.iter().map(|(b, n, a)| (b.as_str(), *n, *a)).collect();
        add_repo(&api, R, &t, &[".", "a"]);
        let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
        for r in rows(&fin) {
            assert!(is_hex(&r.sha, 40) && is_hex(&r.run_id, 16), "{r:?}");
            assert!(r.prev_sha.is_empty() || is_hex(&r.prev_sha, 40));
            assert!(r.project == "." || crate::projects::folder_name_ok(&r.project));
            assert_eq!(r.context, crate::projects::context(&r.project));
        }
    }
}
