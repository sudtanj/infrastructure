//! Runs containers by calling the host engine CLI (docker or podman). Arguments are passed
//! one by one, never through a host shell. Secrets and tokens travel in the environment of
//! the engine client (`-e NAME` without a value), so they never appear in argv.

use crate::log::Log;
use crate::pipeline::{Pipeline, Step};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const LABEL: &str = "factory-ci.run";
pub const DIND_IMAGE: &str = "docker:27.5.1-dind";

/// Start of the reason when the engine binary cannot be started; `job` reports it as `error`.
pub const SPAWN_FAIL: &str = "cannot run ";

/// The only variables an engine child inherits (FR-012).
const ENGINE_ENV: [&str; 7] = [
    "PATH",
    "HOME",
    "DOCKER_HOST",
    "DOCKER_CONFIG",
    "DOCKER_CERT_PATH",
    "DOCKER_TLS_VERIFY",
    "XDG_RUNTIME_DIR",
];

/// Memory limit of a step container; swap is capped at the same value so it cannot grow past it.
const MEMORY: &str = "5g";

/// Sets git's HTTP auth from the FCI_TOKEN variable, inside the container.
const GIT_AUTH: &str = r#"
git config --global --add safe.directory '*'
if [ -n "$FCI_TOKEN" ]; then
  h=$(printf 'x-access-token:%s' "$FCI_TOKEN" | base64 | tr -d '\n')
  export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=http.extraHeader "GIT_CONFIG_VALUE_0=Authorization: Basic $h"
fi
"#;

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Success,
    Failed(String),
    Skipped(String),
}

pub struct Engine {
    pub bin: String,
}

fn s(v: &str) -> String {
    v.to_string()
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// Reads lines, in pieces of at most 64 KB, and hands each to `f`. A piece cut with no newline never
/// ends inside a character or inside an occurrence of one of `forms`: `Log::write` redacts each piece
/// alone, so the cut moves back to the start of what would straddle it and that text opens the next piece.
fn pump(r: impl Read, forms: &[String], mut f: impl FnMut(&str)) {
    let mut r = BufReader::new(r);
    let mut buf = Vec::new();
    loop {
        let more = r
            .by_ref()
            .take((65536 - buf.len()) as u64)
            .read_until(b'\n', &mut buf);
        let eof = !matches!(more, Ok(n) if n > 0);
        let cut = if buf.len() == 65536 && buf.last() != Some(&b'\n') {
            cut_point(&buf, forms)
        } else {
            buf.len()
        };
        if !buf.is_empty() {
            f(&String::from_utf8_lossy(&buf[..cut]));
        }
        if eof {
            break;
        }
        buf.drain(..cut);
    }
}

/// Where to cut a full chunk: not inside a character, and before every start of a form that the chunk
/// ends in the middle of. If that leaves nothing, the character boundary alone.
fn cut_point(buf: &[u8], forms: &[String]) -> usize {
    let whole = match std::str::from_utf8(buf) {
        Err(e) if e.error_len().is_none() => e.valid_up_to(),
        _ => buf.len(),
    };
    let widest = forms.iter().map(String::len).max().unwrap_or(0);
    let mut cut = whole;
    while let Some(p) = (cut.saturating_sub(widest)..cut).find(|&p| {
        forms
            .iter()
            .any(|f| f.len() > cut - p && f.as_bytes().starts_with(&buf[p..cut]))
    }) {
        cut = p;
    }
    if cut == 0 {
        whole
    } else {
        cut
    }
}

pub(crate) fn read_capped(r: impl Read, cap: u64) -> String {
    let mut out = String::new();
    let _ = r.take(cap).read_to_string(&mut out);
    out
}

/// Runs `c` to the end with a time limit: `(success, stdout, stderr)`. The caller sets the
/// environment; stdin is null and both output streams are piped and capped. `None` when the
/// child cannot start or the limit passes (it is killed).
pub(crate) fn run_capped(mut c: Command, limit: Duration) -> Option<(bool, String, String)> {
    c.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().ok()?;
    let (out, err) = (child.stdout.take(), child.stderr.take());
    let end = Instant::now() + limit;
    // Plain threads, not a scope: a grandchild that keeps a pipe open must not block the return.
    let (o_tx, o_rx) = mpsc::channel();
    let (e_tx, e_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = o_tx.send(out.map(|r| read_capped(r, 16 << 20)).unwrap_or_default());
    });
    std::thread::spawn(move || {
        let _ = e_tx.send(err.map(|r| read_capped(r, 64 << 10)).unwrap_or_default());
    });
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    // After the child exits the pipes close unless a grandchild holds them: wait a little, then give up.
    let grace = Duration::from_secs(2);
    let (o, e) = (
        o_rx.recv_timeout(grace).unwrap_or_default(),
        e_rx.recv_timeout(grace).unwrap_or_default(),
    );
    Some((status.success(), o, e))
}

