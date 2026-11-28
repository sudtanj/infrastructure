//! `rust-ci-runner ci`: the whole manual CI job (FR-006 to FR-024). Checks the target against the
//! watch list, reads four settings from Bitwarden, probes the log store, checks the repo identity,
//! fetches one branch with `git`, refuses docker steps and runs the pipeline. Output is a closed set
//! of lines (`Log` in public mode); every detail goes to the log store only.

use crate::engine::{run_capped, Engine};
use crate::job::{
    dind_image, load_pipeline, project_dir, remove_ws, run_env, run_pipeline, with_shipper,
    MAX_LOG_BYTES,
};
use crate::log::{Log, LOG_STORE_HINT};
use crate::pipeline::valid_name;
use crate::projects::folder_name_ok;
use crate::redact::Redactor;
use crate::ship::{self, Labels, LogConfig, Meta, Shipper, FLUSH_WINDOW};
use crate::store::{self, Held, NamedError};
use crate::util::{base64, random_id};
use crate::watch;
use std::collections::BTreeMap;
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Time limits of the outside calls; each is per attempt where the call is retried.
#[derive(Clone)]
pub struct Limits {
    pub bws: Duration,
    pub probe: Duration,
    pub fetch: Duration,
    pub identity: Duration,
}

/// What `run` talks to. Production values come from `Seams::production`; tests point them at stubs.
pub struct Seams {
    pub bws: String,
    pub engine: String,
    pub git: String,
    /// Where `<owner>/<name>.git` is fetched from.
    pub clone_base: String,
    /// The value of `GIT_ALLOW_PROTOCOL`.
    pub protocol: String,
    /// Where `GET /repos/<repo>` goes.
    pub api_base: String,
    pub limits: Limits,
    /// Delete the checkout directory after the run. Tests that read the checkout afterwards turn it off.
    pub remove_work: bool,
}

impl Seams {
    /// Fixed values: nothing in the environment can point the runner at another `bws` or engine.
    pub fn production() -> Seams {
        Seams {
            bws: "/usr/local/bin/bws".to_string(),
            engine: "docker".to_string(),
            git: "git".to_string(),
            clone_base: "https://github.com".to_string(),
            protocol: "https".to_string(),
            api_base: "https://api.github.com".to_string(),
            remove_work: true,
            limits: Limits {
                bws: Duration::from_secs(60),
                probe: Duration::from_secs(15),
                fetch: Duration::from_secs(300),
                identity: Duration::from_secs(15),
            },
        }
    }
}

struct Args {
    repos: String,
    repo: String,
    git_ref: String,
    project: String,
    work: String,
    allow_docker: bool,
    /// The repository id, the secret allowlist, the allowlisted store rows and whether the ref may
    /// hold secrets: filled by `job` from the watch entry and the store, empty after `parse_args`.
    id: u64,
    allowed: Vec<String>,
    held: Held,
    ref_may_hold_secrets: bool,
}

/// Strict: no unknown flag, no repeated flag, no value that starts with `--`, no stray word.
fn parse_args(args: &[String]) -> Option<Args> {
    let (mut repos, mut repo, mut git_ref, mut project, mut work) = (None, None, None, None, None);
    let mut allow_docker = false;
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        if flag == "--allow-docker" {
            if std::mem::replace(&mut allow_docker, true) {
                return None;
            }
            continue;
        }
        let slot = match flag.as_str() {
            "--repos" => &mut repos,
            "--repo" => &mut repo,
            "--ref" => &mut git_ref,
            "--project" => &mut project,
            "--work" => &mut work,
            _ => return None,
        };
        let v = it.next().filter(|v| !v.starts_with("--"))?;
        if slot.replace(v.clone()).is_some() {
            return None;
        }
    }
    let (repos, repo, git_ref, work): (String, String, String, String) =
        (repos?, repo?, git_ref?, work?);
    let project = project.unwrap_or_else(|| ".".to_string());
    let ref_ok = (1..=200).contains(&git_ref.len())
        && !git_ref.starts_with('-')
        && !git_ref.contains("..")
        && git_ref
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-'));
    // The path must equal its canonical form: the runner builds every mount path under it unchanged.
    let work_ok = work.starts_with('/')
        && !work.contains(':')
        && std::fs::metadata(&work).is_ok_and(|m| m.is_dir())
        && std::fs::canonicalize(&work).is_ok_and(|c| c.to_str() == Some(work.as_str()));
    if !(ref_ok && work_ok && (project == "." || folder_name_ok(&project))) {
        return None;
    }
    Some(Args {
        repos,
        repo,
        git_ref,
        project,
        work,
        allow_docker,
        id: 0,
        allowed: Vec::new(),
        held: Held::default(),
        ref_may_hold_secrets: false,
    })
}

