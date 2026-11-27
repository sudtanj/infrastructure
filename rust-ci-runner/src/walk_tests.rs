//! T015. Targets `walk::{decide, Cache, Decision, Decisions}` (data-model.md "Decision per
//! project"). FR-009, FR-010, FR-011; AS 2.1 to 2.4; I4, I5, I13, I20, I24.
//! Assumed shapes: `Decisions { by_context: BTreeMap<String, Decision>, tip_date: Option<i64> }`,
//! `Decision::{Done, Close, Unchanged { since }, Run { prev: Option<String>, retry: bool }}`
//! (derive Debug, PartialEq, Clone), `Cache::new(user)`, `Project { path, context }`.

use crate::fake::FakeApi;
use crate::github::ApiError;
use crate::projects::{context, owns, Project};
use crate::walk::{decide, Cache, Decision, Decisions};

const NOW: i64 = 1_795_651_200;
const R: &str = "o/r";

fn sha(n: u32) -> String {
    format!("{n:040x}")
}

fn proj(p: &str) -> Project {
    Project {
        path: p.to_string(),
        context: context(p),
    }
}

fn root() -> Vec<Project> {
    vec![proj(".")]
}

fn two() -> Vec<Project> {
    vec![proj("."), proj("a")]
}

fn api() -> FakeApi {
    FakeApi::new("ci-bot", NOW)
}

/// Commits sha(0) (the tip) to sha(len-1); each has the next as its only parent.
fn chain(api: &FakeApi, len: u32) {
    let list: Vec<(String, Vec<String>, i64)> = (0..len)
        .map(|i| {
            (
                sha(i),
                if i + 1 < len {
                    vec![sha(i + 1)]
                } else {
                    vec![]
                },
                100 * i as i64,
            )
        })
        .collect();
    api.set_commits(R, &sha(0), list);
}

fn run(api: &FakeApi, projects: &[Project]) -> Result<Decisions, ApiError> {
    decide(api, R, &sha(0), projects, NOW, &mut Cache::new("ci-bot"))
}

fn of(d: &Decisions, ctx: &str) -> Decision {
    d.by_context
        .get(ctx)
        .unwrap_or_else(|| panic!("no decision for {ctx}: {:?}", d.by_context))
        .clone()
}

fn is_run(d: &Decision) -> bool {
    matches!(d, Decision::Run { .. })
}

fn retry_of(d: &Decision) -> bool {
    matches!(d, Decision::Run { retry: true, .. })
}

#[test]
fn any_status_on_the_tip_is_done_and_costs_no_history_call() {
    // AS 2.1, FR-009
    for state in ["success", "failure", "error", "pending"] {
        let a = api();
        chain(&a, 3);
        a.set_status(R, &sha(0), "factory-ci", state, 10, "x");
        let d = run(&a, &root()).unwrap();
        assert_eq!(of(&d, "factory-ci"), Decision::Done, "{state}");
        assert!(
            !a.calls().iter().any(|c| c.starts_with("commits")),
            "{state}: {:?}",
            a.calls()
        );
    }
}

#[test]
fn stale_pending_boundary_at_7199_7200_7201() {
    // data-model.md: more than 7200 s counts as no status
    for (age, stale) in [(7199, false), (7200, false), (7201, true)] {
        let a = api();
        chain(&a, 3);
        a.set_status(R, &sha(0), "factory-ci", "pending", age, "queued");
        let d = of(&run(&a, &root()).unwrap(), "factory-ci");
        assert_eq!(is_run(&d), stale, "age {age}: {d:?}");
        assert_eq!(retry_of(&d), stale, "age {age}: {d:?}");
    }
}

#[test]
fn unparsable_pending_time_is_stale_and_unparsable_error_time_is_young() {
    let a = api();
    chain(&a, 3);
    a.set_status_raw(R, &sha(0), "factory-ci", "pending", "not a time", "queued");
    assert!(is_run(&of(&run(&a, &root()).unwrap(), "factory-ci")));
    let b = api();
    chain(&b, 3);
    b.set_status_raw(R, &sha(0), "factory-ci", "error", "not a time", "boom");
    assert_eq!(of(&run(&b, &root()).unwrap(), "factory-ci"), Decision::Done);
}

#[test]
fn nearest_success_ancestor_with_untouched_files_is_unchanged() {
    // AS 2.2
    let a = api();
    chain(&a, 6);
    for ctx in ["factory-ci", "factory-ci/a"] {
        a.set_status(R, &sha(3), ctx, "success", 500, "all steps passed");
    }
    a.set_changed(R, &sha(3), &sha(0), &[("b/x.txt", None)]);
    let d = run(&a, &two()).unwrap();
    assert_eq!(
        of(&d, "factory-ci/a"),
        Decision::Unchanged { since: sha(3) }
    );
    // the root owns b/x.txt
    assert_eq!(
        of(&d, "factory-ci"),
        Decision::Run {
            prev: Some(sha(3)),
            retry: false
        }
    );
}

