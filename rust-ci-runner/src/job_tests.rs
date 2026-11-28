//! T018. Targets `job::job(args, env) -> i32` (exit code) with a stub engine script and a
//! `tiny_http` status server. FR-016, FR-017, FR-022; AS 1.1 to 1.3, 1.5, 6.3; I11, I12.
//! Assumed: `pub fn job(args: &[String], env: &dyn Fn(&str) -> Option<String>) -> i32`, `args`
//! being the flags after `job`; `env` answers every variable the job reads (`FACTORY_CI_TOKEN`, `GITHUB_API_URL`, `FACTORY_CI_ENGINE`
//! = path of the engine binary, `FACTORY_CI_CLONE_IMAGE`, `RUNNER_TEMP`, and secret names).
//! A panic inside the run (`internal error`) needs a seam the plan does not define: not covered.

use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const PREV: &str = "fedcba9876543210fedcba9876543210fedcba98";
const PIN: &str = "r/i@sha256:0000000000000000000000000000000000000000000000000000000000000000";
const TOKEN: &str = "tok-JOB-UNIT-9";

struct Gh {
    url: String,
    posts: Arc<Mutex<Vec<serde_json::Value>>>,
}

/// Answers 500 to the first `fail_first` requests, then 201.
fn gh(fail_first: usize) -> Gh {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let posts: Arc<Mutex<Vec<serde_json::Value>>> = Arc::default();
    let log = posts.clone();
    std::thread::spawn(move || {
        let mut n = 0;
        for mut rq in server.incoming_requests() {
            let mut body = String::new();
            let _ = rq.as_reader().read_to_string(&mut body);
            log.lock()
                .unwrap()
                .push(serde_json::from_str(&body).unwrap_or_default());
            n += 1;
            let code = if n <= fail_first { 500 } else { 201 };
            let _ = rq.respond(tiny_http::Response::from_string("{}").with_status_code(code));
        }
    });
    Gh {
        url: format!("http://127.0.0.1:{port}"),
        posts,
    }
}

impl Gh {
    fn last(&self) -> serde_json::Value {
        self.posts
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or_default()
    }
    fn count(&self) -> usize {
        self.posts.lock().unwrap().len()
    }
    fn descriptions(&self) -> Vec<String> {
        self.posts
            .lock()
            .unwrap()
            .iter()
            .map(|p| p["description"].as_str().unwrap_or("").to_string())
            .collect()
    }
}

struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Sandbox {
        let dir =
            std::env::temp_dir().join(format!("fci-jobtest-{tag}-{}", crate::util::random_id()));
        std::fs::create_dir_all(dir.join("tmp")).unwrap();
        Sandbox { dir }
    }

    fn fixture(&self, files: &[(&str, &str)]) -> PathBuf {
        let d = self.dir.join("fixture");
        std::fs::create_dir_all(&d).unwrap();
        for (n, t) in files {
            std::fs::write(d.join(n), t).unwrap();
        }
        d
    }

    /// An engine stand-in. `clone_ok` copies the fixture; a call whose arguments contain `fail`
    /// exits with `code`; `step_out` is printed by every non-clone `run`.
    fn engine(
        &self,
        fixture: &Path,
        clone_ok: bool,
        fail: &str,
        code: i32,
        step_out: &str,
    ) -> String {
        let (d, f) = (self.dir.display(), fixture.display());
        let clone = if clone_ok {
            "prev=\"\"; for a in \"$@\"; do [ \"$prev\" = \"-v\" ] && ws=\"${a%%:/workspace}\"; prev=\"$a\"; done\n    cp -a \"$FIX\"/. \"$ws/\"; exit 0"
        } else {
            "exit 1"
        };
        let fail_case = if fail.is_empty() {
            String::new()
        } else {
            format!("case \"$*\" in *{fail}*) exit {code};; esac\n")
        };
        let script = format!(
            "#!/bin/sh\nd='{d}'; FIX='{f}'\nprintf '%s\\n' \"$@\" >> \"$d/calls\"; echo -- >> \"$d/calls\"\n/usr/bin/env >> \"$d/envs\"\n\
             case \"$*\" in *-clone*)\n    {clone};;\nesac\ncase \"$1\" in run) printf '%s\\n' '{step_out}';; esac\n{fail_case}exit 0\n"
        );
        let p = self
            .dir
            .join(format!("engine-{}.sh", crate::util::random_id()));
        std::fs::write(&p, script).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p.display().to_string()
    }

    fn calls(&self) -> Vec<Vec<String>> {
        let text = std::fs::read_to_string(self.dir.join("calls")).unwrap_or_default();
        text.split("--\n")
            .map(|c| c.lines().map(String::from).collect::<Vec<_>>())
            .filter(|c: &Vec<String>| !c.is_empty())
            .collect()
    }

    fn step_runs(&self) -> Vec<Vec<String>> {
        self.calls()
            .into_iter()
            .filter(|c| c[0] == "run" && !c.iter().any(|a| a.ends_with("-clone")))
            .collect()
    }

    fn envs(&self) -> String {
        std::fs::read_to_string(self.dir.join("envs")).unwrap_or_default()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn pipeline(steps: &[(&str, &str)]) -> String {
    steps
        .iter()
        .map(|(n, x)| format!("[steps.{n}]\nimage = \"alpine\"\ncommands = [\"echo {n}\"]\n{x}\n"))
        .collect()
}

fn run_job(sb: &Sandbox, gh: &Gh, engine: &str, extra: &[&str], vars: &[(&str, &str)]) -> i32 {
    let mut args: Vec<String> = [
        "--repo",
        "o/r",
        "--sha",
        SHA,
        "--project",
        ".",
        "--branch",
        "main",
        "--run-id",
        "0123456789abcdef",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.extend(extra.iter().map(|s| s.to_string()));
    let tmp = sb.dir.join("tmp").display().to_string();
    let vars: Vec<(String, String)> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let (url, engine) = (gh.url.clone(), engine.to_string());
    let env = move |k: &str| -> Option<String> {
        match k {
            "FACTORY_CI_TOKEN" => Some(TOKEN.into()),
            "GITHUB_API_URL" => Some(url.clone()),
            "FACTORY_CI_ENGINE" => Some(engine.clone()),
            "FACTORY_CI_CLONE_IMAGE" | "FACTORY_CI_DIND_IMAGE" => Some(PIN.into()),
            "RUNNER_TEMP" => Some(tmp.clone()),
            _ => vars.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()),
        }
    };
    crate::job::job(&args, &env)
}

fn setup(toml: &str) -> (Sandbox, Gh, PathBuf) {
    let sb = Sandbox::new("t");
    let fixture = sb.fixture(&[(".factory-ci.toml", toml)]);
    (sb, gh(0), fixture)
}

fn desc(g: &Gh) -> String {
    g.last()["description"].as_str().unwrap_or("").to_string()
}

#[test]
fn all_steps_passing_posts_success() {
    // AS 1.1
    let (sb, g, fx) = setup(&pipeline(&[("a", ""), ("b", "depends_on = [\"a\"]")]));
    assert_eq!(
        run_job(&sb, &g, &sb.engine(&fx, true, "", 0, "out"), &[], &[]),
        0
    );
    assert_eq!(
        (g.last()["state"].as_str(), desc(&g).as_str()),
        (Some("success"), "all steps passed")
    );
    assert_eq!(g.last()["context"], "factory-ci");
}

#[test]
fn a_failing_step_posts_failure_naming_the_first_failed_step() {
    // AS 1.2, FR-016
    let (sb, g, fx) = setup(&pipeline(&[("good", ""), ("bad", "")]));
    assert_eq!(
        run_job(&sb, &g, &sb.engine(&fx, true, "-bad", 1, ""), &[], &[]),
        0
    );
    assert_eq!(g.last()["state"], "failure");
    assert!(desc(&g).starts_with("step bad failed"), "{}", desc(&g));
}

#[test]
fn a_step_timeout_posts_failure() {
    // AS 1.2
    let (sb, g, fx) = setup(&pipeline(&[("slow", "timeout = 1")]));
    let engine = sb.engine(&fx, true, "", 0, "");
    let slow = std::fs::read_to_string(&engine).unwrap().replace(
        "case \"$1\" in run)",
        "case \"$1\" in run) sleep 3;;\n  xx)",
    );
    std::fs::write(&engine, slow).unwrap();
    assert_eq!(run_job(&sb, &g, &engine, &[], &[]), 0);
    assert_eq!(g.last()["state"], "failure");
    assert!(desc(&g).contains("timed out"), "{}", desc(&g));
}

#[test]
fn exit_126_and_127_stay_failure_but_125_is_an_error() {
    // FR-017
    for (code, state) in [
        (126, "failure"),
        (127, "failure"),
        (125, "error"),
        (1, "failure"),
    ] {
        let (sb, g, fx) = setup(&pipeline(&[("img", "")]));
        run_job(&sb, &g, &sb.engine(&fx, true, "-img", code, ""), &[], &[]);
        assert_eq!(g.last()["state"], state, "exit {code}: {}", desc(&g));
    }
}

#[test]
fn a_clone_failure_is_an_error_for_that_item_only() {
    // AS 6.3
    let (sb, g, fx) = setup(&pipeline(&[("a", "")]));
    run_job(&sb, &g, &sb.engine(&fx, false, "", 0, ""), &[], &[]);
    assert_eq!(g.last()["state"], "error");
    assert!(desc(&g).starts_with("clone failed"), "{}", desc(&g));
    assert!(sb.step_runs().is_empty());
    // a second item in the same process is unaffected
    let g2 = gh(0);
    assert_eq!(
        run_job(&sb, &g2, &sb.engine(&fx, true, "", 0, ""), &[], &[]),
        0
    );
    assert_eq!(g2.last()["state"], "success");
}

#[test]
fn an_invalid_pipeline_is_an_error_listing_every_problem_and_starts_no_container() {
    // AS 1.5, FR-022
    let big: String = (0..101)
        .map(|i| pipeline(&[(&format!("s{i}"), "")]))
        .collect();
    let cases = [
        ("invalid toml", "[steps.a\nimage".to_string()),
        ("cycle", format!("{}{}", pipeline(&[("a", "depends_on = [\"b\"]")]), pipeline(&[("b", "depends_on = [\"a\"]")]))),
        ("101 steps", big),
        ("two problems", "[steps.a]\nimage = \"x\"\ncommands = [\"y\"]\ndepends_on = [\"nope\"]\n[steps.b]\nimage = \"x\"\ncommands = [\"y\"]\ndepends_on = [\"gone\"]\n".to_string()),
    ];
    for (label, toml) in &cases {
        let (sb, g, fx) = setup(toml);
        assert_eq!(
            run_job(&sb, &g, &sb.engine(&fx, true, "", 0, ""), &[], &[]),
            0,
            "{label}"
        );
        assert_eq!(g.last()["state"], "error", "{label}");
        assert!(sb.step_runs().is_empty(), "{label}: container started");
    }
    let (sb, g, fx) = setup(&cases[3].1);
    run_job(&sb, &g, &sb.engine(&fx, true, "", 0, ""), &[], &[]);
    assert!(
        desc(&g).contains("nope") && desc(&g).contains("gone"),
        "{}",
        desc(&g)
    );
}

#[test]
fn a_symlinked_pipeline_file_is_an_error() {
    let sb = Sandbox::new("sym");
    let fx = sb.fixture(&[("real.toml", &pipeline(&[("a", "")]))]);
    std::os::unix::fs::symlink(fx.join("real.toml"), fx.join(".factory-ci.toml")).unwrap();
    let g = gh(0);
    run_job(&sb, &g, &sb.engine(&fx, true, "", 0, ""), &[], &[]);
    assert_eq!(g.last()["state"], "error");
    assert!(sb.step_runs().is_empty());
}

#[test]
fn the_final_state_is_never_pending() {
    // I12
    let toml = pipeline(&[("x", "")]);
    for (clone_ok, fail, code) in [
        (true, "", 0),
        (true, "-x", 1),
        (true, "-x", 125),
        (false, "", 0),
    ] {
        let (sb, g, fx) = setup(&toml);
        run_job(&sb, &g, &sb.engine(&fx, clone_ok, fail, code, ""), &[], &[]);
        let s = g.last()["state"].as_str().unwrap_or("none").to_string();
        assert!(["success", "failure", "error"].contains(&s.as_str()), "{s}");
    }
}

#[test]
fn the_final_post_is_retried_after_a_5xx() {
    // plan.md failure table "job: status post fails"
    let sb = Sandbox::new("retry");
    let fx = sb.fixture(&[(".factory-ci.toml", &pipeline(&[("a", "")]))]);
    let g = gh(1);
    assert_eq!(
        run_job(&sb, &g, &sb.engine(&fx, true, "", 0, ""), &[], &[]),
        0
    );
    assert!(g.count() >= 2);
    assert_eq!(g.last()["state"], "success");
}

#[test]
fn a_post_that_never_succeeds_exits_1() {
    let sb = Sandbox::new("dead");
    let fx = sb.fixture(&[(".factory-ci.toml", &pipeline(&[("a", "")]))]);
    let g = gh(100);
    assert_eq!(
        run_job(&sb, &g, &sb.engine(&fx, true, "", 0, ""), &[], &[]),
        1
    );
}

#[test]
fn the_run_environment_holds_sha_prev_sha_branch_and_the_push_event() {
    // data-model.md run_env
    let (sb, g, fx) = setup(&pipeline(&[("a", "")]));
    run_job(
        &sb,
        &g,
        &sb.engine(&fx, true, "", 0, ""),
        &["--prev", PREV],
        &[],
    );
    let run = &sb.step_runs()[0];
    let has = |kv: String| run.windows(2).any(|w| w[0] == "-e" && w[1] == kv);
    assert!(
        has(format!("FACTORY_CI_SHA={SHA}")) && has(format!("FACTORY_CI_PREV_SHA={PREV}")),
        "{run:?}"
    );
    assert!(
        has("FACTORY_CI_BRANCH=main".into()) && has("FACTORY_CI_EVENT=push".into()),
        "{run:?}"
    );
}

#[test]
fn the_clone_carries_the_token_in_the_environment_only() {
    let (sb, g, fx) = setup(&pipeline(&[("a", "")]));
    run_job(&sb, &g, &sb.engine(&fx, true, "", 0, ""), &[], &[]);
    assert!(
        sb.calls().iter().all(|c| !c.join("\n").contains(TOKEN)),
        "token in argv"
    );
    assert!(sb.envs().contains(&format!("FCI_TOKEN={TOKEN}")));
}

#[test]
fn a_pipeline_with_a_secret_is_refused_and_the_description_holds_neither_name_nor_value() {
    // FR-024; the job passes no secrets (T017b)
    let (sb, g, fx) = setup(&pipeline(&[("deploy", "secrets = [\"NPM_TOKEN\"]")]));
    let engine = sb.engine(&fx, true, "-deploy", 1, "login failed for npm-secret-77");
    run_job(&sb, &g, &engine, &[], &[("NPM_TOKEN", "npm-secret-77")]);
    assert_eq!(g.last()["state"], "error");
    let last = g.last().to_string();
    assert!(
        last.contains("steps with secrets are not run by this job"),
        "{last}"
    );
    assert!(
        !last.contains("npm-secret-77") && !last.contains("NPM_TOKEN"),
        "{last}"
    );
}

#[test]
fn retry_marks_every_posted_description_even_after_a_long_reason() {
    // A6c, data-model.md "Retry marker"
    let (sb, g, fx) = setup(&pipeline(&[("a", "")]));
    run_job(&sb, &g, &sb.engine(&fx, true, "", 0, ""), &["--retry"], &[]);
    assert!(
        g.descriptions().iter().all(|d| d.ends_with(" (retry)")),
        "{:?}",
        g.descriptions()
    );
    let names = ["a".repeat(60), "b".repeat(60), "c".repeat(60)];
    let toml: String = names
        .iter()
        .map(|n| pipeline(&[(n.as_str(), "depends_on = [\"missing\"]")]))
        .collect();
    let (sb, g, fx) = setup(&toml);
    run_job(&sb, &g, &sb.engine(&fx, true, "", 0, ""), &["--retry"], &[]);
    let d = desc(&g);
    assert!(d.ends_with(" (retry)") && d.chars().count() <= 140, "{d}");
}

#[test]
fn descriptions_are_sanitized() {
    // FR-051: a step name cannot smuggle a command marker or markup
    let (sb, g, fx) = setup(&pipeline(&[("bad", "")]));
    run_job(&sb, &g, &sb.engine(&fx, true, "-bad", 1, ""), &[], &[]);
    assert!(
        desc(&g)
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " _.:/-()".contains(c)),
        "{}",
        desc(&g)
    );
}