/// A writer both logs share: the first log has no redactor yet, the second has no sink yet.
#[derive(Clone)]
struct Shared(Arc<Mutex<Box<dyn Write + Send>>>);

impl Write for Shared {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).write(b)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).flush()
    }
}

struct Streams {
    out: Shared,
    err: Shared,
}

impl Streams {
    fn log(&self, red: Redactor, shipper: &Option<Arc<Shipper>>) -> Log {
        let log = Log::new(Box::new(self.out.clone()), red, MAX_LOG_BYTES)
            .public_events(true)
            .with_err(Box::new(self.err.clone()));
        with_shipper(log, shipper)
    }
}

/// Runs the job and returns the exit code: 0 only when the pipeline passed and every log line was
/// delivered, 1 for anything else. `token` is the Bitwarden access token; `args` follow `ci`.
pub fn run(
    args: &[String],
    token: &str,
    env: &dyn Fn(&str) -> Option<String>,
    seams: &Seams,
    out: Box<dyn Write + Send>,
    err: Box<dyn Write + Send>,
) -> i32 {
    let streams = Streams {
        out: Shared(Arc::new(Mutex::new(out))),
        err: Shared(Arc::new(Mutex::new(err))),
    };
    match catch_unwind(AssertUnwindSafe(|| job(args, token, env, seams, &streams))) {
        Ok(code) => code,
        Err(_) => {
            streams
                .log(Redactor::new([]), &None)
                .fault("internal error");
            1
        }
    }
}

fn job(
    args: &[String],
    token: &str,
    env: &dyn Fn(&str) -> Option<String>,
    s: &Seams,
    w: &Streams,
) -> i32 {
    let early = w.log(Redactor::new([]), &None);
    let fail = |text: &str| -> i32 {
        early.fault(text);
        1
    };
    let Some(mut a) = parse_args(args) else {
        return fail("bad arguments");
    };
    let listed = watch::read(Path::new(&a.repos)).ok().and_then(|list| {
        list.into_iter().find(|e| {
            e.repo == a.repo
                && e.id.is_some()
                && e.matches_branch(&a.git_ref)
                && (!a.allow_docker || e.allow_docker)
        })
    });
    let Some(entry) = listed else {
        return fail("repository or ref not in the watch list");
    };
    a.id = entry.id.unwrap_or_default(); // `listed` only holds entries with an id
    a.ref_may_hold_secrets = entry.secrets_allowed_on(&a.git_ref);
    a.allowed = entry.secrets;
    if token.is_empty() {
        return fail("BWS_ACCESS_TOKEN is not set");
    }
    let server = env("FACTORY_CI_BWS_SERVER_URL").filter(|v| !v.is_empty());
    // The one call also brings the four runner values, so it cannot be skipped; a ref that may not hold
    // secrets keeps no step-secret row from it.
    let keep: &[String] = if a.ref_may_hold_secrets {
        &a.allowed
    } else {
        &[]
    };
    let fetched = match store::fetch_with(&s.bws, token, server.as_deref(), s.limits.bws, keep) {
        Ok(v) => v,
        Err(e) => return fail(&e.to_string()),
    };
    let sec = fetched.secrets;
    a.held = fetched.held;
    let settings = |k: &str| -> Option<String> {
        match k {
            "FACTORY_CI_LOG_URL" => Some(sec.log_url.clone()),
            "FACTORY_CI_LOG_USER" => Some(sec.log_user.clone()),
            "FACTORY_CI_LOG_TOKEN" => Some(sec.log_token.clone()),
            _ => None,
        }
    };
    let Ok(Some(cfg)) = LogConfig::from_env(&settings) else {
        return fail(LOG_STORE_HINT);
    };
    let run_id = env("GITHUB_RUN_ID")
        .filter(|v| valid_name(v))
        .unwrap_or_else(random_id);
    let probed = ship::probe(&cfg, &run_id, s.limits.probe);
    let (Ok(code) | Err(code)) = probed;
    early.event("runner", &format!("log store answered HTTP {code:03}"));
    if probed.is_err() {
        return fail(if code == 401 || code == 403 {
            LOG_STORE_HINT
        } else {
            "log store check failed"
        });
    }
    let labels = Labels {
        repo: a.repo.clone(),
        project: a.project.clone(),
    };
    let meta = Meta {
        run_id: run_id.clone(),
        sha: String::new(),
        branch: a.git_ref.clone(),
    };
    let shipper = Shipper::start(cfg, labels, meta);
    let red = Redactor::new([
        token.to_string(),
        sec.token.clone(),
        sec.log_url.clone(),
        sec.log_user.clone(),
        sec.log_token.clone(),
    ]);
    let log = w.log(red, &Some(shipper.clone()));
    let passed = steps(&a, &sec.token, &run_id, env, s, &log);
    let delivered = shipper.finish(FLUSH_WINDOW);
    if passed && !delivered {
        log.event("runner", "log lines were dropped");
    }
    i32::from(!(passed && delivered))
}