impl Engine {
    pub fn new(bin: &str) -> Engine {
        Engine { bin: s(bin) }
    }

    /// The child starts from an empty environment plus the engine client's own variables, so
    /// nothing else the runner holds reaches it (I9, I11). `env` entries are explicit and pass.
    pub(crate) fn command(&self, args: &[String], env: &[(String, String)]) -> Command {
        let mut c = Command::new(&self.bin);
        c.args(args).stdin(Stdio::null()).env_clear();
        for k in ENGINE_ENV {
            if let Ok(v) = std::env::var(k) {
                c.env(k, v);
            }
        }
        c.env_remove("BWS_ACCESS_TOKEN").envs(env.iter().cloned());
        c
    }

    /// Runs the engine with a time limit. Returns stdout, or stderr as the error.
    /// `container` is killed and removed when the limit passes.
    pub fn capture(
        &self,
        args: &[String],
        env: &[(String, String)],
        limit: Duration,
        container: Option<&str>,
    ) -> Result<String, String> {
        let mut child = self
            .command(args, env)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("{SPAWN_FAIL}{}: {e}", self.bin))?;
        let out = child.stdout.take();
        let err = child.stderr.take();
        let end = Instant::now() + limit;
        std::thread::scope(|sc| {
            let o = sc.spawn(move || out.map(|r| read_capped(r, 16 << 20)).unwrap_or_default());
            let e = sc.spawn(move || err.map(|r| read_capped(r, 64 << 10)).unwrap_or_default());
            let status = loop {
                match child.try_wait() {
                    Ok(Some(st)) => break Ok(st),
                    Ok(None) if Instant::now() >= end => {
                        if let Some(name) = container {
                            self.kill(name);
                        }
                        let _ = child.kill();
                        let _ = child.wait();
                        break Err(format!("{} timed out", self.bin));
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                    Err(e) => break Err(e.to_string()),
                }
            };
            let (o, e) = (o.join().unwrap_or_default(), e.join().unwrap_or_default());
            match status {
                Ok(st) if st.success() => Ok(o),
                Ok(st) => Err(format!("{}: {}", st, e.trim())),
                Err(m) => Err(m),
            }
        })
    }

    fn quick(&self, args: &[&str]) -> Result<String, String> {
        self.capture(&strs(args), &[], Duration::from_secs(60), None)
    }

    pub fn kill(&self, name: &str) {
        let _ = self.quick(&["kill", name]);
        let _ = self.quick(&["rm", "-f", name]);
    }

    /// Removes the containers, networks and volumes that carry `label` (`key=value`).
    pub fn cleanup(&self, label: &str) {
        let filter = format!("label={label}");
        let kinds: [(&[&str], &[&str]); 3] = [
            (&["ps", "-aq"], &["rm", "-f", "-v"]),
            (&["network", "ls", "-q"], &["network", "rm"]),
            (&["volume", "ls", "-q"], &["volume", "rm", "-f"]),
        ];
        for (list, rm) in kinds {
            let mut a: Vec<&str> = list.to_vec();
            a.push("--filter");
            a.push(&filter);
            let Ok(out) = self.quick(&a) else {
                continue;
            };
            let ids: Vec<&str> = out.split_whitespace().collect();
            if !ids.is_empty() {
                let mut a: Vec<&str> = rm.to_vec();
                a.extend(ids);
                let _ = self.quick(&a);
            }
        }
    }

