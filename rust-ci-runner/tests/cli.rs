//! Built-binary tests, no real Docker. T012 (plan --check) and T038 (local run).

mod common;
use common::*;

fn run_ok(sb: &Sandbox, args: &[&str]) -> std::process::Output {
    sb.cmd(args).output().unwrap()
}

// ---- T012: plan --repos FILE --check (FR-004, FR-005, FR-029; AS 9.3) ----

#[test]
fn check_accepts_a_valid_list_without_token_or_network() {
    let sb = Sandbox::new();
    let f = sb.write("list.toml", "[[watch]]\nrepo = \"o/r\"\nbranches = [\"main\"]\n");
    let mut c = sb.cmd(&["plan", "--repos", dir_str(&f), "--check"]);
    c.env("GITHUB_API_URL", "http://127.0.0.1:1");
    let o = c.output().unwrap();
    assert!(o.status.success(), "{}", out_text(&o));
    assert!(sb.calls().is_empty());
}

#[test]
fn check_accepts_an_empty_list() {
    let sb = Sandbox::new();
    let f = sb.write("list.toml", "");
    assert!(run_ok(&sb, &["plan", "--repos", dir_str(&f), "--check"]).status.success());
}

#[test]
fn check_names_every_problem_and_exits_1() {
    let sb = Sandbox::new();
    let bad = "[[watch]]\nrepo = \"a/b/c\"\n[[watch]]\nrepo = \"x/y\"\nsecrets = [\"GITHUB_X\"]\nunknown = 1\n";
    let f = sb.write("bad.toml", bad);
    let o = run_ok(&sb, &["plan", "--repos", dir_str(&f), "--check"]);
    assert_eq!(o.status.code(), Some(1));
    let e = stderr(&o);
    for needle in ["a/b/c", "GITHUB_X", "unknown"] {
        assert!(e.contains(needle), "stderr lacks {needle}: {e}");
    }
}

#[test]
fn check_fails_on_a_missing_file() {
    let sb = Sandbox::new();
    let o = run_ok(&sb, &["plan", "--repos", "/nonexistent/list.toml", "--check"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(!stderr(&o).is_empty());
}

#[test]
fn check_with_a_partial_log_environment_names_the_missing_variables() {
    // FR-029
    let sb = Sandbox::new();
    let f = sb.write("list.toml", "");
    let mut c = sb.cmd(&["plan", "--repos", dir_str(&f), "--check"]);
    c.env("FACTORY_CI_LOG_URL", "http://127.0.0.1:1");
    let o = c.output().unwrap();
    assert_eq!(o.status.code(), Some(1));
    let e = stderr(&o);
    assert!(e.contains("FACTORY_CI_LOG_USER") && e.contains("FACTORY_CI_LOG_TOKEN"), "{e}");
    assert!(!e.contains("127.0.0.1"), "the URL must not be echoed: {e}");
}

#[test]
fn check_with_a_complete_log_environment_passes() {
    let sb = Sandbox::new();
    let f = sb.write("list.toml", "");
    let mut c = sb.cmd(&["plan", "--repos", dir_str(&f), "--check"]);
    log_env(&mut c, "http://127.0.0.1:1");
    assert!(c.output().unwrap().status.success());
}

// ---- T038: local `run` with a stub docker first on PATH (FR-025; AS 8.1 to 8.3) ----

fn local(sb: &Sandbox, toml: &str, extra: &[&str]) -> std::process::Output {
    let d = sb.dir.join("proj");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join(".factory-ci.toml"), toml).unwrap();
    let mut args = vec!["run", "--dir", dir_str(&d)];
    args.extend_from_slice(extra);
    sb.cmd(&args).output().unwrap()
}

#[test]
fn run_exits_0_when_every_step_succeeds() {
    // AS 8.1
    let sb = Sandbox::new();
    let o = local(&sb, &pipeline(&[("a", ""), ("b", "depends_on = [\"a\"]")]), &[]);
    assert!(o.status.success(), "{}", out_text(&o));
    assert!(sb.step_runs().len() >= 2);
}

#[test]
fn run_exits_non_zero_when_a_step_fails() {
    // AS 8.1
    let sb = Sandbox::new();
    let mut c = sb.cmd(&[]);
    let d = sb.dir.join("proj");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join(".factory-ci.toml"), pipeline(&[("ok", ""), ("bad", "")])).unwrap();
    c.args(["run", "--dir", dir_str(&d)]).env("STUB_FAIL", "-bad");
    let o = c.output().unwrap();
    assert!(!o.status.success(), "{}", out_text(&o));
    assert!(out_text(&o).contains("bad"));
}