/// Engine check, identity check, fetch, pipeline. Faults print their fixed line; the detail goes
/// to the log store as step `ci`. True when the pipeline passed.
fn steps(
    a: &Args,
    token: &str,
    run_id: &str,
    env: &dyn Fn(&str) -> Option<String>,
    s: &Seams,
    log: &Log,
) -> bool {
    let detail = |text: &str| log.out("ci", "stderr", text);
    let engine = Engine::new(&s.engine);
    if let Err(e) = engine.capture(&["version".to_string()], &[], Duration::from_secs(60), None) {
        detail(&e);
        log.fault("cannot reach the container engine");
        return false;
    }
    if let Err(e) = identity(a, a.id, token, s) {
        detail(&format!("identity check: {e}"));
        log.fault("repository identity check failed");
        return false;
    }
    let (src, sha) = match fetch_ref(a, token, run_id, s) {
        Ok(v) => v,
        Err(e) => {
            detail(&e);
            log.fault("clone failed");
            return false;
        }
    };
    log.out("ci", "stdout", &format!("checked out {sha}"));
    let (passed, image) = run_checkout(a, &src, &sha, run_id, env, &engine, log);
    if s.remove_work {
        // Steps run as root, so what they wrote needs root to go: remove it through the container.
        if let Some(root) = src.parent() {
            remove_ws(&engine, image.as_deref().unwrap_or(""), root);
        }
    }
    passed
}

/// Pipeline checks and the run, on a checkout. Returns whether it passed and a step image that can
/// remove the directory. Faults print their fixed line; the detail goes to the log store.
fn run_checkout(
    a: &Args,
    src: &Path,
    sha: &str,
    run_id: &str,
    env: &dyn Fn(&str) -> Option<String>,
    engine: &Engine,
    log: &Log,
) -> (bool, Option<String>) {
    let detail = |text: &str| log.out("ci", "stderr", text);
    let loaded = project_dir(src, &a.project).and_then(|dir| {
        load_pipeline(&dir, None)
            .map(|p| (dir, p))
            .map_err(|bad| bad.join("\n"))
    });
    let (dir, p) = match loaded {
        Ok(v) => v,
        Err(e) => {
            detail(&e);
            log.fault("pipeline file refused");
            return (false, None);
        }
    };
    let image = p.steps.values().next().map(|st| st.image.clone());
    let mut bad = p.docker_problems(a.allow_docker);
    bad.extend(p.docker_secret_problems());
    let dind = match dind_image(env, &p) {
        Ok(dind) if bad.is_empty() => dind,
        dind => {
            detail(
                &bad.into_iter()
                    .chain(dind.err())
                    .collect::<Vec<_>>()
                    .join(". "),
            );
            log.fault("docker steps are refused");
            return (false, image);
        }
    };
    let unlisted = p.unlisted(&a.allowed);
    if !unlisted.is_empty() {
        for (step, name) in unlisted {
            detail(&format!(
                "step {step} requests secret {name}, which the watch entry does not allow"
            ));
        }
        log.fault("step secret not allowed");
        return (false, image);
    }
    let names = p.secret_names();
    let secrets = if names.is_empty() {
        BTreeMap::new()
    } else if !a.ref_may_hold_secrets {
        detail("secrets need secret_branches in the watch entry, and the ref must match it");
        log.fault("repository or ref not in the watch list");
        return (false, image);
    } else {
        match a.held.take(&names) {
            Ok(held) => held.0,
            Err(e) => {
                detail(&match e {
                    NamedError::Count { index, found } => {
                        format!("secret {}: expected 1, found {found}", names[index])
                    }
                    NamedError::Unusable { index } => {
                        format!("secret {}: unusable value", names[index])
                    }
                });
                log.fault("step secret unavailable");
                return (false, image);
            }
        }
    };
    log.add_secrets(secrets.values().cloned().collect());
    let mut step_env = run_env(run_id, sha, None, &a.git_ref, "manual");
    // The checkout belongs to the runner's user, not to root in the step: let git use it.
    let safe = [
        ("GIT_CONFIG_COUNT", "1"),
        ("GIT_CONFIG_KEY_0", "safe.directory"),
        ("GIT_CONFIG_VALUE_0", "/workspace"),
    ];
    step_env.extend(safe.map(|(k, v)| (k.to_string(), v.to_string())));
    let out = run_pipeline(engine, run_id, &dir, &p, step_env, secrets, log, &dind);
    engine.cleanup_run(run_id);
    let passed = match out {
        Ok(o) => {
            log.event(
                "runner",
                if o.ok() {
                    "pipeline passed"
                } else {
                    "pipeline failed"
                },
            );
            o.ok()
        }
        Err(e) => {
            detail(&e);
            log.fault("runner fault");
            false
        }
    };
    (passed, image)
}

