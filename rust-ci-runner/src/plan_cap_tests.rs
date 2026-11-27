//! T023. The cap, the interleaved order, `waiting` and `n`. FR-006, FR-012, FR-055;
//! AS 4.1 to 4.3, 9.4; I1, I3. Helpers come from `plan_tests`.

use crate::plan::Opts;
use crate::plan_tests::{add_repo, entries, fake, opts, plan_all, rows, sha};

fn tips_of(n: usize, prefix: &str, base: u32, oldest_age: i64) -> Vec<(String, u32, i64)> {
    // b00 is the oldest; ages shrink by 10 s per branch
    (0..n).map(|i| (format!("{prefix}{i:02}"), base + i as u32, oldest_age - 10 * i as i64)).collect()
}

fn add(api: &crate::fake::FakeApi, repo: &str, tips: &[(String, u32, i64)]) {
    let t: Vec<(&str, u32, i64)> = tips.iter().map(|(b, n, a)| (b.as_str(), *n, *a)).collect();
    add_repo(api, repo, &t, &["."]);
}

#[test]
fn rows_come_from_every_repo_with_news_and_an_idle_repo_costs_only_reads() {
    // AS 4.1, 4.2
    let api = fake();
    add_repo(&api, "o/a", &[("main", 1, 100)], &["."]);
    add_repo(&api, "o/b", &[("main", 2, 100)], &["."]);
    add_repo(&api, "o/c", &[("main", 3, 100)], &["."]);
    api.set_status("o/c", &sha(3), "factory-ci", "success", 50, "all steps passed");
    let (_, fin) = plan_all(&api, &entries(&[("o/a", ""), ("o/b", ""), ("o/c", "")]), &opts());
    let mut repos: Vec<_> = rows(&fin).iter().map(|r| r.repo.clone()).collect();
    repos.sort();
    assert_eq!(repos, ["o/a", "o/b"]);
    assert!(api.posts().iter().all(|p| p.repo != "o/c"));
    assert!(api.calls().iter().filter(|c| c.contains("o/c")).all(|c| !c.starts_with("post")));
}

#[test]
fn twenty_five_items_give_twenty_rows_interleaved_oldest_first() {
    // AS 4.3, FR-055, I3
    let api = fake();
    let (a, b) = (tips_of(15, "a", 100, 5000), tips_of(10, "b", 200, 4000));
    add(&api, "o/a", &a);
    add(&api, "o/b", &b);
    let (_, fin) = plan_all(&api, &entries(&[("o/a", ""), ("o/b", "")]), &opts());
    let rows = rows(&fin);
    assert_eq!(rows.len(), 20);
    assert_eq!(fin.waiting, 5);
    // starts with the repo whose oldest row is oldest (o/a), then alternates
    let order: Vec<&str> = rows.iter().map(|r| r.repo.as_str()).collect();
    for (i, r) in order.iter().enumerate() {
        assert_eq!(*r, if i % 2 == 0 { "o/a" } else { "o/b" }, "{order:?}");
    }
    // inside each repo, oldest commit first
    for repo in ["o/a", "o/b"] {
        let names: Vec<&str> = rows.iter().filter(|r| r.repo == repo).map(|r| r.branch.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "{repo}");
    }
    // pending went to exactly the 20 planned
    let pending: Vec<_> = api.posts().into_iter().filter(|p| p.state == "pending").collect();
    assert_eq!(pending.len(), 20);
    for r in &rows {
        assert!(pending.iter().any(|p| p.repo == r.repo && p.sha == r.sha && p.context == r.context));
    }
}

#[test]
fn one_repo_with_25_items_cannot_take_the_whole_cap() {
    // FR-055: a second repo with one item gets a row in the first two places
    let api = fake();
    add(&api, "o/big", &tips_of(25, "b", 100, 9000));
    add_repo(&api, "o/small", &[("main", 500, 10)], &["."]);
    let (_, fin) = plan_all(&api, &entries(&[("o/big", ""), ("o/small", "")]), &opts());
    let rows = rows(&fin);
    assert_eq!(rows.len(), 20);
    assert!(rows.iter().take(2).any(|r| r.repo == "o/small"), "{:?}", rows.iter().map(|r| &r.repo).collect::<Vec<_>>());
    assert_eq!(fin.waiting, 6);
}

