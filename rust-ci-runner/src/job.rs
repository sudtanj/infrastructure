use crate::engine::{execute, Engine, Outcome, RunCtx, State, DIND_IMAGE, SPAWN_FAIL};
use crate::github::{compose_description, Api, ApiError, HttpApi};
use crate::log::Log;
use crate::pipeline::{self, Pipeline};
use crate::projects;
use crate::redact::Redactor;
use crate::ship::{self, Labels, LogConfig, Meta, Shipper, FLUSH_WINDOW};
use crate::text;
use crate::util::random_id;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Answers the variables a command reads; `main` passes the process environment.

type Setup = (Opts, HttpApi, Vec<String>, Option<LogConfig>, String);
pub type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

const RETRY: &str = " (retry)";
const LOG_VARS: [&str; 3] = [
    "FACTORY_CI_LOG_URL",
    "FACTORY_CI_LOG_USER",
    "FACTORY_CI_LOG_TOKEN",
];
/// Output of one run, printed and shipped (data-model.md limits).
const MAX_LOG_BYTES: u64 = 10 << 20;

/// What every step sees about the run: commit, previous commit, branch, event.
pub fn run_env(
    id: &str,
    sha: &str,
    prev: Option<&str>,
    branch: &str,
    event: &str,
) -> Vec<(String, String)> {
    let mut v = vec![
        ("CI".to_string(), "true".to_string()),
        ("FACTORY_CI".to_string(), "true".to_string()),
        ("FACTORY_CI_RUN_ID".to_string(), id.to_string()),
        ("FACTORY_CI_SHA".to_string(), sha.to_string()),
        ("FACTORY_CI_BRANCH".to_string(), branch.to_string()),
        ("FACTORY_CI_EVENT".to_string(), event.to_string()),
    ];
    if let Some(p) = prev {
        v.push(("FACTORY_CI_PREV_SHA".to_string(), p.to_string()));
    }
    v
}

fn load_pipeline(dir: &Path, step: Option<&str>) -> Result<Pipeline, Vec<String>> {
    let text = pipeline::read_file(dir).map_err(|e| vec![e])?;
    let p = pipeline::parse(&text)?;
    match step {
        Some(s) => p.only(s).map_err(|e| vec![e]),
        None => Ok(p),
    }
}

/// Reads the pipeline in `dir`, then checks docker steps and looks up the listed secrets, so
/// that nothing starts on a bad file. Every problem is returned.
fn prepare(
    dir: &Path,
    step: Option<&str>,
    lookup: impl Fn(&str) -> Option<String>,
    allow_docker: bool,
) -> Result<(Pipeline, BTreeMap<String, String>), Vec<String>> {
    let p = load_pipeline(dir, step)?;
    let secrets = p.resolve_secrets(lookup);
    let mut bad = p.docker_problems(allow_docker);
    if let Err(e) = &secrets {
        bad.extend(e.iter().cloned());
    }
    match secrets {
        Ok(s) if bad.is_empty() => Ok((p, s)),
        _ => Err(bad),
    }
}

/// Runs the graph in `dir`: the one executor behind both `run` and `job`. The caller cleans up.
#[allow(clippy::too_many_arguments)]
fn run_pipeline(
    engine: &Engine,
    id: &str,
    dir: &Path,
    p: &Pipeline,
    env: Vec<(String, String)>,
    secrets: BTreeMap<String, String>,
    log: &Log,
    dind_image: &str,
) -> Result<Outcome, String> {
    let ctx = RunCtx {
        engine,
        run_id: id,
        workspace: dir,
        env,
        secrets,
        // shortcut: fixed limits; the server's config file does not apply here.
        max_parallel: 4,
        deadline: Instant::now() + Duration::from_secs(30 * 60),
        log,
        dind_image,
    };
    execute(&ctx, p)
}

