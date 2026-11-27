//! T056. Review hardening at plan level. FR-049, FR-050, FR-052, FR-055, FR-058, FR-059; I3, I20,
//! I26, I28. Helpers come from `plan_tests`.

use crate::fake::FakeApi;
use crate::plan::{candidates, finish, Apis, Opts};
use crate::plan_tests::{add_repo, entries, fake, opts, plan_all, rows, sha, NOW};
use std::cell::Cell;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const R: &str = "o/r";

thread_local! {
    static SLEPT_MS: Cell<u64> = const { Cell::new(0) };
}

fn record_sleep(d: Duration) {
    SLEPT_MS.with(|s| s.set(s.get() + d.as_millis() as u64));
}

#[test]
fn a_stranger_status_is_ignored_and_the_project_still_runs() {
    // FR-050, I20
    let api = fake();
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    api.set_status_by(
        R,
        &sha(1),
        "factory-ci",
        "success",
        10,
        "all steps passed",
        "mallory",
    );
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    assert_eq!(rows(&fin).len(), 1);
}

#[test]
fn users_are_looked_up_once_per_token() {
    // FR-050: two repos on the plain token, one call
    let api = fake();
    add_repo(&api, "o/a", &[("main", 1, 10)], &["."]);
    add_repo(&api, "o/b", &[("main", 2, 10)], &["."]);
    plan_all(&api, &entries(&[("o/a", ""), ("o/b", "")]), &opts());
    assert_eq!(
        api.calls().iter().filter(|c| c.starts_with("user")).count(),
        1
    );
}

#[test]
fn an_entry_id_that_differs_from_the_api_id_skips_the_repo_naming_both() {
    // FR-052, M4
    let api = fake();
    api.set_repo(R, 999, false, "main");
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    let (_, fin) = plan_all(&api, &entries(&[(R, "id = 123")]), &opts());
    assert!(rows(&fin).is_empty());
    assert_eq!(fin.problems.len(), 1);
    assert!(
        fin.problems[0].contains("123") && fin.problems[0].contains("999"),
        "{:?}",
        fin.problems
    );
    assert!(
        api.calls().iter().all(|c| !c.starts_with("branches")),
        "a skipped repo costs only the lookup"
    );
}

#[test]
fn no_id_leaves_repo_id_empty_and_a_matching_id_fills_it() {
    let api = fake();
    api.set_repo(R, 123, false, "main");
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    assert_eq!(rows(&fin)[0].repo_id, "");
}

#[test]
fn the_default_branch_comes_from_the_repo_lookup() {
    // FR-060: default secret_branches is the repo's default branch
    let api = fake();
    api.set_repo(R, 1, false, "trunk");
    add_repo(&api, R, &[("main", 1, 30), ("trunk", 2, 20)], &["."]);
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    for r in rows(&fin) {
        assert_eq!(r.secrets_ok, r.branch == "trunk", "{}", r.branch);
    }
}

#[test]
fn phase_a_makes_no_write() {
    // FR-045, I28
    let api = fake();
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    candidates(&Apis::same(&api), &entries(&[(R, "")]), &opts());
    assert_eq!(api.writes(), 0);
}

#[test]
fn a_private_repo_on_a_public_b_without_a_log_store_is_dropped_with_a_problem() {
    // FR-049, H3
    let api = fake();
    api.set_repo(R, 1, true, "main");
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    let o = Opts {
        b_private: false,
        log_store: false,
        ..opts()
    };
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &o);
    assert!(rows(&fin).is_empty());
    assert_eq!(fin.problems.len(), 1);
    assert!(api.posts().is_empty(), "no pending for a dropped row");
}

#[test]
fn with_a_log_store_the_private_row_is_planned_fail_closed() {
    let api = fake();
    api.set_repo(R, 1, true, "main");
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    let o = Opts {
        b_private: false,
        log_store: true,
        ..opts()
    };
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &o);
    assert!(rows(&fin)[0].fail_closed);
    // a public repo is never fail_closed; a private central repo needs no gate
    let api = fake();
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    let (_, fin) = plan_all(
        &api,
        &entries(&[(R, "")]),
        &Opts {
            b_private: false,
            log_store: true,
            ..opts()
        },
    );
    assert!(!rows(&fin)[0].fail_closed);
    let api = fake();
    api.set_repo(R, 1, true, "main");
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    let (_, fin) = plan_all(
        &api,
        &entries(&[(R, "")]),
        &Opts {
            b_private: true,
            log_store: false,
            ..opts()
        },
    );
    assert!(!rows(&fin)[0].fail_closed);
}