#[test]
fn a_touched_file_runs_with_the_ancestor_as_prev_including_renames() {
    // AS 2.3
    let a = api();
    chain(&a, 4);
    for ctx in ["factory-ci", "factory-ci/a"] {
        a.set_status(R, &sha(2), ctx, "success", 500, "ok");
    }
    a.set_changed(R, &sha(2), &sha(0), &[("b/new", Some("a/old"))]);
    let d = run(&a, &two()).unwrap();
    assert_eq!(
        of(&d, "factory-ci/a"),
        Decision::Run {
            prev: Some(sha(2)),
            retry: false
        }
    );
}

#[test]
fn zero_changed_files_marks_every_project_unchanged() {
    // spec edge case: empty commit
    let a = api();
    chain(&a, 3);
    for ctx in ["factory-ci", "factory-ci/a"] {
        a.set_status(R, &sha(1), ctx, "success", 500, "ok");
    }
    a.set_changed(R, &sha(1), &sha(0), &[]);
    let d = run(&a, &two()).unwrap();
    assert_eq!(of(&d, "factory-ci"), Decision::Unchanged { since: sha(1) });
    assert_eq!(
        of(&d, "factory-ci/a"),
        Decision::Unchanged { since: sha(1) }
    );
}

#[test]
fn an_ancestor_that_is_not_a_clean_success_runs() {
    // AS 2.4: failure, error, fresh pending, stale pending
    for (state, age) in [
        ("failure", 100),
        ("error", 100),
        ("error", 9000),
        ("pending", 100),
        ("pending", 9000),
    ] {
        let a = api();
        chain(&a, 4);
        a.set_status(R, &sha(2), "factory-ci", state, age, "x");
        a.set_changed(R, &sha(2), &sha(0), &[]);
        let d = of(&run(&a, &root()).unwrap(), "factory-ci");
        assert!(
            matches!(d, Decision::Run { retry: false, .. }),
            "{state} {age}: {d:?}"
        );
    }
}

#[test]
fn no_ancestor_with_a_status_runs_with_no_prev() {
    // FR-010, I4
    let a = api();
    chain(&a, 60);
    let d = of(&run(&a, &root()).unwrap(), "factory-ci");
    assert_eq!(
        d,
        Decision::Run {
            prev: None,
            retry: false
        }
    );
}

#[test]
fn an_ancestor_past_50_commits_is_not_used() {
    let a = api();
    chain(&a, 80);
    a.set_status(R, &sha(60), "factory-ci", "success", 500, "ok");
    a.set_changed(R, &sha(60), &sha(0), &[]);
    assert!(is_run(&of(&run(&a, &root()).unwrap(), "factory-ci")));
}

#[test]
fn a_chain_that_leaves_the_commit_window_runs() {
    // the window answer has 30 commits; the 31st exists only as a parent id
    let a = api();
    chain(&a, 30);
    a.set_commits(
        R,
        &sha(0),
        (0..30)
            .map(|i| (sha(i), vec![sha(i + 1)], 100 * i as i64))
            .collect(),
    );
    a.set_status(R, &sha(35), "factory-ci", "success", 500, "ok");
    a.set_changed(R, &sha(35), &sha(0), &[]);
    assert!(is_run(&of(&run(&a, &root()).unwrap(), "factory-ci")));
}

#[test]
fn only_the_first_parent_is_followed_over_a_merge() {
    // FR-010
    let a = api();
    a.set_commits(
        R,
        &sha(0),
        vec![
            (sha(0), vec![sha(1), sha(9)], 0),
            (sha(1), vec![sha(2)], 100),
            (sha(2), vec![], 200),
            (sha(9), vec![], 300),
        ],
    );
    a.set_status(R, &sha(9), "factory-ci", "success", 500, "ok");
    a.set_changed(R, &sha(9), &sha(0), &[]);
    let d = of(&run(&a, &root()).unwrap(), "factory-ci");
    assert_eq!(
        d,
        Decision::Run {
            prev: None,
            retry: false
        }
    );
    assert!(!a
        .calls()
        .iter()
        .any(|c| c.starts_with(&format!("status {R} {}", sha(9)))));
    a.set_status(R, &sha(2), "factory-ci", "success", 500, "ok");
    a.set_changed(R, &sha(2), &sha(0), &[]);
    let d = of(
        &decide(&a, R, &sha(0), &root(), NOW, &mut Cache::new("ci-bot")).unwrap(),
        "factory-ci",
    );
    assert_eq!(d, Decision::Unchanged { since: sha(2) });
}