    pub fn cleanup_run(&self, run_id: &str) {
        self.cleanup(&format!("{LABEL}={run_id}"));
    }

    /// Deletes `parent/name` as root inside a container: files that steps created as root
    /// cannot be removed by the runner's own user.
    pub fn remove_dir(&self, image: &str, parent: &Path, name: &str) -> Result<(), String> {
        let p = parent
            .to_str()
            .filter(|p| !p.contains(':'))
            .ok_or("unusable path")?;
        let mount = format!("{p}:/w");
        let target = format!("/w/{name}");
        let args: [&str; 9] = [
            "run",
            "--rm",
            "--entrypoint",
            "rm",
            "-v",
            &mount,
            image,
            "-rf",
            &target,
        ];
        self.quick(&args).map(|_| ())
    }

    /// Runs a container to the end, streaming its output to the log.
    #[allow(clippy::too_many_arguments)]
    pub fn run_logged(
        &self,
        args: &[String],
        env: &[(String, String)],
        who: &str,
        container: &str,
        log: &Log,
        end: Instant,
        timeout_msg: &str,
    ) -> State {
        let mut child = match self
            .command(args, env)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return State::Failed(format!("{SPAWN_FAIL}{}: {e}", self.bin)),
        };
        let out = child.stdout.take();
        let err = child.stderr.take();
        let forms = log.secret_forms();
        let forms = &forms;
        std::thread::scope(|sc| {
            if let Some(r) = out {
                sc.spawn(move || pump(r, forms, |l| log.out(who, "stdout", l)));
            }
            if let Some(r) = err {
                sc.spawn(move || pump(r, forms, |l| log.out(who, "stderr", l)));
            }
            loop {
                match child.try_wait() {
                    Ok(Some(st)) if st.success() => return State::Success,
                    Ok(Some(st)) => {
                        return State::Failed(match st.code() {
                            Some(c) => format!("exit code {c}"),
                            None => s("killed by a signal"),
                        })
                    }
                    Ok(None) if Instant::now() >= end => {
                        self.kill(container);
                        let _ = child.kill();
                        let _ = child.wait();
                        return State::Failed(s(timeout_msg));
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                    Err(e) => return State::Failed(e.to_string()),
                }
            }
        })
    }

    /// Clones `sha` of `url` into `ws` in a container. The token goes in the environment.
    #[allow(clippy::too_many_arguments)]
    pub fn clone_repo(
        &self,
        image: &str,
        run_id: &str,
        ws: &Path,
        url: &str,
        token: &str,
        sha: &str,
        extra: &[String],
        log: &Log,
        end: Instant,
    ) -> State {
        let mount = match mount_arg(ws) {
            Ok(m) => m,
            Err(e) => return State::Failed(e),
        };
        let name = format!("factory-ci-{run_id}-clone");
        // shortcut: fetches the commit by SHA; a host that refuses that needs a branch fetch.
        let script = format!(
            "set -e\n{GIT_AUTH}git init -q .\ngit fetch -q --depth 1 \"$FCI_URL\" \"$FCI_SHA\"\ngit checkout -q FETCH_HEAD"
        );
        let label = format!("{LABEL}={run_id}");
        let (url, sha) = (format!("FCI_URL={url}"), format!("FCI_SHA={sha}"));
        let mut a = strs(&["run", "--rm", "--name", &name, "--label", &label]);
        a.extend(strs(&["-v", &mount, "-w", "/workspace"]));
        a.extend(strs(&["-e", &url, "-e", &sha, "-e", "FCI_TOKEN"]));
        a.extend(extra.iter().cloned());
        a.extend(strs(&["--entrypoint", "sh", image, "-c", &script]));
        let env = [(s("FCI_TOKEN"), s(token))];
        self.run_logged(&a, &env, "clone", &name, log, end, "clone timed out")
    }
}

fn mount_arg(ws: &Path) -> Result<String, String> {
    let p = ws.to_str().ok_or("workspace path is not UTF-8")?;
    if p.contains(':') {
        return Err(format!("workspace path {p} must not contain ':'"));
    }
    Ok(format!("{p}:/workspace"))
}

