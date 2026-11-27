use rust_ci_runner::github::HttpApi;
use rust_ci_runner::{ci, job, plan, ship, watch};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage:
  rust-ci-runner run [--dir PATH] [--quiet-logs] [STEP]  run .factory-ci.toml in PATH (default .), or one STEP and its dependencies
  rust-ci-runner plan --repos FILE --check  validate the watch list FILE and the log settings, no network
  rust-ci-runner plan --repos FILE --rows-out CAND  phase A: candidates, reads only
  rust-ci-runner plan --rows-in CAND --rows-out ROWS --summary FILE [--cap N] [--now EPOCH]  phase C: cap, posts; prints fail_tick=0|1
  rust-ci-runner job --repo OWNER/NAME --sha SHA --project PATH --branch NAME --run-id ID [--prev SHA] [--allow-docker] [--quiet-logs] [--private] [--retry] [--names-file FILE --reason-file FILE]  clone, run and post the final status; with the two files, only list the pipeline's secret names
  rust-ci-runner ci --repos FILE --repo OWNER/NAME --ref REF [--project PATH] --work DIR [--allow-docker]  the manual CI job: fetch the settings, clone, run, print only fixed lines; exit 0 or 1
  rust-ci-runner status --repo OWNER/NAME --sha SHA --project PATH --run-id ID --state error|failure|success --description TEXT [--retry]  post one status
  rust-ci-runner help

environment: FACTORY_CI_ENGINE=docker|podman (default docker; `job` also accepts a path; `ci` always uses docker); `ci` reads BWS_ACCESS_TOKEN, and optionally FACTORY_CI_BWS_SERVER_URL, FACTORY_CI_DIND_IMAGE, GITHUB_RUN_ID; `plan`, `job` and `status` read FACTORY_CI_TOKEN. `run` reads secrets from
the environment variables named in the pipeline file.";

fn fail(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::FAILURE
}

fn run(args: &[String]) -> ExitCode {
    let mut dir = PathBuf::from(".");
    let mut step = None;
    let mut quiet = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--quiet-logs" => quiet = true,
            "--dir" => match it.next() {
                Some(d) => dir = PathBuf::from(d),
                None => return fail(USAGE),
            },
            s if !s.starts_with('-') && step.is_none() => step = Some(s.to_string()),
            _ => return fail(USAGE),
        }
    }
    let engine = std::env::var("FACTORY_CI_ENGINE").unwrap_or_else(|_| "docker".to_string());
    if engine != "docker" && engine != "podman" {
        return fail("FACTORY_CI_ENGINE must be docker or podman");
    }
    match job::local(&dir, step.as_deref(), &engine, quiet) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => fail(&e),
    }
}

/// Reads and validates the watch list and the log settings.
fn load_list(repos: &Path) -> Result<Vec<watch::WatchEntry>, String> {
    ship::LogConfig::from_env(&|n| std::env::var(n).ok()).map_err(|e| e.join(". "))?;
    watch::read(repos)
}

/// Settings from the environment: token and log values are redacted, the log store counts as
/// resolved when all three `FACTORY_CI_LOG_*` values are set.
fn plan_opts() -> plan::Opts {
    let var = |n: &str| std::env::var(n).unwrap_or_default();
    let logs = ["FACTORY_CI_LOG_URL", "FACTORY_CI_LOG_USER", "FACTORY_CI_LOG_TOKEN"].map(var);
    let run_id = var("GITHUB_RUN_ID");
    let heartbeat: Option<plan::Heartbeat> = match ship::LogConfig::from_env(&|k| std::env::var(k).ok()) {
        Ok(Some(cfg)) => {
            let id = run_id.clone();
            Some(std::sync::Arc::new(move |line: &str| ship::send_heartbeat(&cfg, &id, line)))
        }
        _ => None,
    };
    plan::Opts {
        b_private: var("B_PRIVATE") != "false",
        log_store: logs.iter().all(|v| !v.is_empty()),
        secrets: logs.into_iter().chain([var("FACTORY_CI_TOKEN"), var("FACTORY_CI_TOKEN_DOCKER")]).collect(),
        heartbeat,
        run_id,
        ..Default::default()
    }
}

/// The client for `allow_docker` entries; `None` without `FACTORY_CI_TOKEN_DOCKER` (phase A skips those entries).
fn docker_api() -> Option<HttpApi> {
    HttpApi::from_env("FACTORY_CI_TOKEN_DOCKER").ok()
}

/// The plan files hold private data: owner-only.
fn write_private(path: &Path, text: &str) -> Result<(), String> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    f.write_all(text.as_bytes()).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn plan_a(repos: &Path, out: &Path) -> Result<(), String> {
    let list = load_list(repos)?;
    let api = HttpApi::from_env("FACTORY_CI_TOKEN")?;
    let docker = docker_api();
    // FR-047: no docker token, no row; the plain token never stands in.
    let (list, skipped): (Vec<_>, Vec<_>) = list.into_iter().partition(|e| !e.allow_docker || docker.is_some());
    // The list holds no docker entry when `docker` is None, so `Apis::same` never serves one.
    let apis = match docker.as_ref() {
        Some(d) => plan::Apis { plain: &api, docker: d },
        None => plan::Apis::same(&api),
    };
    let mut c = plan::candidates(&apis, &list, &plan_opts());
    c.problems.extend(skipped.iter().map(|e| format!("{}: FACTORY_CI_TOKEN_DOCKER is not set", e.repo)));
    write_private(out, &serde_json::to_string(&c).map_err(|e| e.to_string())?)
}