#[test]
fn an_engine_that_cannot_be_started_is_an_error_not_a_failure() {
    // FR-017, R10: the engine deletes itself at `network create`, after the clone, so only the step spawn fails
    let (sb, g, fx) = setup(&pipeline(&[("a", "")]));
    let p = sb.dir.join("engine-gone.sh");
    let script = format!(
        "#!/bin/sh\ncase \"$*\" in *-clone*)\n  prev=\"\"; for a in \"$@\"; do [ \"$prev\" = \"-v\" ] && ws=\"${{a%%:/workspace}}\"; prev=\"$a\"; done\n  cp -a '{}'/. \"$ws/\";;\nnetwork*) rm -f \"$0\";;\nesac\nexit 0\n",
        fx.display()
    );
    std::fs::write(&p, script).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    run_job(&sb, &g, &p.display().to_string(), &[], &[]);
    assert_eq!(g.last()["state"], "error", "{}", desc(&g));
    assert!(desc(&g).contains("could not start"), "{}", desc(&g));
}

// ---- 008 T007 (test-first): `local` fails when a passing run dropped log lines. FR-017 ----
// Assumed seam: `job::local_with(dir, step, engine, quiet, env: Env, timing: ship::Timing) -> Result<bool, String>`;
// `local` calls it with the process environment and `Timing::default()`.

