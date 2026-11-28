//! T048. Watch entry `id` and `secret_branches`, and the row fields they feed. FR-040, FR-041,
//! FR-052, FR-060; AS 12.2, 12.11. Nothing here mentions a vendor
//! (FR-038, tests/no_vendor.rs skips `*_tests.rs`). Helpers come from `plan_tests`.

use crate::plan_tests::{add_repo, entries, fake, opts, plan_all, rows};
use crate::watch::parse;

const R: &str = "o/r";

fn entry(extra: &str) -> String {
    format!("[[watch]]\nrepo = \"{R}\"\n{extra}\n")
}

#[test]
fn id_parses_as_a_number_and_a_string_is_refused() {
    assert_eq!(parse(&entry("id = 42")).unwrap()[0].id, Some(42));
    assert!(parse(&entry("id = \"42\"")).is_err());
    assert!(parse(&entry("id = -1")).is_err());
}

#[test]
fn secret_branches_parse_and_follow_the_glob_rules_of_branches() {
    let v = parse(&entry("secret_branches = [\"main\", \"release/*\"]")).unwrap();
    assert_eq!(
        v[0].secret_branches,
        Some(vec!["main".to_string(), "release/*".to_string()])
    );
    for bad in ["\"\"", "\"a[b\"", "\"a b\"", "\"a..b\""] {
        assert!(
            parse(&entry(&format!("secret_branches = [{bad}]"))).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn bitwarden_project_is_an_unknown_key() {
    // FR-040: no per-repo projects
    let p = parse(&entry("bitwarden_project = \"x\"")).unwrap_err();
    assert!(p.iter().any(|l| l.contains("bitwarden_project")), "{p:?}");
}

#[test]
fn the_store_token_name_in_secrets_is_a_problem() {
    let p = parse(&entry("secrets = [\"BWS_ACCESS_TOKEN\"]")).unwrap_err();
    assert!(p.iter().any(|l| l.contains("BWS_ACCESS_TOKEN")), "{p:?}");
}

#[test]
fn rows_carry_repo_id_only_when_the_api_id_matches() {
    let api = fake();
    api.set_repo(R, 7, false, "main");
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    let (_, ok) = plan_all(&api, &entries(&[(R, "id = 7")]), &opts());
    assert_eq!(rows(&ok)[0].repo_id, "7");
    let (_, none) = plan_all(&fake_with(7), &entries(&[(R, "")]), &opts());
    assert_eq!(rows(&none)[0].repo_id, "");
    let bad = fake_with(8);
    let (_, fin) = plan_all(&bad, &entries(&[(R, "id = 7")]), &opts());
    assert!(rows(&fin).is_empty());
    assert_eq!(fin.problems.len(), 1);
}

fn fake_with(id: u64) -> crate::fake::FakeApi {
    let api = fake();
    api.set_repo(R, id, false, "main");
    add_repo(&api, R, &[("main", 1, 10)], &["."]);
    api
}

#[test]
fn rows_carry_the_entry_shared_names_space_separated() {
    let api = fake_with(1);
    let (_, fin) = plan_all(
        &api,
        &entries(&[(R, "secrets = [\"NPM_TOKEN\", \"DEPLOY_KEY\"]")]),
        &opts(),
    );
    assert_eq!(rows(&fin)[0].secrets, "NPM_TOKEN DEPLOY_KEY");
    let (_, none) = plan_all(&fake_with(1), &entries(&[(R, "")]), &opts());
    assert_eq!(rows(&none)[0].secrets, "");
}

#[test]
fn secret_branches_default_to_the_default_branch_and_an_explicit_list_wins() {
    // FR-060, AS 12.11, I29
    let api = fake();
    api.set_repo(R, 1, false, "main");
    add_repo(
        &api,
        R,
        &[("main", 1, 30), ("dev", 2, 20), ("release/1", 3, 10)],
        &["."],
    );
    let (_, d) = plan_all(&api, &entries(&[(R, "")]), &opts());
    for r in rows(&d) {
        assert_eq!(r.secrets_ok, r.branch == "main", "{}", r.branch);
    }
    let api = fake();
    api.set_repo(R, 1, false, "main");
    add_repo(
        &api,
        R,
        &[("main", 1, 30), ("dev", 2, 20), ("release/1", 3, 10)],
        &["."],
    );
    let (_, e) = plan_all(
        &api,
        &entries(&[(R, "secret_branches = [\"release/*\", \"dev\"]")]),
        &opts(),
    );
    for r in rows(&e) {
        assert_eq!(r.secrets_ok, r.branch != "main", "{}", r.branch);
    }
}

#[test]
fn phase_a_makes_no_write_and_never_names_a_store() {
    let api = fake_with(1);
    let apis = crate::plan::Apis::same(&api);
    let cand = crate::plan::candidates(&apis, &entries(&[(R, "")]), &opts());
    assert_eq!(cand.rows.len(), 1);
    assert_eq!(api.writes(), 0);
}

// ---- 010 T003 (test-first, `#[ignore = "test-first"]`): FR-004. `BW_` and `BITWARDEN` are reserved. ----

#[test]
#[ignore = "test-first"]
fn bw_and_bitwarden_names_are_refused_in_an_entry_and_a_pipeline() {
    for n in ["BW_X", "BW_", "bw_x", "BITWARDEN", "bitwarden_x", "Bitwarden", "BITWARDENX"] {
        let p = parse(&entry(&format!("secrets = [{n:?}]"))).unwrap_err();
        assert!(p.iter().any(|l| l.contains(n)), "{n} in an entry: {p:?}");
        let t = format!("[steps.a]\nimage = \"x\"\ncommands = [\"y\"]\nsecrets = [{n:?}]\n");
        let v = crate::pipeline::parse(&t).unwrap_err();
        assert!(v.iter().any(|l| l.contains(n)), "{n} in a pipeline: {v:?}");
        assert!(!crate::pipeline::valid_secret_name(n), "{n}");
    }
    for n in ["BWX", "MY_BW_KEY", "SITE_API_KEY"] {
        assert!(parse(&entry(&format!("secrets = [{n:?}]"))).is_ok(), "{n}");
        assert!(crate::pipeline::valid_secret_name(n), "{n}");
    }
}