#[test]
fn writes_are_spaced_750_ms_apart() {
    // FR-055: the clock is injected
    let api = fake();
    add_repo(&api, R, &[("a", 1, 30), ("b", 2, 20), ("c", 3, 10)], &["."]);
    SLEPT_MS.with(|s| s.set(0));
    let o = Opts {
        pace_ms: 750,
        sleep: record_sleep,
        ..opts()
    };
    plan_all(&api, &entries(&[(R, "")]), &o);
    assert_eq!(api.writes(), 3);
    assert!(
        SLEPT_MS.with(Cell::get) >= 2 * 750,
        "slept {}",
        SLEPT_MS.with(Cell::get)
    );
}

/// `n` unchanged branches (a success ancestor, no changed file) plus `k` changed ones.
fn mixed(api: &FakeApi, changed: u32, unchanged: u32) {
    let mut tips = vec![];
    for i in 0..changed {
        tips.push((format!("c{i:03}"), 1000 + i, 5000 - i as i64));
    }
    let t: Vec<(&str, u32, i64)> = tips.iter().map(|(b, n, a)| (b.as_str(), *n, *a)).collect();
    add_repo(api, R, &t, &["."]);
    for i in 0..unchanged {
        let (tip, anc) = (2000 + i, 3000 + i);
        api.set_branch(R, &format!("u{i:03}"), &sha(tip));
        api.set_tree(R, &sha(tip), &[(".factory-ci.toml", "blob")]);
        api.set_commits(
            R,
            &sha(tip),
            vec![(sha(tip), vec![sha(anc)], 10), (sha(anc), vec![], 20)],
        );
        api.set_status(
            R,
            &sha(anc),
            "factory-ci",
            "success",
            500,
            "all steps passed",
        );
        api.set_changed(R, &sha(anc), &sha(tip), &[]);
    }
}

#[test]
fn writes_are_capped_at_60_in_the_order_pending_then_unchanged() {
    // FR-055, I3: 20 pending + 45 unchanged wanted, 60 allowed
    let api = fake();
    mixed(&api, 20, 45);
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    assert_eq!(api.writes(), 60);
    assert_eq!(fin.deferred_writes, 5);
    let states: Vec<String> = api.posts().iter().map(|p| p.state.clone()).collect();
    assert!(states[..20].iter().all(|s| s == "pending"), "{states:?}");
    assert!(states[20..].iter().all(|s| s == "success"), "{states:?}");
    assert_eq!(rows(&fin).len(), 20);
}

#[test]
fn the_deferred_posts_are_made_by_the_next_tick() {
    let api = fake();
    mixed(&api, 0, 70);
    let list = entries(&[(R, "")]);
    plan_all(&api, &list, &opts());
    assert_eq!(api.writes(), 60);
    plan_all(&api, &list, &opts());
    assert_eq!(api.writes(), 70);
}

#[test]
fn closing_errors_come_after_pending_and_unchanged() {
    // data-model.md: pending, unchanged, closing errors
    let api = fake();
    mixed(&api, 2, 2);
    // an invalid folder name on a third tip gives a closing error
    api.set_branch(R, "z-bad", &sha(4000));
    api.set_tree(
        R,
        &sha(4000),
        &[("a b", "tree"), ("a b/.factory-ci.toml", "blob")],
    );
    api.set_commits(R, &sha(4000), vec![(sha(4000), vec![], 1)]);
    plan_all(&api, &entries(&[(R, "")]), &opts());
    let states: Vec<String> = api.posts().iter().map(|p| p.state.clone()).collect();
    assert_eq!(
        states,
        ["pending", "pending", "success", "success", "error"]
    );
}

#[test]
fn rows_split_by_allow_docker_use_the_docker_token_and_share_unique_numbers() {
    // FR-047, FR-048, plan.md decision 9
    let plain = fake();
    let docker = FakeApi::new("ci-docker-bot", NOW);
    add_repo(&plain, "o/plain", &[("main", 1, 10)], &["."]);
    add_repo(
        &docker,
        "o/dock",
        &[("main", 2, 10), ("dev", 3, 20)],
        &["."],
    );
    let list = entries(&[("o/plain", ""), ("o/dock", "allow_docker = true")]);
    let apis = Apis {
        plain: &plain,
        docker: &docker,
    };
    let fin = finish(&apis, candidates(&apis, &list, &opts()), &opts());
    assert_eq!(fin.matrix.len(), 1);
    assert_eq!(fin.matrix_docker.len(), 2);
    assert!(fin
        .matrix_docker
        .iter()
        .all(|r| r.allow_docker && r.repo == "o/dock"));
    assert!(
        plain.calls().iter().all(|c| !c.contains("o/dock")),
        "{:?}",
        plain.calls()
    );
    assert!(
        docker.calls().iter().all(|c| !c.contains("o/plain")),
        "{:?}",
        docker.calls()
    );
    assert!(docker.posts().len() == 2 && plain.posts().len() == 1);
    let mut n: Vec<usize> = rows(&fin).iter().map(|r| r.n as usize).collect();
    n.sort();
    assert_eq!(n, [0, 1, 2]);
}