/// The log store settings, or `None` when shipping is off. `required` (`--quiet-logs`,
/// `--private`) turns "off" into an error, before anything runs.
fn log_config(env: Env, required: bool) -> Result<Option<LogConfig>, String> {
    let cfg = LogConfig::from_env(&|k| var(env, k)).map_err(|v| v.join(". "))?;
    if cfg.is_none() && required {
        return Err(format!("log store required: set {}", LOG_VARS.join(", ")));
    }
    Ok(cfg)
}

/// `rust-ci-runner run`: the same graph and executor as `job`, on one directory.
/// Secrets come from environment variables of the same name. Returns whether it passed.
/// With a log store configured it ships, and prints too unless `quiet`.
pub fn local(dir: &Path, step: Option<&str>, engine: &str, quiet: bool) -> Result<bool, String> {
    let cfg = log_config(&|k| std::env::var(k).ok(), quiet)?;
    let ws =
        std::fs::canonicalize(dir).map_err(|e| format!("cannot use {}: {e}", dir.display()))?;
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    // When FACTORY_CI_SECRETS is set (a comma-separated list), only those names are
    // looked up: a CI system can expose a short list instead of its whole environment.
    let allow: Option<Vec<String>> = std::env::var("FACTORY_CI_SECRETS")
        .ok()
        .map(|v| v.split(',').map(|n| n.trim().to_string()).collect());
    let lookup = |n: &str| {
        let listed = allow.as_ref().is_none_or(|a| a.iter().any(|x| x == n));
        listed.then(|| var(n)).flatten()
    };
    // A local run needs no opt-in for docker steps.
    let (p, secrets) = prepare(&ws, step, lookup, true).map_err(|v| v.join("\n"))?;
    let id = random_id();
    let shipper = cfg.map(|c| {
        let labels = Labels {
            repo: var("GITHUB_REPOSITORY").unwrap_or_else(|| "local".to_string()),
            project: ".".to_string(),
        };
        let meta = Meta {
            run_id: id.clone(),
            sha: var("FACTORY_CI_SHA").unwrap_or_default(),
            branch: var("FACTORY_CI_BRANCH").unwrap_or_default(),
        };
        Shipper::start(c, labels, meta)
    });
    let log = Log::new(
        Box::new(std::io::stdout()),
        Redactor::new(secrets.values().cloned()),
        u64::MAX,
    )
    .quiet(quiet);
    let log = with_shipper(log, &shipper);
    let engine = Engine::new(engine);
    // A CI system that calls this command can pass the same variables the server sets.
    let env = run_env(
        &id,
        &var("FACTORY_CI_SHA").unwrap_or_default(),
        var("FACTORY_CI_PREV_SHA").as_deref(),
        &var("FACTORY_CI_BRANCH").unwrap_or_default(),
        &var("FACTORY_CI_EVENT").unwrap_or_else(|| "local".to_string()),
    )
    .into_iter()
    .filter(|(_, v)| !v.is_empty())
    .collect();
    let out = run_pipeline(&engine, &id, &ws, &p, env, secrets, &log, DIND_IMAGE);
    engine.cleanup_run(&id);
    let res = out.map(|out| {
        log.event(
            "runner",
            if out.ok() {
                "pipeline passed"
            } else {
                "pipeline failed"
            },
        );
        out.ok()
    });
    if let Some(s) = &shipper {
        s.finish(FLUSH_WINDOW);
    }
    res
}

fn with_shipper(log: Log, shipper: &Option<Arc<Shipper>>) -> Log {
    match shipper {
        Some(s) => log.with_sink(s.clone()),
        None => log,
    }
}

fn var(env: Env, k: &str) -> Option<String> {
    env(k).filter(|v| !v.is_empty())
}

/// `--name value` pairs and bare `--switch` flags. Unknown flags and missing values are errors.
fn flags(
    args: &[String],
    valued: &[&str],
    switches: &[&str],
) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let name = a.strip_prefix("--").unwrap_or("");
        if switches.contains(&name) {
            out.insert(name.to_string(), String::new());
        } else if valued.contains(&name) {
            let v = it.next().ok_or_else(|| format!("--{name} needs a value"))?;
            out.insert(name.to_string(), v.clone());
        } else {
            return Err(format!("unknown argument {a}"));
        }
    }
    Ok(out)
}