#[test]
fn a_compare_that_stays_full_or_fails_runs() {
    // contracts/github-api.md: 3 pages then "unknown"
    let a = api();
    chain(&a, 3);
    a.set_status(R, &sha(1), "factory-ci", "success", 500, "ok");
    a.set_changed_unknown(R, &sha(1), &sha(0));
    assert!(is_run(&of(&run(&a, &root()).unwrap(), "factory-ci")));
}

#[test]
fn walk_errors_other_than_rate_limits_make_the_project_run() {
    // FR-011
    let cases = [
        "commits".to_string(),
        format!("status {R} {}", sha(1)),
        "compare".to_string(),
    ];
    for prefix in cases {
        let a = api();
        chain(&a, 3);
        a.set_status(R, &sha(1), "factory-ci", "success", 500, "ok");
        a.set_changed(R, &sha(1), &sha(0), &[]);
        a.fail(&prefix, ApiError::Http(500, "boom".into()));
        let d = of(&run(&a, &root()).unwrap(), "factory-ci");
        assert!(is_run(&d), "{prefix}: {d:?}");
    }
}

#[test]
fn rate_limit_propagates_from_every_walk_call() {
    // I13
    for prefix in [
        "commits".to_string(),
        format!("status {R} {}", sha(1)),
        "compare".to_string(),
    ] {
        let a = api();
        chain(&a, 3);
        a.set_status(R, &sha(1), "factory-ci", "success", 500, "ok");
        a.set_changed(R, &sha(1), &sha(0), &[]);
        a.fail(&prefix, ApiError::RateLimited("limit".into()));
        match run(&a, &root()) {
            Err(ApiError::RateLimited(_)) => {}
            other => panic!("{prefix}: {other:?}"),
        }
    }
}

#[test]
fn statuses_are_cached_per_sha_across_projects_and_tips() {
    let a = api();
    chain(&a, 6);
    // a second tip whose chain joins the first at sha(3)
    a.set_commits(
        R,
        &sha(10),
        vec![
            (sha(10), vec![sha(3)], 0),
            (sha(3), vec![sha(4)], 100),
            (sha(4), vec![], 200),
        ],
    );
    for ctx in ["factory-ci", "factory-ci/a"] {
        a.set_status(R, &sha(3), ctx, "success", 500, "ok");
    }
    a.set_changed(R, &sha(3), &sha(0), &[]);
    a.set_changed(R, &sha(3), &sha(10), &[]);
    let mut cache = Cache::new("ci-bot");
    decide(&a, R, &sha(0), &two(), NOW, &mut cache).unwrap();
    decide(&a, R, &sha(10), &two(), NOW, &mut cache).unwrap();
    let n = a
        .calls()
        .iter()
        .filter(|c| **c == format!("status {R} {}", sha(3)))
        .count();
    assert_eq!(n, 1, "{:?}", a.calls());
}

#[test]
fn the_tip_date_is_returned_for_ordering() {
    let a = api();
    chain(&a, 3);
    let d = run(&a, &root()).unwrap();
    assert_eq!(d.tip_date, Some(NOW)); // age 0 in `chain`
}

// ---- retry budget (A6c, FR-009, I24) ----

fn tip_decision(state: &str, age: i64, desc: &str) -> Decision {
    let a = api();
    chain(&a, 2);
    a.set_status(R, &sha(0), "factory-ci", state, age, desc);
    of(&run(&a, &root()).unwrap(), "factory-ci")
}

#[test]
fn an_old_unmarked_error_is_retried_once() {
    assert!(retry_of(&tip_decision("error", 1801, "step build failed")));
    assert_eq!(
        tip_decision("error", 1801, "clone failed (retry)"),
        Decision::Done
    );
    assert_eq!(tip_decision("error", 1799, "clone failed"), Decision::Done);
    assert_eq!(tip_decision("error", 1800, "clone failed"), Decision::Done);
}

#[test]
fn a_failure_is_never_retried() {
    assert_eq!(
        tip_decision("failure", 999_999, "step x failed: exit code 1"),
        Decision::Done
    );
    assert_eq!(
        tip_decision("success", 999_999, "all steps passed"),
        Decision::Done
    );
}

#[test]
fn an_old_pending_is_replanned_once_then_closed() {
    assert!(retry_of(&tip_decision("pending", 7201, "queued")));
    assert_eq!(
        tip_decision("pending", 7201, "queued (retry)"),
        Decision::Close
    );
    assert_eq!(
        tip_decision("pending", 7200, "queued (retry)"),
        Decision::Done
    );
}