#[test]
fn run_with_a_missing_secret_stops_before_any_engine_call_and_names_it() {
    // AS 8.2
    let sb = Sandbox::new();
    let toml = pipeline(&[("deploy", "secrets = [\"MISSING_TOKEN\"]")]);
    let o = local(&sb, &toml, &[]);
    assert!(!o.status.success());
    assert!(out_text(&o).contains("MISSING_TOKEN"), "{}", out_text(&o));
    assert!(sb.calls().is_empty(), "engine was called: {:?}", sb.calls());
}

#[test]
fn run_passes_a_listed_secret_by_name_only() {
    let sb = Sandbox::new();
    let d = sb.dir.join("proj");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join(".factory-ci.toml"), pipeline(&[("deploy", "secrets = [\"NPM_TOKEN\"]")])).unwrap();
    let mut c = sb.cmd(&["run", "--dir", dir_str(&d)]);
    c.env("NPM_TOKEN", "npm-value-9999");
    let o = c.output().unwrap();
    assert!(o.status.success(), "{}", out_text(&o));
    let runs = sb.step_runs();
    assert!(runs.iter().all(|r| !r.join("\n").contains("npm-value-9999")));
    assert!(runs.iter().any(|r| r.iter().any(|a| a == "NPM_TOKEN")));
}

#[test]
fn run_with_a_step_name_runs_only_that_step_and_its_dependencies() {
    // AS 8.3
    let sb = Sandbox::new();
    let toml = pipeline(&[("a", ""), ("b", "depends_on = [\"a\"]"), ("c", "")]);
    let o = local(&sb, &toml, &["b"]);
    assert!(o.status.success(), "{}", out_text(&o));
    let names: Vec<String> = sb
        .step_runs()
        .iter()
        .flat_map(|r| r.iter().filter(|a| a.starts_with("factory-ci-")).cloned())
        .collect();
    assert!(names.iter().any(|n| n.ends_with("-a")), "{names:?}");
    assert!(names.iter().any(|n| n.ends_with("-b")), "{names:?}");
    assert!(!names.iter().any(|n| n.ends_with("-c")), "{names:?}");
}

#[test]
fn run_needs_no_token_and_makes_no_network_call() {
    // FR-025, FR-046: empty environment, no GITHUB_API_URL, nothing listens.
    let sb = Sandbox::new();
    let o = local(&sb, &pipeline(&[("a", "")]), &[]);
    assert!(o.status.success(), "{}", out_text(&o));
}

// ---- T068: an allow_docker entry without FACTORY_CI_TOKEN_DOCKER (FR-047, R3) ----

#[test]
fn a_docker_entry_without_the_docker_token_is_a_problem_and_never_uses_the_plain_token() {
    let sb = Sandbox::new();
    // Only the docker entry: the fake answers `{}`, so any plain-token fallback shows up as a request.
    let list = "[[watch]]\nrepo = \"o/d\"\nallow_docker = true\n";
    let f = sb.write("list.toml", list);
    let out = sb.dir.join("cand.json");
    let fake = Fake::start();
    let mut c = sb.cmd(&["plan", "--repos", dir_str(&f), "--rows-out", dir_str(&out)]);
    c.env("FACTORY_CI_TOKEN", JOB_TOKEN).env("GITHUB_API_URL", &fake.url);
    let o = c.output().unwrap();
    assert!(o.status.success(), "{}", out_text(&o));
    let cand: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let problems = cand["problems"].to_string();
    assert!(problems.contains("o/d: FACTORY_CI_TOKEN_DOCKER is not set"), "{problems}");
    assert!(cand["rows"].as_array().unwrap().is_empty());
    let urls: Vec<String> = fake.requests().iter().map(|r| r.url.clone()).collect();
    assert!(urls.is_empty(), "the plain token was used for the docker entry: {urls:?}");
}