#[test]
fn summary_lines_name_the_item_and_repo_only() {
    // FR-058, FR-059, I26
    let api = fake();
    add_repo(&api, R, &[("feature-xyz", 0xabc123, 10)], &["go-invoices"]);
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    let line = format!("item {}: {R}", rows(&fin)[0].n);
    assert!(
        fin.summary.lines().any(|l| l.contains(&line)),
        "{}",
        fin.summary
    );
    for secret in ["feature-xyz", "go-invoices", "abc123"] {
        assert!(
            !fin.summary.contains(secret),
            "{secret} leaked: {}",
            fin.summary
        );
    }
}

#[test]
fn problems_never_carry_branch_sha_or_project_text() {
    // I26
    let api = fake();
    add_repo(&api, R, &[("feature-xyz", 0xabc123, 10)], &["go-invoices"]);
    api.fail(
        &format!("post {R} {}", sha(0xabc123)),
        crate::github::ApiError::Http(500, "x".into()),
    );
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    let all = format!("{:?}{}", fin.problems, fin.summary);
    for secret in ["feature-xyz", "go-invoices", "abc123"] {
        assert!(!all.contains(secret), "{secret} leaked: {all}");
    }
}

#[test]
fn fail_tick_is_true_only_with_problems_in_the_first_ten_minutes_of_an_hour() {
    // A12
    let hour = NOW; // 00:00:00 UTC
    for (offset, with_problem, want) in [
        (0, true, true),
        (9 * 60 + 59, true, true),
        (10 * 60, true, false),
        (59 * 60, true, false),
        (3600 + 5 * 60, true, true),
        (0, false, false),
        (30 * 60, false, false),
    ] {
        let api = fake();
        add_repo(&api, R, &[("main", 1, 10)], &["."]);
        if with_problem {
            api.fail(
                "branches",
                crate::github::ApiError::Http(404, "gone".into()),
            );
        }
        let o = Opts {
            now: hour + offset,
            ..opts()
        };
        let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &o);
        assert_eq!(
            fin.fail_tick, want,
            "offset {offset}, problem {with_problem}"
        );
    }
}

#[test]
fn unchecked_repos_also_fail_the_tick_in_the_window() {
    let api = fake();
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    api.fail(
        "branches",
        crate::github::ApiError::RateLimited("limit".into()),
    );
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    assert!(!fin.unchecked.is_empty() && fin.fail_tick);
}

#[test]
fn the_loki_heartbeat_is_sent_once_from_phase_c_and_a_failure_is_ignored() {
    // FR-056
    let sent: Arc<Mutex<Vec<String>>> = Arc::default();
    let log = sent.clone();
    let hb: crate::plan::Heartbeat = Arc::new(move |line: &str| {
        log.lock().unwrap().push(line.to_string());
        Ok(())
    });
    let api = fake();
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    let o = Opts {
        log_store: true,
        heartbeat: Some(hb),
        ..opts()
    };
    plan_all(&api, &entries(&[(R, "")]), &o);
    let lines = sent.lock().unwrap().clone();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("run=") && lines[0].contains(" tick items=1 problems=0"),
        "{}",
        lines[0]
    );

    let failing: crate::plan::Heartbeat = Arc::new(|_| Err("loki down".into()));
    let api = fake();
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    let o = Opts {
        log_store: true,
        heartbeat: Some(failing),
        ..opts()
    };
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &o);
    assert_eq!(rows(&fin).len(), 1);
    assert!(
        !fin.problems.iter().any(|p| p.contains("loki")),
        "{:?}",
        fin.problems
    );
}

#[test]
fn no_heartbeat_without_log_credentials() {
    let sent: Arc<Mutex<Vec<String>>> = Arc::default();
    let log = sent.clone();
    let hb: crate::plan::Heartbeat = Arc::new(move |l: &str| {
        log.lock().unwrap().push(l.to_string());
        Ok(())
    });
    let api = fake();
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    plan_all(
        &api,
        &entries(&[(R, "")]),
        &Opts {
            log_store: false,
            heartbeat: Some(hb),
            ..opts()
        },
    );
    assert!(sent.lock().unwrap().is_empty());
}