/// What names one status: the repo, the commit, the project and the run.
struct Target {
    repo: String,
    sha: String,
    project: String,
    run_id: String,
    retry: bool,
}

fn target(f: &HashMap<String, String>) -> Result<Target, String> {
    let get = |k: &str| {
        f.get(k)
            .cloned()
            .ok_or_else(|| format!("--{k} is required"))
    };
    let t = Target {
        repo: get("repo")?,
        sha: get("sha")?,
        project: get("project")?,
        run_id: get("run-id")?,
        retry: f.contains_key("retry"),
    };
    let repo_ok = t.repo.split_once('/').is_some_and(|(o, n)| {
        let part = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        };
        part(o) && part(n)
    });
    if !repo_ok {
        return Err("--repo must be OWNER/NAME".to_string());
    }
    if !crate::github::is_sha(&t.sha) {
        return Err("--sha must be 40 lowercase hex characters".to_string());
    }
    if t.project != "." && !projects::folder_name_ok(&t.project) {
        return Err("--project must be . or a top-level folder name".to_string());
    }
    if !pipeline::valid_name(&t.run_id) {
        return Err("--run-id must be 1 to 64 letters, digits, '-' or '_'".to_string());
    }
    Ok(t)
}

/// At most 140 characters; the retry marker always survives a long reason.
fn describe(reason: &str, delivered: bool, retry: bool) -> String {
    let tail = if retry { RETRY } else { "" };
    let room = 140 - tail.len() - compose_description("", delivered).chars().count();
    let cut: String = reason.chars().take(room).collect();
    format!("{}{tail}", compose_description(&cut, delivered))
}

/// The status link of FR-031: the log template when shipping is configured (none without one),
/// else the Actions run.
fn link(env: Env, run_id: &str) -> Option<String> {
    match LogConfig::from_env(&|k| var(env, k)) {
        Ok(Some(c)) => c.link.map(|t| ship::link(&t, run_id)),
        _ => ship::actions_url(&|k| var(env, k)),
    }
}

fn api(env: Env) -> Result<HttpApi, String> {
    let token = var(env, "FACTORY_CI_TOKEN").ok_or("FACTORY_CI_TOKEN is not set")?;
    let base = var(env, "GITHUB_API_URL").unwrap_or_else(|| "https://api.github.com".to_string());
    Ok(HttpApi::new(&base, &token))
}

/// Posts the final status. 0 when it was delivered. `secrets` are masked in the reason.
fn post(
    env: Env,
    api: &HttpApi,
    t: &Target,
    state: &str,
    reason: &str,
    secrets: Vec<String>,
    delivered: bool,
) -> i32 {
    let reason = text::sanitize(&Redactor::new(secrets).apply(reason), 140);
    let text = describe(&reason, delivered, t.retry);
    let url = link(env, &t.run_id);
    match api.post_status(
        &t.repo,
        &t.sha,
        &projects::context(&t.project),
        state,
        &text,
        url.as_deref(),
    ) {
        Ok(()) => 0,
        Err(e) => {
            // The path holds repo and sha (FR-024): print the code only.
            let why = match e {
                ApiError::Http(c, _) => format!("HTTP {c}"),
                ApiError::RateLimited(_) => "rate limited".to_string(),
                ApiError::Net(_) => "network error".to_string(),
            };
            eprintln!("final status not delivered ({why})");
            1
        }
    }
}