#[test]
fn a_status_by_another_creator_is_ignored() {
    // FR-049, I20
    let a = api();
    chain(&a, 3);
    a.set_status_by(
        R,
        &sha(0),
        "factory-ci",
        "success",
        10,
        "all steps passed",
        "mallory",
    );
    let d = of(&run(&a, &root()).unwrap(), "factory-ci");
    assert_eq!(
        d,
        Decision::Run {
            prev: None,
            retry: false
        }
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
fn foreign_statuses_never_change_a_decision() {
    // I20: decision with foreign statuses mixed in equals the decision with them removed
    let mut rng = Lcg(11);
    for _ in 0..150 {
        let (full, clean) = (api(), api());
        chain(&full, 6);
        chain(&clean, 6);
        for i in 0..4 {
            for _ in 0..rng.next(3) {
                let state = ["success", "failure", "error", "pending"][rng.next(4) as usize];
                let age = [10, 2000, 9000][rng.next(3) as usize];
                let foreign = rng.next(2) == 0;
                let who = if foreign { "mallory" } else { "ci-bot" };
                full.set_status_by(R, &sha(i), "factory-ci", state, age, "d", who);
                if !foreign {
                    clean.set_status_by(R, &sha(i), "factory-ci", state, age, "d", who);
                }
            }
            full.set_changed(R, &sha(i), &sha(0), &[]);
            clean.set_changed(R, &sha(i), &sha(0), &[]);
        }
        assert_eq!(
            run(&full, &root()).unwrap().by_context,
            run(&clean, &root()).unwrap().by_context
        );
    }
}

#[test]
fn the_retry_fires_exactly_when_the_rule_says() {
    // I24 oracle
    let mut rng = Lcg(5);
    for _ in 0..200 {
        let state = ["success", "failure", "error", "pending"][rng.next(4) as usize];
        let age = [0, 1799, 1801, 7200, 7201, 90_000][rng.next(6) as usize];
        let marked = rng.next(2) == 0;
        let desc = if marked { "reason (retry)" } else { "reason" };
        let old_error = state == "error" && age > 1800;
        let old_pending = state == "pending" && age > 7200;
        let want = match (old_error || old_pending, marked, state) {
            (true, false, _) => "retry",
            (true, true, "pending") => "close",
            _ => "done",
        };
        let d = tip_decision(state, age, desc);
        let got = match d {
            Decision::Run { retry: true, .. } => "retry",
            Decision::Close => "close",
            Decision::Done => "done",
            ref other => panic!("{state} {age} {desc}: {other:?}"),
        };
        assert_eq!(got, want, "{state} {age} {desc}");
    }
}

#[test]
fn with_no_status_anywhere_every_project_runs() {
    // I4
    let mut rng = Lcg(3);
    for _ in 0..30 {
        let a = api();
        chain(&a, 1 + rng.next(70) as u32);
        let d = run(&a, &two()).unwrap();
        for ctx in ["factory-ci", "factory-ci/a"] {
            assert_eq!(
                of(&d, ctx),
                Decision::Run {
                    prev: None,
                    retry: false
                }
            );
        }
    }
}

#[test]
fn unchanged_only_when_no_changed_path_is_owned() {
    // I5
    let files = ["README.md", "a/x", "b/y", "a/old", "ab/z"];
    let set: Vec<String> = vec![".".into(), "a".into()];
    let mut rng = Lcg(9);
    for _ in 0..100 {
        let a = api();
        chain(&a, 3);
        for ctx in ["factory-ci", "factory-ci/a"] {
            a.set_status(R, &sha(1), ctx, "success", 500, "ok");
        }
        let changed: Vec<(&str, Option<&str>)> = (0..rng.next(3))
            .map(|_| {
                let f = files[rng.next(5) as usize];
                (
                    f,
                    if rng.next(3) == 0 {
                        Some(files[rng.next(5) as usize])
                    } else {
                        None
                    },
                )
            })
            .collect();
        a.set_changed(R, &sha(1), &sha(0), &changed);
        let d = run(&a, &two()).unwrap();
        for p in [".", "a"] {
            let touched = changed
                .iter()
                .any(|(f, prev)| owns(p, f, &set) || prev.is_some_and(|x| owns(p, x, &set)));
            let got = of(&d, &context(p));
            assert_eq!(
                matches!(got, Decision::Unchanged { .. }),
                !touched,
                "{p} {changed:?} {got:?}"
            );
        }
    }
}