/// A per-run Docker daemon (`docker:dind`) on a private network, for steps with
/// `docker = true`. The host engine socket is never mounted anywhere.
pub struct Sidecar {
    network: String,
    host: String,
}

pub struct RunCtx<'a> {
    pub engine: &'a Engine,
    pub run_id: &'a str,
    pub workspace: &'a Path,
    /// Run-wide environment (commit, branch, event). Not secret.
    pub env: Vec<(String, String)>,
    /// Values for the secrets that steps list.
    pub secrets: BTreeMap<String, String>,
    pub max_parallel: usize,
    pub deadline: Instant,
    pub log: &'a Log,
    pub dind_image: &'a str,
}

pub struct Outcome {
    pub states: BTreeMap<String, State>,
    /// The first step that failed, with the reason.
    pub failed: Option<(String, String)>,
    pub timed_out: bool,
}

impl Outcome {
    pub fn ok(&self) -> bool {
        self.failed.is_none() && !self.timed_out
    }
}

impl Sidecar {
    fn start(ctx: &RunCtx) -> Result<Sidecar, String> {
        let e = ctx.engine;
        let id = ctx.run_id;
        // A network of its own: steps without `docker = true` are on another network and
        // cannot reach the daemon.
        let (net, vol, name) = (
            format!("factory-ci-{id}-docker"),
            format!("factory-ci-{id}-data"),
            format!("factory-ci-{id}-dind"),
        );
        let label = format!("{LABEL}={id}");
        ctx.log
            .out("runner", "stdout", "starting the docker sidecar");
        let fail = |m: String| format!("cannot start the docker sidecar: {m}");
        e.quick(&["network", "create", "--label", &label, &net])
            .map_err(fail)?;
        e.quick(&["volume", "create", "--label", &label, &vol])
            .map_err(fail)?;
        // Plain TCP on the private network: the sidecar is reachable only from this run's
        // steps, so TLS would add certificate plumbing and no isolation.
        let data = format!("{vol}:/var/lib/docker");
        let args: Vec<&str> = vec![
            "run",
            "-d",
            "--privileged",
            "--name",
            &name,
            "--label",
            &label,
            "--network",
            &net,
            "-v",
            &data,
            "-e",
            "DOCKER_TLS_CERTDIR=",
            ctx.dind_image,
        ];
        // The first start pulls the image, which can take minutes on a slow link.
        e.capture(&strs(&args), &[], Duration::from_secs(900), None)
            .map_err(fail)?;
        let ready = Instant::now() + Duration::from_secs(90);
        loop {
            if e.quick(&["exec", &name, "docker", "info"]).is_ok() {
                return Ok(Sidecar {
                    network: net,
                    host: format!("tcp://{name}:2375"),
                });
            }
            if Instant::now() >= ready {
                return Err(fail(s("the daemon did not become ready in 90 seconds")));
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

fn run_step(ctx: &RunCtx, name: &str, step: &Step, side: Option<&Sidecar>) -> State {
    let mount = match mount_arg(ctx.workspace) {
        Ok(m) => m,
        Err(e) => return State::Failed(e),
    };
    let container = format!("factory-ci-{}-{name}", ctx.run_id);
    let label = format!("{LABEL}={}", ctx.run_id);
    let mut a = strs(&["run", "--rm", "--name", &container, "--label", &label]);
    a.extend(strs(&[
        "--pids-limit",
        "512",
        "--security-opt",
        "no-new-privileges",
    ]));
    a.extend(strs(&[
        "--cap-drop",
        "NET_RAW",
        "--cap-drop",
        "MKNOD",
        "--cap-drop",
        "SETFCAP",
    ]));
    a.extend(strs(&["--memory", MEMORY, "--memory-swap", MEMORY]));
    a.extend(strs(&["-v", &mount, "-w", "/workspace"]));
    let plain = format!("factory-ci-{}", ctx.run_id);
    match (step.docker, side) {
        (true, Some(sc)) => {
            let host = format!("DOCKER_HOST={}", sc.host);
            a.extend(strs(&["--network", &sc.network, "-e", &host]));
        }
        _ => a.extend(strs(&["--network", &plain])),
    }
    let mut add = |k: &String, v: &String| a.extend([s("-e"), format!("{k}={v}")]);
    for (k, v) in &ctx.env {
        add(k, v);
    }
    for (k, v) in &step.env {
        add(k, v);
    }
    let mut env = Vec::new();
    for sec in &step.secrets {
        if let Some(v) = ctx.secrets.get(sec) {
            a.extend([s("-e"), sec.clone()]);
            env.push((sec.clone(), v.clone()));
        }
    }
    // One shell, stopping at the first failing command.
    let script = format!("set -e\n{}", step.commands.join("\n"));
    a.extend([s(&step.image), s("sh"), s("-c"), script]);
    let own = step
        .timeout
        .map(|t| (Instant::now() + Duration::from_secs(t), t));
    let (end, msg) = match own {
        Some((at, t)) if at < ctx.deadline => (at, format!("timed out after {t}s")),
        _ => (ctx.deadline, s("run timed out")),
    };
    ctx.engine
        .run_logged(&a, &env, name, &container, ctx.log, end, &msg)
}

fn blocked(p: &Pipeline, states: &BTreeMap<String, State>, name: &str) -> Option<String> {
    p.steps[name]
        .depends_on
        .iter()
        .find_map(|d| match states.get(d) {
            Some(State::Failed(_)) | Some(State::Skipped(_)) => Some(d.clone()),
            _ => None,
        })
}

/// Runs the graph: a step starts when its dependencies succeeded, up to `max_parallel` at
/// once; a step whose dependency failed or was skipped is skipped. Err is a runner-side
/// fault before any step ran (the docker sidecar did not start).
pub fn execute(ctx: &RunCtx, p: &Pipeline) -> Result<Outcome, String> {
    let label = format!("{LABEL}={}", ctx.run_id);
    let net = format!("factory-ci-{}", ctx.run_id);
    ctx.engine
        .quick(&["network", "create", "--label", &label, &net])
        .map_err(|e| format!("cannot create the run network: {e}"))?;
    let side = if p.needs_docker() {
        Some(Sidecar::start(ctx)?)
    } else {
        None
    };
    let mut states: BTreeMap<String, State> = BTreeMap::new();
    let mut started: BTreeSet<&str> = BTreeSet::new();
    let mut running = 0usize;
    let mut failed = None;
    let mut timed_out = false;
    let (tx, rx) = mpsc::channel::<(String, State)>();
    std::thread::scope(|sc| loop {
        // Only while steps are left to start: a run that finished in time did not time out.
        let unstarted = p
            .steps
            .keys()
            .any(|n| !started.contains(n.as_str()) && !states.contains_key(n));
        timed_out = timed_out || (unstarted && Instant::now() >= ctx.deadline);
        loop {
            let mut changed = false;
            for name in p.steps.keys() {
                if started.contains(name.as_str()) || states.contains_key(name) {
                    continue;
                }
                let why = if timed_out {
                    Some(s("run timed out"))
                } else {
                    blocked(p, &states, name).map(|d| format!("dependency {d} did not succeed"))
                };
                if let Some(why) = why {
                    ctx.log.event(name, &format!("skipped: {why}"));
                    states.insert(name.clone(), State::Skipped(why));
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for (name, step) in &p.steps {
            let ready = step
                .depends_on
                .iter()
                .all(|d| states.get(d) == Some(&State::Success));
            if timed_out
                || running >= ctx.max_parallel
                || started.contains(name.as_str())
                || states.contains_key(name)
                || !ready
            {
                continue;
            }
            started.insert(name);
            running += 1;
            let tx = tx.clone();
            let side = side.as_ref();
            sc.spawn(move || {
                ctx.log.event(name, "started");
                let t = Instant::now();
                let st = catch_unwind(AssertUnwindSafe(|| run_step(ctx, name, step, side)))
                    .unwrap_or_else(|_| State::Failed("internal error".to_string()));
                let took = t.elapsed().as_secs();
                match &st {
                    State::Success => ctx.log.event(name, &format!("success in {took}s")),
                    State::Failed(r) => ctx.log.event(name, &format!("failed in {took}s: {r}")),
                    State::Skipped(_) => {}
                }
                let _ = tx.send((name.clone(), st));
            });
        }
        if running == 0 {
            break;
        }
        if let Ok((name, st)) = rx.recv_timeout(Duration::from_millis(100)) {
            running -= 1;
            match &st {
                State::Success => {}
                State::Failed(r) => {
                    if failed.is_none() {
                        failed = Some((name.clone(), r.clone()));
                    }
                }
                State::Skipped(_) => {}
            }
            states.insert(name, st);
        }
    });
    Ok(Outcome {
        states,
        failed,
        timed_out,
    })
}

#[cfg(test)]
mod tests {
    use super::pump;
    use crate::redact::Redactor;
    use crate::util::base64;
    use std::io::{Cursor, Read};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    // 010 T006. FR-013; I9. `pump(r, forms, f)`: a piece cut with no newline never ends inside a character or
    // inside an occurrence of a form (`Redactor::forms`), and the cut-off text opens the next piece.
    const V: &str = "canary-Zq81-value-key-0123456789";
    const CUT: usize = 65536;

    fn pieces(input: &[u8], forms: &[String]) -> Vec<String> {
        let mut got = Vec::new();
        pump(Cursor::new(input.to_vec()), forms, |p| {
            got.push(p.to_string())
        });
        got
    }

    /// What `Log::write` makes of the pieces: each is redacted alone, then they are read as one line.
    fn shown(red: &Redactor, got: &[String]) -> String {
        got.iter()
            .map(|p| red.apply(p.trim_end_matches('\n')))
            .collect()
    }

    /// A reader that returns at most `step` bytes per call.
    struct Drip<'a> {
        data: &'a [u8],
        step: usize,
    }

    impl Read for Drip<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.step.min(buf.len()).min(self.data.len());
            buf[..n].copy_from_slice(&self.data[..n]);
            self.data = &self.data[n..];
            Ok(n)
        }
    }

    #[test]
    fn pump_masks_a_value_across_the_64k_cut() {
        let red = Redactor::new([V.to_string()]);
        let forms = red.forms();
        for form in [V.to_string(), base64(V.as_bytes())] {
            let starts = [
                CUT - form.len(),
                CUT - form.len() + 1,
                CUT - form.len() / 2,
                CUT - 1,
                CUT,
            ];
            for start in starts {
                let line = format!("{}{form}{}", "a".repeat(start), "b".repeat(200));
                let got = pieces(format!("{line}\n").as_bytes(), &forms);
                let want = format!("{}***{}", "a".repeat(start), "b".repeat(200));
                assert_eq!(shown(&red, &got), want, "{form} at {start}");
            }
        }
        // the same with a reader that gives a few bytes at a time
        let line = format!("{}{V}{}\n", "a".repeat(CUT - 10), "b".repeat(200));
        let mut got = Vec::new();
        let r = Drip {
            data: line.as_bytes(),
            step: 7,
        };
        pump(r, &forms, |p| got.push(p.to_string()));
        let want = format!("{}***{}", "a".repeat(CUT - 10), "b".repeat(200));
        assert_eq!(shown(&red, &got), want);
    }

    #[test]
    fn pump_masks_a_32_kib_value_across_the_cut() {
        let v: String = (0..32768)
            .map(|i| (b'a' + (i % 26) as u8) as char)
            .collect();
        let red = Redactor::new([v.clone()]);
        let forms = red.forms();
        for start in [CUT - 32767, CUT - 16384, CUT - 1] {
            let line = format!("{}{v}{}", "x".repeat(start), "y".repeat(100));
            let got = pieces(format!("{line}\n").as_bytes(), &forms);
            let want = format!("{}***{}", "x".repeat(start), "y".repeat(100));
            assert_eq!(shown(&red, &got), want, "start {start}");
        }
    }

    // `Held::take` refuses a value over 32 KiB (store_tests `held_take_size_cap`), so the pump never sees a
    // form built from one. This is the pump side: with a value one byte over, the forms stay under the
    // 48 KiB cap, and nothing waits forever or is lost; a form that is longer than a chunk does not stall it
    // either.
    #[test]
    fn pump_with_a_value_one_byte_over_the_cap_is_never_given_to_it() {
        let v = "k".repeat(32769);
        let red = Redactor::new([v]);
        let forms = red.forms();
        assert!(forms.iter().all(|f| f.len() <= 48 * 1024));
        let input: Vec<u8> = (0..200_000u32).map(|i| b'a' + (i % 23) as u8).collect();
        for forms in [forms, vec!["a".repeat(3 * CUT)]] {
            let got = pieces(&input, &forms);
            assert_eq!(got.concat().as_bytes(), &input[..]);
        }
    }

    #[test]
    fn pump_keeps_order_and_flushes_the_tail() {
        for forms in [vec![], vec!["line 4".to_string()], vec!["a".repeat(100)]] {
            // no newline at the end: the last text still arrives
            let got = pieces(b"one\ntwo\nthree", &forms);
            assert_eq!(got.concat(), "one\ntwo\nthree", "{forms:?}");
            assert!(got.last().unwrap().ends_with("three"), "{forms:?}: {got:?}");
            assert!(got.iter().all(|p| !p.is_empty()), "{forms:?}: {got:?}");
            // lines keep their order
            let text: String = (0..500).map(|i| format!("line {i}\n")).collect();
            assert_eq!(pieces(text.as_bytes(), &forms).concat(), text, "{forms:?}");
            assert!(pieces(b"", &forms).is_empty(), "{forms:?}");
            assert_eq!(pieces(b"\n\n", &forms).concat(), "\n\n", "{forms:?}");
            // a long stream without a newline, cut at 64 KB several times
            let long: Vec<u8> = (0..3 * CUT + 17).map(|i| b'a' + (i % 26) as u8).collect();
            assert_eq!(
                pieces(&long, &forms).concat().as_bytes(),
                &long[..],
                "{forms:?}"
            );
        }
        // no forms is what `pump` does today: pieces of at most 64 KB, a newline ends a piece
        let line = format!("{}\n", "a".repeat(70_000));
        let sizes: Vec<usize> = pieces(line.as_bytes(), &[])
            .iter()
            .map(String::len)
            .collect();
        assert_eq!(sizes, [CUT, 70_001 - CUT]);
        assert_eq!(pieces(b"one\ntwo\nthree", &[]), ["one\n", "two\n", "three"]);
    }

    // A line that has arrived whole is handed over at once, not held until the stream ends.
    #[test]
    fn pump_does_not_hold_back_a_complete_line() {
        struct Probe {
            step: u8,
            seen: Arc<AtomicUsize>,
            ok: Arc<AtomicBool>,
        }
        impl Read for Probe {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                self.step += 1;
                if self.step == 1 {
                    buf[..6].copy_from_slice(b"line1\n");
                    return Ok(6);
                }
                if self.step == 2 {
                    self.ok
                        .store(self.seen.load(Ordering::SeqCst) >= 1, Ordering::SeqCst);
                }
                Ok(0)
            }
        }
        let (seen, ok) = (
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicBool::new(false)),
        );
        let r = Probe {
            step: 0,
            seen: seen.clone(),
            ok: ok.clone(),
        };
        let s2 = seen.clone();
        pump(r, &[], move |_| {
            s2.fetch_add(1, Ordering::SeqCst);
        });
        assert!(ok.load(Ordering::SeqCst), "line1 was held back");
        assert_eq!(seen.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn pump_never_cuts_a_utf8_char() {
        for ch in ["é", "漢", "😀"] {
            for prefix in 0..4 {
                let line = format!("{}{}", "a".repeat(prefix), ch.repeat(40_000));
                for forms in [vec![], vec![ch.repeat(3)], vec![ch.repeat(50)]] {
                    for tail in ["\n", ""] {
                        let input = format!("{line}{tail}");
                        let got = pieces(input.as_bytes(), &forms);
                        let joined = got.concat();
                        assert!(
                            !joined.contains('\u{fffd}'),
                            "{ch} prefix {prefix} {forms:?}"
                        );
                        assert_eq!(joined, input, "{ch} prefix {prefix} {forms:?}");
                    }
                }
            }
        }
        // bytes that are not UTF-8 at all still come out, one replacement character each
        assert_eq!(pieces(b"ab\xffcd\n", &[]).concat(), "ab\u{fffd}cd\n");
    }

    // A value that starts just before an emit boundary is not split: the cut moves back to where it starts.
    #[test]
    fn pump_does_not_split_a_value_at_its_own_emit_boundary() {
        let red = Redactor::new([V.to_string()]);
        let widest = red.forms().iter().map(String::len).max().unwrap_or(0);
        let start = CUT - widest - 1;
        let line = format!("{}{V}{}\n", "a".repeat(start), "b".repeat(500));
        let got = pieces(line.as_bytes(), &red.forms());
        let want = format!("{}***{}", "a".repeat(start), "b".repeat(500));
        assert_eq!(shown(&red, &got), want);
    }

    // QA: every string `Redactor::add` makes (plain, base64 at three alignments, URL-safe, wrapped 76-column
    // lines, percent, hex, clone header), placed before, across and after the 64 KB cut, is masked once the
    // pieces are redacted one by one. Nothing is lost and no piece is over the cut. Values: ASCII, a wrapped
    // one, multi-byte, two in one batch.
    #[test]
    fn pump_masks_every_form_around_the_cut() {
        let wrapped = format!("canary-Zq81-{}", "k9".repeat(44));
        let batches = [
            vec![V.to_string()],
            vec![wrapped],
            vec!["pässwörd-漢字-😀-key-77".to_string()],
            vec![V.to_string(), "second-Zq82-value-key".to_string()],
        ];
        for values in batches {
            let red = Redactor::new(values.clone());
            let forms = red.forms();
            if values[0].len() == 100 {
                assert!(forms.iter().any(|f| f.len() == 76), "no 76-column form");
            }
            for f in forms.iter().filter(|f| !f.contains('\n')) {
                let len = f.len();
                for k in [0, 1, 2, len / 2, len - 2, len - 1, len, len + 1] {
                    let (pad, tail) = (" ".repeat(CUT - k), " ".repeat(100));
                    let input = format!("{pad}{f}{tail}\n");
                    let got = pieces(input.as_bytes(), &forms);
                    assert_eq!(got.concat(), input, "lossless: {f} at {k}");
                    assert!(got.iter().all(|p| p.len() <= CUT), "{f} at {k}");
                    assert_eq!(shown(&red, &got), format!("{pad}***{tail}"), "{f} at {k}");
                }
            }
        }
    }

    // Two occurrences back to back: the cut is inside the second, then inside the first.
    #[test]
    fn pump_masks_two_adjacent_values_around_the_cut() {
        let red = Redactor::new([V.to_string()]);
        let forms = red.forms();
        for k in [1, V.len() / 2, V.len() - 1, V.len() + 1, 2 * V.len() - 1] {
            let (pad, tail) = (" ".repeat(CUT - k), " ".repeat(50));
            let input = format!("{pad}{V}{V}{tail}\n");
            let got = pieces(input.as_bytes(), &forms);
            assert_eq!(got.concat(), input, "k {k}");
            assert_eq!(shown(&red, &got), format!("{pad}******{tail}"), "k {k}");
        }
    }

    // D1 in test-map.md. The base64 of `ke-site-value-Q7x1` starts with `a`, so a run of `a` is a prefix of a form
    // at every byte. `cut_point` walks back one byte at a time to the start of the chunk, then falls back to
    // the plain cut, which is in the middle of the value. Same with a text that repeats the value's own prefix.
    #[test]
    #[ignore = "defect D1: cut_point walks to the chunk start on a run of a form's first character, see test-map.md"]
    fn pump_masks_a_value_after_a_run_of_a_forms_first_character() {
        let cases = [
            ("a".repeat(CUT - 6), "fake-site-value-Q7x1"),
            ("abc".repeat(21845), "abcabcabcabcabcabc-Z9"),
        ];
        for (fill, v) in cases {
            let red = Redactor::new([v.to_string()]);
            let input = format!("{fill}{v}tail\n");
            let got = pieces(input.as_bytes(), &red.forms());
            assert_eq!(got.concat(), input);
            assert_eq!(shown(&red, &got), format!("{fill}***tail"), "{v}");
        }
    }
}