/// `rust-ci-runner status`: one status, nothing run.
pub fn status(args: &[String], env: Env) -> i32 {
    let go = || -> Result<(Target, String, String, HttpApi), String> {
        let f = flags(
            args,
            &["repo", "sha", "project", "run-id", "state", "description"],
            &["retry"],
        )?;
        let t = target(&f)?;
        let state = f
            .get("state")
            .filter(|s| ["error", "failure", "success"].contains(&s.as_str()));
        let state = state
            .ok_or("--state must be error, failure or success")?
            .clone();
        let text = f
            .get("description")
            .cloned()
            .ok_or("--description is required")?;
        Ok((t, state, text, api(env)?))
    };
    match go() {
        Ok((t, state, text, api)) => post(env, &api, &t, &state, &text, token_values(env), true),
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

/// The token and the log credentials: never printed, never posted.
fn token_values(env: Env) -> Vec<String> {
    ["FACTORY_CI_TOKEN"]
        .iter()
        .chain(&LOG_VARS)
        .filter_map(|k| var(env, k))
        .collect()
}

struct Opts {
    t: Target,
    branch: String,
    prev: Option<String>,
    allow_docker: bool,
    quiet: bool,
    private: bool,
    /// Prepare mode: the names file and the reason file.
    prepare: Option<(PathBuf, PathBuf)>,
}

fn opts(args: &[String]) -> Result<Opts, String> {
    let valued = [
        "repo",
        "sha",
        "project",
        "branch",
        "run-id",
        "prev",
        "names-file",
        "reason-file",
    ];
    let f = flags(
        args,
        &valued,
        &["allow-docker", "quiet-logs", "private", "retry"],
    )?;
    let prev = f.get("prev").cloned();
    if prev.as_deref().is_some_and(|p| !crate::github::is_sha(p)) {
        return Err("--prev must be 40 lowercase hex characters".to_string());
    }
    let branch = f
        .get("branch")
        .cloned()
        .filter(|b| !b.is_empty())
        .ok_or("--branch is required")?;
    let prepare = match (f.get("names-file"), f.get("reason-file")) {
        (Some(n), Some(r)) => Some((PathBuf::from(n), PathBuf::from(r))),
        (None, None) => None,
        _ => return Err("--names-file and --reason-file go together".to_string()),
    };
    Ok(Opts {
        t: target(&f)?,
        branch,
        prev,
        allow_docker: f.contains_key("allow-docker"),
        quiet: f.contains_key("quiet-logs") || f.contains_key("private"),
        private: f.contains_key("private"),
        prepare,
    })
}

/// How a run ended: the state, a one-line reason and the secret values it learned (to mask).
struct Done {
    state: &'static str,
    reason: String,
    secrets: Vec<String>,
}

fn done(state: &'static str, reason: impl Into<String>) -> Done {
    Done {
        state,
        reason: reason.into(),
        secrets: vec![],
    }
}

/// FR-057: an image reference is pinned by digest, `...@sha256:<64 hex>`.
fn require_pinned(env: Env, name: &str) -> Result<String, String> {
    let pinned = |i: &str| {
        i.rsplit_once("@sha256:").is_some_and(|(n, h)| {
            !n.is_empty() && h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())
        })
    };
    var(env, name)
        .filter(|i| pinned(i))
        .ok_or_else(|| format!("{name}: image is not pinned"))
}

/// Clones into `ws` and returns the project directory.
fn checkout(
    o: &Opts,
    env: Env,
    engine: &Engine,
    ws: &Path,
    log: &Log,
    image: &str,
) -> Result<PathBuf, Done> {
    let server = var(env, "GITHUB_SERVER_URL").unwrap_or_else(|| "https://github.com".to_string());
    let url = format!("{}/{}.git", server.trim_end_matches('/'), o.t.repo);
    let token = var(env, "FACTORY_CI_TOKEN").unwrap_or_default();
    let end = Instant::now() + Duration::from_secs(10 * 60);
    let st = engine.clone_repo(
        image,
        &o.t.run_id,
        ws,
        &url,
        &token,
        &o.t.sha,
        &[],
        log,
        end,
    );
    if let State::Failed(m) = st {
        return Err(done("error", format!("clone failed: {m}")));
    }
    // The checkout is untrusted: the project must be a real directory inside it, not a link.
    let dir = ws.join(&o.t.project);
    let real = std::fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir());
    let dir = std::fs::canonicalize(&dir)
        .ok()
        .filter(|d| real && std::fs::canonicalize(ws).is_ok_and(|w| d.starts_with(w)));
    dir.ok_or_else(|| {
        done(
            "error",
            "project directory is missing or not a plain directory",
        )
    })
}

