//! T025. Overlapping ticks and the retry budget at plan level. FR-009; AS 5.2 to 5.4, spec edge
//! case "two ticks racing"; I24. Helpers come from `plan_tests`.

use crate::plan_tests::{add_repo, entries, fake, opts, plan_all, rows, sha};

const R: &str = "o/r";

#[test]
fn a_fresh_pending_is_not_planned() {
    // AS 5.2
    let api = fake();
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    api.set_status(R, &sha(1), "factory-ci", "pending", 100, "queued");
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    assert!(rows(&fin).is_empty());
    assert_eq!(api.writes(), 0);
}

#[test]
fn a_pending_older_than_7200_s_is_planned_again_with_a_new_pending_post() {
    // AS 5.3
    let api = fake();
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    api.set_status(R, &sha(1), "factory-ci", "pending", 7201, "queued");
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    assert_eq!(rows(&fin).len(), 1);
    assert!(rows(&fin)[0].retry);
    let posts = api.posts();
    assert_eq!(posts.len(), 1);
    assert_eq!((posts[0].state.as_str(), posts[0].description.as_str()), ("pending", "queued (retry)"));
}

#[test]
fn the_next_tick_plans_the_rest_and_nothing_twice() {
    // AS 5.4: 25 items, cap 20; the second tick sees the first tick's pending posts
    let api = fake();
    let tips: Vec<(String, u32, i64)> = (0..25).map(|i| (format!("b{i:02}"), 100 + i as u32, 1000 - 10 * i as i64)).collect();
    let t: Vec<(&str, u32, i64)> = tips.iter().map(|(b, n, a)| (b.as_str(), *n, *a)).collect();
    add_repo(&api, R, &t, &["."]);
    let list = entries(&[(R, "")]);
    let (_, one) = plan_all(&api, &list, &opts());
    let first: Vec<String> = rows(&one).iter().map(|r| r.sha.clone()).collect();
    assert_eq!(first.len(), 20);
    let (_, two) = plan_all(&api, &list, &opts());
    let second: Vec<String> = rows(&two).iter().map(|r| r.sha.clone()).collect();
    assert_eq!(second.len(), 5);
    assert!(second.iter().all(|s| !first.contains(s)));
    let (_, three) = plan_all(&api, &list, &opts());
    assert!(rows(&three).is_empty());
}

#[test]
fn every_row_has_its_pending_post_before_the_rows_are_returned() {
    // FR-012, I2: the output exists only for items whose post is already on the fake
    let api = fake();
    add_repo(&api, R, &[("a", 1, 30), ("b", 2, 20), ("c", 3, 10)], &["."]);
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    let posts = api.posts();
    for r in rows(&fin) {
        assert!(posts.iter().any(|p| p.sha == r.sha && p.context == r.context && p.state == "pending"));
    }
    assert_eq!(posts.len(), rows(&fin).len());
}

#[test]
fn after_a_retry_post_the_next_tick_sees_a_fresh_pending_and_skips_it() {
    // A6c
    let api = fake();
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    api.set_status(R, &sha(1), "factory-ci", "error", 5000, "clone failed");
    let list = entries(&[(R, "")]);
    let (_, one) = plan_all(&api, &list, &opts());
    assert_eq!(rows(&one).len(), 1);
    assert!(rows(&one)[0].retry);
    assert_eq!(api.posts()[0].description, "queued (retry)");
    let (_, two) = plan_all(&api, &list, &opts());
    assert!(rows(&two).is_empty());
}

#[test]
fn a_second_error_marked_retry_stays_done() {
    // I24
    let api = fake();
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    api.set_status(R, &sha(1), "factory-ci", "error", 90_000, "clone failed (retry)");
    let (_, fin) = plan_all(&api, &entries(&[(R, "")]), &opts());
    assert!(rows(&fin).is_empty());
    assert_eq!(api.writes(), 0);
}

#[test]
fn an_old_marked_pending_is_closed_with_one_error_post_and_no_row() {
    // data-model.md Decision per project: closing error counts as a write
    let api = fake();
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    api.set_status(R, &sha(1), "factory-ci", "pending", 9000, "queued (retry)");
    let list = entries(&[(R, "")]);
    let (_, fin) = plan_all(&api, &list, &opts());
    assert!(rows(&fin).is_empty());
    let posts = api.posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].state, "error");
    // A6c: the closing error carries the marker, so 30 minutes later it is still final
    assert_eq!(posts[0].description, "job did not finish (retry)");
    let mut later = opts();
    later.now += 2000;
    let aged = crate::fake::FakeApi::new("ci-bot", later.now);
    add_repo(&aged, R, &[("main", 1, 2010)], &["."]);
    aged.set_status(R, &sha(1), "factory-ci", "error", 1900, &posts[0].description);
    let (_, old) = plan_all(&aged, &list, &later);
    assert!(rows(&old).is_empty(), "a closed item was planned a third time");
    assert_eq!(aged.writes(), 0);
    // and the next tick leaves it alone: a fresh error ending in (retry) is final
    let (_, again) = plan_all(&api, &list, &opts());
    assert!(rows(&again).is_empty());
    assert_eq!(api.posts().len(), 1);
}

#[test]
fn a_crash_after_the_posts_loses_nothing_a_later_tick_cannot_recover() {
    // plan.md failure table "plan C: crash after posts": the rows are gone, pending ages out
    let api = fake();
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    let list = entries(&[(R, "")]);
    plan_all(&api, &list, &opts()); // rows dropped on the floor
    let mut late = opts();
    late.now += 7300;
    let aged = crate::fake::FakeApi::new("ci-bot", late.now);
    add_repo(&aged, R, &[("main", 1, 7310)], &["."]);
    aged.set_status(R, &sha(1), "factory-ci", "pending", 7300, "queued");
    let (_, fin) = plan_all(&aged, &list, &late);
    assert_eq!(rows(&fin).len(), 1);
    assert!(rows(&fin)[0].retry);
}