#[test]
fn a_single_repo_with_25_items_is_capped_at_20_oldest_first() {
    let api = fake();
    add(&api, "o/a", &tips_of(25, "b", 100, 9000));
    let (_, fin) = plan_all(&api, &entries(&[("o/a", "")]), &opts());
    let names: Vec<&str> = rows(&fin).iter().map(|r| r.branch.as_str()).collect();
    assert_eq!(names.len(), 20);
    assert_eq!(names[0], "b00");
    assert_eq!(names[19], "b19");
    assert_eq!(fin.waiting, 5);
}

#[test]
fn ties_are_broken_by_branch_then_project() {
    // data-model.md Item: order (date, branch, project)
    let api = fake();
    add_repo(&api, "o/a", &[("zeta", 1, 100), ("alpha", 2, 100)], &[".", "a"]);
    let (_, fin) = plan_all(&api, &entries(&[("o/a", "")]), &opts());
    let got: Vec<(String, String)> = rows(&fin).iter().map(|r| (r.branch.clone(), r.project.clone())).collect();
    let want: Vec<(String, String)> =
        [("alpha", "."), ("alpha", "a"), ("zeta", "."), ("zeta", "a")].iter().map(|(b, p)| (b.to_string(), p.to_string())).collect();
    assert_eq!(got, want);
}

#[test]
fn the_cap_option_overrides_the_default() {
    let api = fake();
    add(&api, "o/a", &tips_of(10, "b", 100, 900));
    let o = Opts { cap: 3, ..opts() };
    let (_, fin) = plan_all(&api, &entries(&[("o/a", "")]), &o);
    assert_eq!(rows(&fin).len(), 3);
    assert_eq!(fin.waiting, 7);
}

#[test]
fn an_unlisted_repo_gets_no_call_and_a_removed_entry_is_never_touched() {
    // AS 9.4, I1
    let api = fake();
    add_repo(&api, "o/listed", &[("main", 1, 10)], &["."]);
    add_repo(&api, "o/removed", &[("main", 2, 10)], &["."]);
    plan_all(&api, &entries(&[("o/listed", "")]), &opts());
    assert!(api.calls().iter().all(|c| !c.contains("o/removed")), "{:?}", api.calls());
}

#[test]
fn item_numbers_are_unique_and_run_from_zero() {
    let api = fake();
    add(&api, "o/a", &tips_of(8, "a", 100, 900));
    add(&api, "o/b", &tips_of(8, "b", 200, 900));
    let (_, fin) = plan_all(&api, &entries(&[("o/a", ""), ("o/b", "")]), &opts());
    let mut n: Vec<usize> = rows(&fin).iter().map(|r| r.n as usize).collect();
    n.sort();
    assert_eq!(n, (0..16).collect::<Vec<_>>());
}

#[test]
fn the_cap_and_interleave_hold_for_random_fleets() {
    // I3: count <= cap, the first k rows come from k different repos, writes <= 60
    let mut x: u64 = 41;
    let mut next = |m: u64| {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (x >> 33) % m
    };
    for round in 0..25u32 {
        let api = fake();
        let repos = 1 + next(4) as usize;
        let mut list = vec![];
        let names: Vec<String> = (0..repos).map(|i| format!("o/r{i}")).collect();
        let mut with_items = 0;
        for (i, name) in names.iter().enumerate() {
            let k = next(14) as usize;
            if k > 0 {
                with_items += 1;
            }
            add(&api, name, &tips_of(k, "b", 1000 * round + 100 * i as u32 + 1, 3000));
            list.push((name.as_str(), ""));
        }
        let cap = 1 + next(25) as usize;
        let o = Opts { cap, ..opts() };
        let (_, fin) = plan_all(&api, &entries(&list), &o);
        let rows = rows(&fin);
        assert!(rows.len() <= cap);
        assert!(api.writes() <= 60);
        let k = with_items.min(rows.len());
        let mut firsts: Vec<&str> = rows.iter().take(k).map(|r| r.repo.as_str()).collect();
        firsts.sort();
        firsts.dedup();
        assert_eq!(firsts.len(), k, "round {round}");
    }
}