fn plan_c(input: &Path, out: &Path, summary: &Path, cap: Option<usize>, now: Option<i64>) -> Result<bool, String> {
    let text = std::fs::read_to_string(input).map_err(|e| format!("cannot read {}: {e}", input.display()))?;
    let mut cand: plan::Candidates = serde_json::from_str(&text).map_err(|e| format!("bad candidates file: {e}"))?;
    let api = HttpApi::from_env("FACTORY_CI_TOKEN")?;
    let mut o = plan_opts();
    o.cap = cap.unwrap_or(o.cap);
    o.now = now.unwrap_or(o.now);
    let docker = docker_api();
    // FR-047: no docker token, no row and no post for a docker repo; the plain token never stands in.
    let apis = match docker.as_ref() {
        Some(d) => plan::Apis { plain: &api, docker: d },
        None => {
            let mut gone: Vec<String> = cand
                .rows
                .iter()
                .filter(|r| r.allow_docker)
                .map(|r| r.repo.clone())
                .chain(cand.unchanged.iter().chain(&cand.invalid).filter(|s| s.docker).map(|s| s.repo.clone()))
                .collect();
            gone.sort();
            gone.dedup();
            cand.rows.retain(|r| !r.allow_docker);
            cand.unchanged.retain(|s| !s.docker);
            // A27: a stale `pending` on a Docker repo stays until a tick runs with the Docker token.
            cand.invalid.retain(|s| !s.docker);
            cand.problems.extend(gone.iter().map(|r| format!("{r}: FACTORY_CI_TOKEN_DOCKER is not set")));
            plan::Apis::same(&api)
        }
    };
    let fin = plan::finish(&apis, cand, &o);
    let mut rows: Vec<&plan::Item> = fin.matrix.iter().chain(&fin.matrix_docker).collect();
    rows.sort_by_key(|r| r.n);
    write_private(out, &serde_json::to_string(&rows).map_err(|e| e.to_string())?)?;
    write_private(summary, &fin.summary)?;
    Ok(fin.fail_tick)
}

/// `plan`: `--repos FILE --check`, phase A (`--repos`, `--rows-out`) or phase C (`--rows-in`,
/// `--rows-out`, `--summary`, optional `--cap` and `--now`).
fn plan(args: &[String]) -> ExitCode {
    let (mut repos, mut rows_in, mut rows_out, mut summary) = (None, None, None, None);
    let (mut check, mut cap, mut now) = (false, None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = || it.next().map(String::as_str);
        match a.as_str() {
            "--repos" => repos = val().map(PathBuf::from),
            "--rows-in" => rows_in = val().map(PathBuf::from),
            "--rows-out" => rows_out = val().map(PathBuf::from),
            "--summary" => summary = val().map(PathBuf::from),
            "--cap" => match val().and_then(|v| v.parse().ok()) {
                Some(n) => cap = Some(n),
                None => return fail(USAGE),
            },
            "--now" => match val().and_then(|v| v.parse().ok()) {
                Some(n) => now = Some(n),
                None => return fail(USAGE),
            },
            "--check" => check = true,
            _ => return fail(USAGE),
        }
    }
    let done = |r: Result<(), String>| match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e),
    };
    match (repos, rows_in, rows_out, summary) {
        (Some(r), None, None, None) if check => match load_list(&r) {
            Ok(v) => {
                println!("{} entries ok", v.len());
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e),
        },
        (Some(r), None, Some(o), None) if !check => done(plan_a(&r, &o)),
        (None, Some(i), Some(o), Some(s)) if !check => match plan_c(&i, &o, &s, cap, now) {
            Ok(f) => {
                println!("fail_tick={}", u8::from(f));
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e),
        },
        _ => fail(USAGE),
    }
}

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

/// `ci`: the whole manual CI job. The access token is read once and removed from the process
/// environment before anything else starts; a panic message never reaches the public log, `ci::run`
/// prints the fixed line instead.
fn run_ci(args: &[String]) -> ExitCode {
    let token = std::env::var("BWS_ACCESS_TOKEN").unwrap_or_default();
    std::env::remove_var("BWS_ACCESS_TOKEN");
    std::panic::set_hook(Box::new(|_| {}));
    let seams = ci::Seams::production();
    ExitCode::from(ci::run(args, &token, &env, &seams, Box::new(std::io::stdout()), Box::new(std::io::stderr())) as u8)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("run") => run(&args[1..]),
        Some("plan") => plan(&args[1..]),
        Some("ci") => run_ci(&args[1..]),
        Some("job") =>ExitCode::from(job::job(&args[1..], &env) as u8),
        Some("status") => ExitCode::from(job::status(&args[1..], &env) as u8),
        Some("help") | Some("--help") | Some("-h") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => fail(USAGE),
    }
}