/// `GET <api>/repos/<repo>`: the numeric id must be the one in the watch entry. Errors are fixed text.
fn identity(a: &Args, want: u64, token: &str, s: &Seams) -> Result<(), String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(s.limits.identity)
        .redirects(0)
        .build();
    let res = agent
        .get(&format!(
            "{}/repos/{}",
            s.api_base.trim_end_matches('/'),
            a.repo
        ))
        .set("Authorization", &format!("Bearer {token}"))
        .set("Accept", "application/vnd.github+json")
        .call();
    let body = match res {
        Ok(r) => r
            .into_string()
            .map_err(|_| "unreadable answer".to_string())?,
        Err(ureq::Error::Status(c, _)) => return Err(format!("HTTP {c}")),
        Err(_) => return Err("no answer".to_string()),
    };
    let got = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v["id"].as_u64());
    if got == Some(want) {
        Ok(())
    } else {
        Err("id differs from the watch entry".to_string())
    }
}

/// A git child: cleared environment plus the allowlist. The clone header goes to the fetch only.
fn git(s: &Seams, header: Option<&str>) -> Command {
    let mut c = Command::new(&s.git);
    c.env_clear()
        .env("HOME", "/tmp")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ALLOW_PROTOCOL", &s.protocol);
    if let Ok(path) = std::env::var("PATH") {
        c.env("PATH", path);
    }
    if let Some(h) = header {
        c.env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "http.https://github.com/.extraHeader")
            .env("GIT_CONFIG_VALUE_0", h);
    }
    c
}

/// Runs git and returns its stdout. The error is git's stderr, for the log store only.
fn git_run(mut c: Command, args: &[&str], limit: Duration) -> Result<String, String> {
    c.args(args);
    match run_capped(c, limit) {
        Some((true, out, _)) => Ok(out),
        Some((false, _, err)) => Err(err),
        None => Err("git did not finish or could not start".to_string()),
    }
}

/// Fetches `refs/heads/<ref>` (a branch, never a tag) into a new directory under `--work`.
/// Returns the checkout and its full SHA.
fn fetch_ref(a: &Args, token: &str, run_id: &str, s: &Seams) -> Result<(PathBuf, String), String> {
    let dir = format!("{}/ci-{run_id}-{}/src", a.work, random_id());
    let url = format!("{}/{}.git", s.clone_base.trim_end_matches('/'), a.repo);
    let refspec = format!("refs/heads/{}", a.git_ref);
    let header = format!(
        "Authorization: basic {}",
        base64(format!("x-access-token:{token}").as_bytes())
    );
    let lim = s.limits.fetch;
    git_run(git(s, None), &["init", "-q", &dir], lim)?;
    git_run(
        git(s, Some(&header)),
        &[
            "-C",
            &dir,
            "fetch",
            "-q",
            "--depth",
            "1",
            "--no-tags",
            "--",
            &url,
            &refspec,
        ],
        lim,
    )?;
    git_run(
        git(s, None),
        &["-C", &dir, "checkout", "-q", "--detach", "FETCH_HEAD"],
        lim,
    )?;
    let sha = git_run(git(s, None), &["-C", &dir, "rev-parse", "HEAD"], lim)?
        .trim()
        .to_string();
    if !crate::github::is_sha(&sha) {
        return Err("git rev-parse did not print a commit id".to_string());
    }
    Ok((PathBuf::from(dir), sha))
}