/// Clone, check, run. Every way out is a `Done`; nothing here posts or cleans up.
fn attempt(
    o: &Opts,
    env: Env,
    engine: &Engine,
    ws: &Path,
    creds: &[String],
    shipper: &Option<Arc<Shipper>>,
    image: &str,
) -> Done {
    let mk_log = |vals: Vec<String>| {
        let out: Box<dyn std::io::Write + Send> = if o.private {
            Box::new(std::io::sink())
        } else {
            Box::new(std::io::stdout())
        };
        with_shipper(
            Log::new(out, Redactor::new(vals), MAX_LOG_BYTES).quiet(o.quiet),
            shipper,
        )
    };
    let dir = match checkout(o, env, engine, ws, &mk_log(creds.to_vec()), image) {
        Ok(d) => d,
        Err(d) => return d,
    };
    let (p, secrets) = match prepare(&dir, None, |n| var(env, n), o.allow_docker) {
        Ok(v) => v,
        Err(bad) => return done("error", bad.join(". ")),
    };
    let dind = match dind_image(env, &p) {
        Ok(i) => i,
        Err(e) => return done("error", e),
    };
    let vals: Vec<String> = secrets.values().cloned().collect();
    let log = mk_log(creds.iter().cloned().chain(vals.clone()).collect());
    let run = run_env(&o.t.run_id, &o.t.sha, o.prev.as_deref(), &o.branch, "push");
    let out = run_pipeline(engine, &o.t.run_id, &dir, &p, run, secrets, &log, &dind);
    let (state, reason) = match out {
        Err(e) => ("error", format!("runner fault: {e}")),
        // 125 and a failed spawn are the engine's own failure (FR-017); 126 and 127 are the step's.
        Ok(Outcome {
            failed: Some((n, r)),
            ..
        }) if r == "exit code 125" || r.starts_with(SPAWN_FAIL) => {
            ("error", format!("step {n} could not start: {r}"))
        }
        Ok(Outcome {
            failed: Some((n, r)),
            ..
        }) => ("failure", format!("step {n} failed: {r}")),
        Ok(Outcome {
            timed_out: true, ..
        }) => ("failure", "run timed out".to_string()),
        Ok(_) => ("success", "all steps passed".to_string()),
    };
    Done {
        state,
        reason,
        secrets: vals,
    }
}

fn remove_ws(engine: &Engine, image: &str, ws: &Path) {
    if std::fs::remove_dir_all(ws).is_ok() {
        return;
    }
    // Files a step created as root: remove them as root, in a container.
    if let (Some(parent), Some(name)) = (ws.parent(), ws.file_name().and_then(|n| n.to_str())) {
        let _ = engine.remove_dir(image, parent, name);
    }
    let _ = std::fs::remove_dir_all(ws);
}

/// The docker-in-docker image, only when a step needs it.
fn dind_image(env: Env, p: &Pipeline) -> Result<String, String> {
    if p.needs_docker() {
        require_pinned(env, "FACTORY_CI_DIND_IMAGE")
    } else {
        Ok(String::new())
    }
}

fn workspace(o: &Opts, env: Env) -> PathBuf {
    let base = var(env, "RUNNER_TEMP").map_or_else(std::env::temp_dir, PathBuf::from);
    base.join(format!("fci-job-{}-{}", o.t.run_id, random_id()))
}

/// The sorted union of the pipeline's secret names, one per line, or the reason it is refused.
/// Every pre-container check of a real run happens here; no step container starts.
fn secret_names(
    o: &Opts,
    env: Env,
    engine: &Engine,
    ws: &Path,
    image: &str,
) -> Result<String, String> {
    let token = var(env, "FACTORY_CI_TOKEN").unwrap_or_default();
    let log = Log::new(
        Box::new(std::io::sink()),
        Redactor::new([token]),
        MAX_LOG_BYTES,
    )
    .quiet(true);
    let dir = checkout(o, env, engine, ws, &log, image).map_err(|d| d.reason)?;
    let p = load_pipeline(&dir, None).map_err(|v| v.join(". "))?;
    let bad = p.docker_problems(o.allow_docker);
    if !bad.is_empty() {
        return Err(bad.join(". "));
    }
    dind_image(env, &p)?;
    let names: BTreeSet<&str> = p
        .steps
        .values()
        .flat_map(|s| s.secrets.iter().map(String::as_str))
        .collect();
    Ok(names.into_iter().map(|n| format!("{n}\n")).collect())
}