#[test]
fn phase_c_drops_a_docker_row_without_the_docker_token_and_keeps_the_plain_row() {
    // FR-047, T082: a candidates file with a docker row, no FACTORY_CI_TOKEN_DOCKER.
    let sb = Sandbox::new();
    let row = |repo: &str, docker: bool| {
        serde_json::json!({
            "repo": repo, "repo_id": "1", "sha": "0123456789abcdef0123456789abcdef01234567",
            "branch": "main", "project": "", "context": "factory-ci", "prev_sha": "", "allow_docker": docker,
            "fail_closed": false, "retry": false, "secrets_ok": false, "secrets": "", "run_id": "", "n": 0
        })
    };
    let cand = serde_json::json!({
        "rows": [row("o/d", true), row("o/p", false)], "unchanged": [], "invalid": [],
        "problems": [], "unchecked": [], "checked": 2
    });
    let input = sb.write("cand.json", &cand.to_string());
    let (rows, summary) = (sb.dir.join("rows.json"), sb.dir.join("summary.md"));
    let fake = Fake::start();
    let mut c = sb.cmd(&[
        "plan", "--rows-in", dir_str(&input), "--rows-out", dir_str(&rows), "--summary", dir_str(&summary),
    ]);
    c.env("FACTORY_CI_TOKEN", JOB_TOKEN).env("GITHUB_API_URL", &fake.url);
    let o = c.output().unwrap();
    assert!(o.status.success(), "{}", out_text(&o));
    let out: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&rows).unwrap()).unwrap();
    let repos: Vec<&str> = out.as_array().unwrap().iter().map(|r| r["repo"].as_str().unwrap()).collect();
    assert_eq!(repos, ["o/p"]);
    let text = std::fs::read_to_string(&summary).unwrap();
    assert!(text.contains("o/d: FACTORY_CI_TOKEN_DOCKER is not set"), "{text}");
    let urls: Vec<String> = fake.requests().iter().map(|r| r.url.clone()).collect();
    assert!(urls.iter().any(|u| u.contains("o/p")), "{urls:?}");
    assert!(urls.iter().all(|u| !u.contains("o/d")), "{urls:?}");
}

#[test]
fn phase_c_posts_nothing_for_docker_unchanged_and_invalid_entries_without_the_docker_token() {
    // FR-047, A27, T087: Docker entries in `unchanged` and `invalid` are dropped, the plain repo's are posted.
    let sb = Sandbox::new();
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let status = |repo: &str, state: &str, docker: bool| {
        serde_json::json!({
            "repo": repo, "sha": sha, "context": "factory-ci", "state": state, "description": "x", "docker": docker
        })
    };
    let cand = serde_json::json!({
        "rows": [],
        "unchanged": [status("o/d", "success", true), status("o/p", "success", false)],
        "invalid": [status("o/d", "error", true), status("o/p", "error", false)],
        "problems": [], "unchecked": [], "checked": 2
    });
    let input = sb.write("cand.json", &cand.to_string());
    let (rows, summary) = (sb.dir.join("rows.json"), sb.dir.join("summary.md"));
    let fake = Fake::start();
    let mut c = sb.cmd(&[
        "plan", "--rows-in", dir_str(&input), "--rows-out", dir_str(&rows), "--summary", dir_str(&summary),
    ]);
    c.env("FACTORY_CI_TOKEN", JOB_TOKEN).env("GITHUB_API_URL", &fake.url);
    let o = c.output().unwrap();
    assert!(o.status.success(), "{}", out_text(&o));
    let urls: Vec<String> = fake.requests().iter().map(|r| r.url.clone()).collect();
    assert_eq!(urls.iter().filter(|u| u.contains("o/p")).count(), 2, "{urls:?}");
    assert!(urls.iter().all(|u| !u.contains("o/d")), "{urls:?}");
}