fn dead_loki(code: u16) -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    std::thread::spawn(move || {
        for mut rq in server.incoming_requests() {
            let mut body = String::new();
            let _ = rq.as_reader().read_to_string(&mut body);
            let _ = rq.respond(tiny_http::Response::from_string("").with_status_code(code));
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn quiet_run(sb: &Sandbox, fixture: &Path, run_exit: i32, loki_code: u16) -> Result<bool, String> {
    let engine = sb.dir.join("engine-q.sh");
    std::fs::write(
        &engine,
        format!("#!/bin/sh\ncase \"$1\" in run) exit {run_exit};; esac\nexit 0\n"),
    )
    .unwrap();
    std::fs::set_permissions(&engine, std::fs::Permissions::from_mode(0o755)).unwrap();
    let url = dead_loki(loki_code);
    let env = move |k: &str| -> Option<String> {
        match k {
            "FACTORY_CI_LOG_URL" => Some(url.clone()),
            "FACTORY_CI_LOG_USER" => Some("u-1234".into()),
            "FACTORY_CI_LOG_TOKEN" => Some("t-1234".into()),
            _ => None,
        }
    };
    let timing = crate::ship::Timing {
        batch: std::time::Duration::from_millis(50),
        retry_window: std::time::Duration::from_millis(300),
        backoff: std::time::Duration::from_millis(20),
        request_timeout: std::time::Duration::from_millis(500),
        ..crate::ship::Timing::default()
    };
    crate::job::local_with(
        fixture,
        None,
        &engine.display().to_string(),
        true,
        &env,
        timing,
    )
}

#[test]
fn run_quiet_logs_fails_when_a_passing_run_dropped_lines() {
    let sb = Sandbox::new("qdrop");
    let fx = sb.fixture(&[(".factory-ci.toml", &pipeline(&[("a", "")]))]);
    // the store answers 500 until the retry window ends: lines are dropped, the pipeline passed
    assert_eq!(
        quiet_run(&sb, &fx, 0, 500),
        Err("log shipping dropped lines".to_string())
    );
    // a store that takes the lines: a pass stays a pass
    let sb = Sandbox::new("qok");
    let fx = sb.fixture(&[(".factory-ci.toml", &pipeline(&[("a", "")]))]);
    assert_eq!(quiet_run(&sb, &fx, 0, 204), Ok(true));
    // a failed pipeline stays a plain failure, dropped lines or not
    let sb = Sandbox::new("qfail");
    let fx = sb.fixture(&[(".factory-ci.toml", &pipeline(&[("a", "")]))]);
    assert_eq!(quiet_run(&sb, &fx, 1, 500), Ok(false));
}

#[test]
fn project_dir_refuses_a_symlinked_intermediate_directory() {
    // 012 add-on: `docker` is a link to a folder inside the checkout, `docker/hello` is a plain directory
    // whose canonical path stays inside it, but not at `<ws>/docker/hello`
    let ws = std::env::temp_dir().join(format!("fci-pdir-{}", crate::util::random_id()));
    std::fs::create_dir_all(ws.join("real/hello")).unwrap();
    std::fs::create_dir_all(ws.join("plain")).unwrap();
    std::os::unix::fs::symlink(ws.join("real"), ws.join("docker")).unwrap();
    assert!(crate::job::project_dir(&ws, "docker/hello").is_err());
    assert!(crate::job::project_dir(&ws, "plain").is_ok());
    assert!(crate::job::project_dir(&ws, ".").is_ok());
    let _ = std::fs::remove_dir_all(&ws);
}

#[test]
fn job_and_status_silence_the_panic_hook() {
    // 012 T042, security S7: a panic message could carry a value read from the environment
    let src = include_str!("job.rs");
    for f in ["pub fn job(", "pub fn status("] {
        let at = src.find(f).unwrap();
        let head: String = src[at..].chars().take(300).collect();
        assert!(
            head.contains("std::panic::set_hook(Box::new(|_| {}))"),
            "{f}"
        );
    }
}