/// Prepare mode: writes the names file, or one reason line to the reason file. Prints nothing,
/// ships nothing, posts nothing. 0 on success, 1 otherwise.
fn prepare_job(
    o: &Opts,
    env: Env,
    image: Result<String, String>,
    names: &Path,
    reason: &Path,
) -> i32 {
    let engine =
        Engine::new(&var(env, "FACTORY_CI_ENGINE").unwrap_or_else(|| "docker".to_string()));
    let ws = workspace(o, env);
    let res = image.and_then(|image| {
        let res = std::fs::create_dir_all(&ws)
            .map_err(|e| format!("cannot create the workspace: {e}"))
            .and_then(|()| {
                // A panic must still leave a reason.
                catch_unwind(AssertUnwindSafe(|| {
                    secret_names(o, env, &engine, &ws, &image)
                }))
                .unwrap_or_else(|_| Err("internal error".to_string()))
            });
        engine.cleanup_run(&o.t.run_id);
        remove_ws(&engine, &image, &ws);
        res
    });
    match res.and_then(|text| {
        std::fs::write(names, text).map_err(|e| format!("cannot write the names file: {e}"))
    }) {
        Ok(()) => 0,
        Err(why) => {
            let why = text::sanitize(&Redactor::new(token_values(env)).apply(&why), 200);
            let _ = std::fs::write(reason, format!("{why}\n"));
            1
        }
    }
}

/// `rust-ci-runner job`: clone, run, post the final status. 0 when the status was delivered,
/// whatever its state; 1 when it was not or the flags are wrong.
pub fn job(args: &[String], env: Env) -> i32 {
    let o = match opts(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let image = require_pinned(env, "FACTORY_CI_CLONE_IMAGE");
    if let Some((names, reason)) = &o.prepare {
        return prepare_job(&o, env, image, names, reason);
    }
    let setup = || -> Result<Setup, String> {
        let mut o = o;
        let image = image?;
        let cfg = log_config(env, o.quiet)?;
        // With a log store the step output goes there only (FR-030).
        o.quiet |= cfg.is_some();
        let api = api(env)?;
        Ok((o, api, token_values(env), cfg, image))
    };
    let (o, api, creds, cfg, image) = match setup() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let shipper = cfg.map(|c| {
        let labels = Labels {
            repo: o.t.repo.clone(),
            project: o.t.project.clone(),
        };
        let meta = Meta {
            run_id: o.t.run_id.clone(),
            sha: o.t.sha.clone(),
            branch: o.branch.clone(),
        };
        Shipper::start(c, labels, meta)
    });
    let engine =
        Engine::new(&var(env, "FACTORY_CI_ENGINE").unwrap_or_else(|| "docker".to_string()));
    let ws = workspace(&o, env);
    let res = match std::fs::create_dir_all(&ws) {
        Err(e) => done("error", format!("cannot create the workspace: {e}")),
        // A panic must not leave the item without a final status.
        Ok(()) => catch_unwind(AssertUnwindSafe(|| {
            attempt(&o, env, &engine, &ws, &creds, &shipper, &image)
        }))
        .unwrap_or_else(|_| done("error", "internal error")),
    };
    engine.cleanup_run(&o.t.run_id);
    remove_ws(&engine, &image, &ws);
    let delivered = shipper.is_none_or(|s| s.finish(FLUSH_WINDOW));
    post(
        env,
        &api,
        &o.t,
        res.state,
        &res.reason,
        creds.into_iter().chain(res.secrets).collect(),
        delivered,
    )
}
