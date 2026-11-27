//! T027. Per-repo errors and the rate-limit stop. FR-013, FR-014; AS 6.1, 6.2, 6.4; SC-004;
//! I2, I7, I13. Helpers come from `plan_tests`. Problem lines are `<repo or list>: <reason>`.

use crate::github::ApiError;
use crate::plan::Opts;
use crate::plan_tests::{add_repo, entries, fake, opts, plan_all, rows, sha};

fn two_repos() -> (crate::fake::FakeApi, Vec<crate::watch::WatchEntry>) {
    let api = fake();
    add_repo(&api, "o/bad", &[("main", 1, 10)], &["."]);
    add_repo(&api, "o/good", &[("main", 2, 10)], &["."]);
    (api, entries(&[("o/bad", ""), ("o/good", "")]))
}

fn assert_bad_repo_skipped(err: ApiError, reason: &str) {
    let (api, list) = two_repos();
    api.fail("branches o/bad", err);
    let (_, fin) = plan_all(&api, &list, &opts());
    let r = rows(&fin);
    assert_eq!(r.len(), 1, "{:?}", fin.problems);
    assert_eq!(r[0].repo, "o/good");
    assert_eq!(fin.problems.len(), 1);
    assert!(
        fin.problems[0].starts_with("o/bad") && fin.problems[0].contains(reason),
        "{:?}",
        fin.problems
    );
    assert!(
        fin.summary.contains("o/bad"),
        "problems are listed in the summary"
    );
    assert!(fin.unchecked.is_empty());
}

#[test]
fn a_404_repo_is_skipped_and_named_with_its_reason() {
    // AS 6.1
    assert_bad_repo_skipped(ApiError::Http(404, "not found".into()), "404");
}

#[test]
fn a_403_repo_is_skipped_and_named_with_its_reason() {
    assert_bad_repo_skipped(ApiError::Http(403, "forbidden".into()), "403");
}

#[test]
fn a_network_error_or_5xx_after_retries_is_skipped_likewise() {
    assert_bad_repo_skipped(
        ApiError::Net("connection refused".into()),
        "connection refused",
    );
    assert_bad_repo_skipped(ApiError::Http(503, "unavailable".into()), "503");
}

#[test]
fn a_failing_tree_call_drops_the_whole_repo_atomically() {
    // data-model.md: per-repo results are atomic
    let api = fake();
    add_repo(&api, "o/r", &[("a", 1, 20), ("b", 2, 10)], &["."]);
    api.fail(
        &format!("tree o/r {}", sha(2)),
        ApiError::Http(500, "boom".into()),
    );
    let (_, fin) = plan_all(&api, &entries(&[("o/r", "")]), &opts());
    assert!(rows(&fin).is_empty());
    assert!(api.posts().is_empty());
    assert_eq!(fin.problems.len(), 1);
}

#[test]
fn a_repo_with_101_branches_is_an_error_with_no_partial_rows() {
    // AS 6.4
    let api = fake();
    let tips: Vec<(String, u32, i64)> = (0..101)
        .map(|i| (format!("b{i:03}"), 1000 + i as u32, 10))
        .collect();
    let t: Vec<(&str, u32, i64)> = tips.iter().map(|(b, n, a)| (b.as_str(), *n, *a)).collect();
    add_repo(&api, "o/big", &t, &["."]);
    add_repo(&api, "o/ok", &[("main", 1, 10)], &["."]);
    let (_, fin) = plan_all(&api, &entries(&[("o/big", ""), ("o/ok", "")]), &opts());
    let r = rows(&fin);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].repo, "o/ok");
    assert!(
        fin.problems
            .iter()
            .any(|p| p.starts_with("o/big") && p.contains("100")),
        "{:?}",
        fin.problems
    );
}

#[test]
fn exactly_100_matching_branches_is_fine_and_filtering_happens_first() {
    let api = fake();
    let tips: Vec<(String, u32, i64)> = (0..120)
        .map(|i| {
            (
                format!("{}{i:03}", if i < 100 { "m" } else { "x" }),
                1000 + i as u32,
                10,
            )
        })
        .collect();
    let t: Vec<(&str, u32, i64)> = tips.iter().map(|(b, n, a)| (b.as_str(), *n, *a)).collect();
    add_repo(&api, "o/r", &t, &["."]);
    let o = Opts {
        cap: 200,
        write_cap: 200,
        ..opts()
    };
    let (_, fin) = plan_all(&api, &entries(&[("o/r", "branches = [\"m*\"]")]), &o);
    assert!(fin.problems.is_empty(), "{:?}", fin.problems);
    assert_eq!(rows(&fin).len(), 100);
}

#[test]
fn a_repo_with_more_than_500_branches_is_a_problem_and_the_others_continue() {
    // R9, FR-006
    let api = fake();
    let tips: Vec<(String, u32, i64)> = (0..501)
        .map(|i| (format!("b{i:03}"), 1000 + i as u32, 10))
        .collect();
    let t: Vec<(&str, u32, i64)> = tips.iter().map(|(b, n, a)| (b.as_str(), *n, *a)).collect();
    add_repo(&api, "o/huge", &t, &["."]);
    add_repo(&api, "o/ok", &[("main", 1, 10)], &["."]);
    let (_, fin) = plan_all(&api, &entries(&[("o/huge", ""), ("o/ok", "")]), &opts());
    assert_eq!(rows(&fin).len(), 1);
    assert_eq!(rows(&fin)[0].repo, "o/ok");
    let hit = fin
        .problems
        .iter()
        .any(|p| p.starts_with("o/huge") && p.contains("more than 500 branches"));
    assert!(hit, "{:?}", fin.problems);
    assert!(fin.unchecked.is_empty());
}

#[test]
fn rate_limit_on_the_third_repo_keeps_earlier_rows_and_lists_the_rest_unchecked() {
    // AS 6.2, SC-004, I13
    let api = fake();
    for (i, n) in ["o/a", "o/b", "o/c", "o/d", "o/e"].iter().enumerate() {
        add_repo(&api, n, &[("main", 10 + i as u32, 10)], &["."]);
    }
    api.fail("branches o/c", ApiError::RateLimited("limit".into()));
    let list = entries(&[
        ("o/a", ""),
        ("o/b", ""),
        ("o/c", ""),
        ("o/d", ""),
        ("o/e", ""),
    ]);
    let (cand, fin) = plan_all(&api, &list, &opts());
    let mut got: Vec<_> = rows(&fin).iter().map(|r| r.repo.clone()).collect();
    got.sort();
    assert_eq!(got, ["o/a", "o/b"]);
    assert_eq!(
        api.posts().iter().filter(|p| p.state == "pending").count(),
        2
    );
    assert_eq!(fin.unchecked, ["o/c", "o/d", "o/e"]);
    assert_eq!(cand.unchecked, ["o/c", "o/d", "o/e"]);
    // I13: nothing after the limited call touched d or e
    assert!(
        api.calls()
            .iter()
            .all(|c| !c.contains("o/d") && !c.contains("o/e")),
        "{:?}",
        api.calls()
    );
}

#[test]
fn a_rate_limit_inside_the_walk_also_stops_reads() {
    let api = fake();
    add_repo(&api, "o/a", &[("main", 1, 10)], &["."]);
    add_repo(&api, "o/b", &[("main", 2, 10)], &["."]);
    api.fail("commits o/a", ApiError::RateLimited("limit".into()));
    let (_, fin) = plan_all(&api, &entries(&[("o/a", ""), ("o/b", "")]), &opts());
    assert!(
        fin.unchecked.contains(&"o/b".to_string()),
        "{:?}",
        fin.unchecked
    );
    assert!(api.calls().iter().all(|c| !c.contains("o/b")));
}

#[test]
fn a_failed_pending_post_drops_the_row_and_records_a_problem() {
    // I2, plan.md failure table "plan C: pending post fails"
    let api = fake();
    add_repo(&api, "o/r", &[("a", 1, 20), ("b", 2, 10)], &["."]);
    api.fail(
        &format!("post o/r {}", sha(1)),
        ApiError::Http(403, "no write access".into()),
    );
    let (_, fin) = plan_all(&api, &entries(&[("o/r", "")]), &opts());
    let r = rows(&fin);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].sha, sha(2));
    assert_eq!(fin.problems.len(), 1);
}

#[test]
fn a_row_exists_exactly_when_its_pending_post_succeeded() {
    // I2 as a property over random post failures
    let mut x: u64 = 23;
    let mut next = |m: u64| {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (x >> 33) % m
    };
    for round in 0..25u32 {
        let api = fake();
        let tips: Vec<(String, u32, i64)> = (0..8)
            .map(|i| (format!("b{i}"), 100 * round + i as u32, 100 - i as i64))
            .collect();
        let t: Vec<(&str, u32, i64)> = tips.iter().map(|(b, n, a)| (b.as_str(), *n, *a)).collect();
        add_repo(&api, "o/r", &t, &["."]);
        for (_, n, _) in &tips {
            if next(3) == 0 {
                api.fail(
                    &format!("post o/r {}", sha(*n)),
                    ApiError::Http(500, "x".into()),
                );
            }
        }
        let (_, fin) = plan_all(&api, &entries(&[("o/r", "")]), &opts());
        let ok: Vec<String> = api
            .posts()
            .iter()
            .filter(|p| p.state == "pending")
            .map(|p| p.sha.clone())
            .collect();
        let mut got: Vec<String> = rows(&fin).iter().map(|r| r.sha.clone()).collect();
        let mut want = ok.clone();
        got.sort();
        want.sort();
        assert_eq!(got, want, "round {round}");
    }
}

#[test]
fn no_token_reaches_the_problems_the_summary_or_the_rows() {
    // I7: the fake's error text carries the token; the plan redacts it
    let token = "tok-PLAN-SECRET-5";
    let (api, list) = two_repos();
    api.fail(
        "branches o/bad",
        ApiError::Http(500, format!("Bearer {token} rejected")),
    );
    let o = Opts {
        secrets: vec![token.to_string()],
        ..opts()
    };
    let (cand, fin) = plan_all(&api, &list, &o);
    let all = format!(
        "{:?}{:?}{}{:?}",
        cand.problems,
        fin.problems,
        fin.summary,
        rows(&fin)
    );
    assert!(!all.contains(token), "{all}");
}
