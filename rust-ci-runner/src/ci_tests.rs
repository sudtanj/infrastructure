//! 008 T010 (test-first). Targets `ci::run` with stub `bws`, `docker` and `git` scripts and one `tiny_http`
//! server that plays both Loki and the GitHub API. FR-006 to FR-008, FR-013, FR-014, FR-016 to FR-024, FR-030;
//! I1, I2, I4 to I7, I10, I11. Not wired yet: T009 adds `#[cfg(test)] mod ci_tests;` to lib.rs.
//! Assumed shapes (plan.md "Test-first interface notes"):
//!   `ci::run(args: &[String], token: &str, env: &dyn Fn(&str) -> Option<String>, seams: &Seams,
//!            out: Box<dyn Write + Send>, err: Box<dyn Write + Send>) -> i32`
//!   `Seams { bws, engine, git: String, clone_base, protocol, api_base: String, limits: Limits }`
//!   `Limits { bws, probe, fetch, identity: Duration }`
//! `args` are the flags after `ci`. `run` wraps everything, `env` included, in `catch_unwind`.
//! No test calls `set_var`: the environment is a closure.

use crate::ci::{run, Limits, Seams};
use crate::log::{tests::Buf, FAULTS};
use crate::util::{base64, random_id};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const ACCESS: &str = "0.access-AAAA-bbbb.cccc";
const TOK: &str = "ghp_CLONE_tok_9A";
const LUSER: &str = "usr-77Q1";
const LTOK: &str = "glc_LOGTOK_5B";
const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const RUN: &str = "0123456789abcdef";
const PIN: &str = "r/i@sha256:0000000000000000000000000000000000000000000000000000000000000000";
const KEYS: [&str; 4] = [
    "FACTORY_CI_TOKEN",
    "FACTORY_CI_LOG_URL",
    "FACTORY_CI_LOG_USER",
    "FACTORY_CI_LOG_TOKEN",
];
const WATCH: &str = "[[watch]]\nrepo = \"o/r\"\nid = 42\nbranches = [\"*\"]\n\n\
    [[watch]]\nrepo = \"o/noid\"\nbranches = [\"*\"]\n\n\
    [[watch]]\nrepo = \"o/docker\"\nid = 43\nbranches = [\"*\"]\nallow_docker = true\n\n\
    [[watch]]\nrepo = \"o/mainonly\"\nid = 44\nbranches = [\"main\"]\n";
const NO_ID: u64 = u64::MAX;
const HINT: &str = "log store check failed: check FACTORY_CI_LOG_URL (host only, e.g. https://logs-prod-xxx.grafana.net), FACTORY_CI_LOG_USER (numeric user id) and FACTORY_CI_LOG_TOKEN (needs logs:write)";

// ---- the fake Loki and GitHub API: one server ----

#[derive(Clone)]
struct Seen {
    url: String,
    auth: String,
    body: String,
}

struct Hub {
    url: String,
    server: Arc<tiny_http::Server>,
    seen: Arc<Mutex<Vec<Seen>>>,
    id: Arc<AtomicU64>,
    api_code: Arc<AtomicU16>,
}

impl Drop for Hub {
    fn drop(&mut self) {
        // ends the accept thread: the step container allows 512 processes
        self.server.unblock();
    }
}

/// `push(n)` is the status for the n-th Loki push (0-based); `GET /repos/..` answers `{"id": N}`.
fn hub(push: impl Fn(usize) -> u16 + Send + 'static) -> Hub {
    let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let port = server.server_addr().to_ip().unwrap().port();
    let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
    let id = Arc::new(AtomicU64::new(42));
    let api_code = Arc::new(AtomicU16::new(200));
    let (srv, log, id2, code2) = (server.clone(), seen.clone(), id.clone(), api_code.clone());
    std::thread::spawn(move || {
        let mut pushes = 0usize;
        for mut rq in srv.incoming_requests() {
            let mut body = String::new();
            let _ = rq.as_reader().read_to_string(&mut body);
            let auth = rq
                .headers()
                .iter()
                .find(|h| h.field.to_string().eq_ignore_ascii_case("Authorization"))
                .map(|h| h.value.to_string())
                .unwrap_or_default();
            let url = rq.url().to_string();
            log.lock().unwrap().push(Seen {
                url: url.clone(),
                auth,
                body,
            });
            let (code, text) = if url.ends_with("/loki/api/v1/push") {
                pushes += 1;
                (push(pushes - 1), String::new())
            } else {
                let i = id2.load(Ordering::SeqCst);
                let text = if i == NO_ID {
                    "{}".to_string()
                } else {
                    format!("{{\"id\":{i}}}")
                };
                (code2.load(Ordering::SeqCst), text)
            };
            let _ = rq.respond(tiny_http::Response::from_string(text).with_status_code(code));
        }
    });
    Hub {
        url: format!("http://127.0.0.1:{port}"),
        server,
        seen,
        id,
        api_code,
    }
}

impl Hub {
    fn pushes(&self) -> Vec<Seen> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.url.ends_with("/loki/api/v1/push"))
            .cloned()
            .collect()
    }

    fn api_calls(&self) -> Vec<Seen> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.url.starts_with("/repos/"))
            .cloned()
            .collect()
    }

    /// `(labels, line)` of every pushed value.
    fn values(&self) -> Vec<(serde_json::Value, String)> {
        let mut out = vec![];
        for r in self.pushes() {
            let v: serde_json::Value = serde_json::from_str(&r.body).unwrap();
            for s in v["streams"].as_array().unwrap() {
                for val in s["values"].as_array().unwrap() {
                    out.push((s["stream"].clone(), val[1].as_str().unwrap().to_string()));
                }
            }
        }
        out
    }

    /// The text of every line pushed for `step` on `stream`.
    fn lines(&self, step: &str, stream: &str) -> Vec<String> {
        self.values()
            .into_iter()
            .filter(|(l, _)| l["step"] == step && l["stream"] == stream)
            .map(|(_, t)| t)
            .collect()
    }

    fn bodies(&self) -> String {
        self.pushes()
            .into_iter()
            .map(|s| s.body)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ---- the world: work dir, fixture checkout, stub scripts ----

struct W {
    root: PathBuf,
    work: PathBuf,
    fixture: PathBuf,
    watch: PathBuf,
    hub: Hub,
    secrets: Vec<(String, String)>,
    token: String,
    bws_body: String,
    docker_body: String,
    git_body: String,
    step_out: String,
    hostile: bool,
    real_git: bool,
    base: String,
    protocol: String,
    env: Vec<(String, String)>,
    panic_on_env: bool,
    out: Buf,
    err: Buf,
    remove_work: bool,
}

fn pipe(names: &[&str]) -> String {
    names
        .iter()
        .map(|n| format!("[steps.{n}]\nimage = \"alpine\"\ncommands = [\"echo {n}\"]\n"))
        .collect()
}

impl W {
    fn new(pipeline: &str) -> W {
        W::with_loki(pipeline, |_| 204)
    }

    fn with_loki(pipeline: &str, push: impl Fn(usize) -> u16 + Send + 'static) -> W {
        let root = std::env::temp_dir().join(format!("fci-ci-{}", random_id()));
        std::fs::create_dir_all(root.join("work")).unwrap();
        std::fs::create_dir_all(root.join("fixture/svc")).unwrap();
        let root = std::fs::canonicalize(&root).unwrap();
        let hub = hub(push);
        std::fs::write(root.join("fixture/.factory-ci.toml"), pipeline).unwrap();
        std::fs::write(root.join("fixture/svc/.factory-ci.toml"), pipeline).unwrap();
        std::fs::write(root.join("repos.toml"), WATCH).unwrap();
        let secrets = vec![
            (KEYS[0].to_string(), TOK.to_string()),
            (KEYS[1].to_string(), hub.url.clone()),
            (KEYS[2].to_string(), LUSER.to_string()),
            (KEYS[3].to_string(), LTOK.to_string()),
        ];
        W {
            work: root.join("work"),
            fixture: root.join("fixture"),
            watch: root.join("repos.toml"),
            root,
            hub,
            secrets,
            token: ACCESS.to_string(),
            bws_body: String::new(),
            docker_body: String::new(),
            git_body: String::new(),
            step_out: "step-output".to_string(),
            hostile: false,
            real_git: false,
            base: "file:///nonexistent/base".to_string(),
            protocol: "file".to_string(),
            env: vec![("GITHUB_RUN_ID".to_string(), RUN.to_string())],
            panic_on_env: false,
            out: Buf::default(),
            err: Buf::default(),
            remove_work: false,
        }
    }

    fn p(&self, name: &str) -> String {
        self.root.join(name).display().to_string()
    }

    fn args(&self) -> Vec<String> {
        let (watch, work) = (
            self.watch.display().to_string(),
            self.work.display().to_string(),
        );
        [
            "--repos",
            &watch,
            "--repo",
            "o/r",
            "--ref",
            "main",
            "--project",
            ".",
            "--work",
            &work,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    /// Every bootstrap value and its encodings (I2).
    fn bootstrap(&self) -> Vec<String> {
        let mut v = vec![ACCESS.to_string()];
        v.extend(self.secrets.iter().map(|(_, x)| x.clone()));
        let b: Vec<String> = v.iter().map(|x| base64(x.as_bytes())).collect();
        v.extend(b);
        v.push(base64(format!("x-access-token:{TOK}").as_bytes()));
        v
    }

    fn hostile_sh(&self, to_stdout: bool) -> String {
        if !self.hostile {
            return String::new();
        }
        let mut lines = self.bootstrap();
        lines.extend(
            [
                "::add-mask::x",
                "::error::boom",
                "[runner] pipeline passed",
                "[a] success in 0s",
                "[runner] log lines were dropped",
            ]
            .map(String::from),
        );
        let mut sh = String::new();
        for l in lines {
            if to_stdout {
                sh.push_str(&format!("printf '%s\\n' '{l}'\n"));
            }
            sh.push_str(&format!("printf '%s\\n' '{l}' >&2\n"));
        }
        sh
    }

    fn script(&self, name: &str, text: String) {
        let p = self.root.join(name);
        std::fs::write(&p, text).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn scripts(&self) {
        let d = self.root.display();
        // Parallel steps call the stub at once: the lock keeps a call and its env block in the same position.
        let rec = |tool: &str| {
            format!(
                "d='{d}'\ni=0\nwhile ! mkdir \"$d/{tool}.lock\" 2>/dev/null; do i=$((i+1)); [ $i -gt 2000 ] && exit 1; sleep 0.01; done\nprintf '%s\\n' \"$@\" '=====' >> \"$d/{tool}.calls\"\ne=$(/usr/bin/env); printf '%s\\n=====\\n' \"$e\" >> \"$d/{tool}.envs\"\nrmdir \"$d/{tool}.lock\"\n"
            )
        };
        let list: Vec<_> = self
            .secrets
            .iter()
            .map(|(k, v)| serde_json::json!({"id": "i", "organizationId": "o", "projectId": "p", "key": k, "value": v, "note": "", "creationDate": "x", "revisionDate": "x"}))
            .collect();
        std::fs::write(
            self.root.join("bws.out"),
            serde_json::Value::Array(list).to_string(),
        )
        .unwrap();
        let bws_body = if self.bws_body.is_empty() {
            "cat \"$d/bws.out\"".to_string()
        } else {
            self.bws_body.clone()
        };
        self.script(
            "bws.sh",
            format!(
                "#!/bin/sh\n{}{}{}\n",
                rec("bws"),
                self.hostile_sh(false),
                bws_body
            ),
        );
        let step = &self.step_out;
        self.script(
            "docker.sh",
            format!(
                "#!/bin/sh\n{}case \"$1\" in run|version)\n{};;\nesac\n{}\ncase \"$1\" in run) printf '%s\\n' '{step}';; esac\nexit 0\n",
                rec("docker"),
                self.hostile_sh(true),
                self.docker_body
            ),
        );
        let fix = self.fixture.display();
        let tail = if self.real_git {
            "exec git \"$@\"\n".to_string()
        } else {
            format!(
                "dir=\"\"; prev=\"\"; last=\"\"\nfor a in \"$@\"; do [ \"$prev\" = -C ] && dir=\"$a\"; prev=\"$a\"; last=\"$a\"; done\n\
                 case \" $* \" in\n  *\" init \"*) mkdir -p \"$last\"; exit 0;;\n  *\" rev-parse \"*) echo {SHA}; exit 0;;\n  *\" checkout \"*) cp -a '{fix}'/. \"$dir\"/; exit 0;;\nesac\nexit 0\n"
            )
        };
        self.script(
            "git.sh",
            format!(
                "#!/bin/sh\n{}case \" $* \" in *\" rev-parse \"*)\n{};;\n*)\n{};;\nesac\n{}\n{tail}",
                rec("git"),
                self.hostile_sh(false),
                self.hostile_sh(true),
                self.git_body
            ),
        );
    }

    fn go(&self, args: &[String]) -> i32 {
        self.scripts();
        let (pairs, boom) = (self.env.clone(), self.panic_on_env);
        let env = move |k: &str| {
            assert!(!boom, "boom-panic-secret");
            pairs.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone())
        };
        let seams = Seams {
            bws: self.p("bws.sh"),
            engine: self.p("docker.sh"),
            git: self.p("git.sh"),
            clone_base: self.base.clone(),
            protocol: self.protocol.clone(),
            api_base: self.hub.url.clone(),
            remove_work: self.remove_work,
            limits: Limits {
                bws: Duration::from_secs(5),
                probe: Duration::from_secs(2),
                fetch: Duration::from_secs(20),
                identity: Duration::from_secs(3),
            },
        };
        run(
            args,
            &self.token,
            &env,
            &seams,
            Box::new(self.out.clone()),
            Box::new(self.err.clone()),
        )
    }

    fn ok(&self) -> i32 {
        self.go(&self.args())
    }

    fn blocks(&self, file: &str) -> Vec<Vec<String>> {
        let text = std::fs::read_to_string(self.root.join(file)).unwrap_or_default();
        text.split("=====\n")
            .map(|c| c.lines().map(String::from).collect::<Vec<_>>())
            .filter(|c: &Vec<String>| !c.is_empty())
            .collect()
    }

    fn text(&self, file: &str) -> String {
        std::fs::read_to_string(self.root.join(file)).unwrap_or_default()
    }

    fn docker(&self) -> Vec<Vec<String>> {
        self.blocks("docker.calls")
    }

    fn steps(&self) -> Vec<Vec<String>> {
        self.docker()
            .into_iter()
            .filter(|c| c[0] == "run")
            .collect()
    }

    fn started_containers(&self) -> usize {
        self.docker()
            .iter()
            .filter(|c| ["run", "network", "volume"].contains(&c[0].as_str()))
            .count()
    }

    fn gits(&self) -> Vec<Vec<String>> {
        self.blocks("git.calls")
    }

    fn bws_calls(&self) -> usize {
        self.blocks("bws.calls").len()
    }

    fn stdout(&self) -> String {
        self.out.text()
    }

    fn stderr(&self) -> String {
        self.err.text()
    }

    /// The checkout directory the runner gave to `git init`.
    fn checkout(&self) -> String {
        let init = self
            .gits()
            .into_iter()
            .find(|c| c[0] == "init")
            .expect("git init was called");
        init.last().unwrap().clone()
    }
}

impl Drop for W {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn set(mut a: Vec<String>, flag: &str, val: Option<&str>) -> Vec<String> {
    if let Some(i) = a.iter().position(|x| x == flag) {
        a.remove(i);
        if i < a.len() && !a[i].starts_with("--") {
            a.remove(i);
        }
    }
    if let Some(v) = val {
        a.push(flag.to_string());
        a.push(v.to_string());
    }
    a
}

fn pair(c: &[String], a: &str, b: &str) -> bool {
    c.windows(2).any(|w| w[0] == a && w[1] == b)
}

/// Output lines with the seconds replaced by N.
fn norm(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| match l.rfind(" in ") {
            Some(p)
                if l.ends_with('s')
                    && l[p + 4..l.len() - 1].bytes().all(|b| b.is_ascii_digit())
                    && l.len() > p + 5 =>
            {
                format!("{} in Ns", &l[..p])
            }
            _ => l.to_string(),
        })
        .collect()
}

fn grammar_line(l: &str) -> bool {
    let Some((name, rest)) = l.strip_prefix('[').and_then(|r| r.split_once("] ")) else {
        return false;
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let name_ok = (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    let secs = |w: &str| {
        rest.strip_prefix(w)
            .and_then(|r| r.strip_suffix('s'))
            .is_some_and(digits)
    };
    (name_ok
        && name != "runner"
        && (rest == "started" || rest == "skipped" || secs("success in ") || secs("failed in ")))
        || (name == "runner"
            && ([
                "pipeline passed",
                "pipeline failed",
                "log lines were dropped",
            ]
            .contains(&rest)
                || rest
                    .strip_prefix("log store answered HTTP ")
                    .is_some_and(|c| c.len() == 3 && digits(c))))
}

fn fixed_err(l: &str) -> bool {
    let Some(t) = l.strip_prefix("[runner] ") else {
        return false;
    };
    if FAULTS.contains(&t) {
        return true;
    }
    KEYS.iter().any(|k| {
        t == format!("secret {k}: unusable value")
            || t.strip_prefix(&format!("secret {k}: expected 1, found "))
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    })
}

fn assert_public(w: &W) {
    for l in w.stdout().lines() {
        assert!(grammar_line(l), "stdout line outside the grammar: {l:?}");
    }
    for l in w.stderr().lines() {
        assert!(fixed_err(l), "stderr line outside the fixed list: {l:?}");
    }
}

// ---- arguments (FR-006, FR-007; I7) ----

#[test]
fn ci_requires_all_flags() {
    let w = W::new(&pipe(&["a"]));
    for flag in ["--repos", "--repo", "--ref", "--work"] {
        let code = w.go(&set(w.args(), flag, None));
        assert_eq!(code, 1, "{flag}");
    }
    assert_eq!(w.stderr().lines().count(), 4);
    assert!(
        w.stderr().lines().all(|l| l == "[runner] bad arguments"),
        "{}",
        w.stderr()
    );
    assert_eq!(
        w.bws_calls() + w.docker().len() + w.gits().len(),
        0,
        "nothing ran"
    );
    // a value is missing at the end
    assert_eq!(w.go(&["--repos".to_string()]), 1);
    // --project is optional and defaults to the root
    let w = W::new(&pipe(&["a"]));
    assert_eq!(w.go(&set(w.args(), "--project", None)), 0, "{}", w.stderr());
}

#[test]
fn unknown_flags_and_stray_words_are_refused() {
    let w = W::new(&pipe(&["a"]));
    for extra in [
        vec!["--bogus", "x"],
        vec!["--bogus"],
        vec!["stray"],
        vec!["--allow-docker=true"],
        vec!["-x"],
    ] {
        let mut a = w.args();
        a.extend(extra.iter().map(|s| s.to_string()));
        assert_eq!(w.go(&a), 1, "{extra:?}");
    }
    assert!(
        w.stderr().lines().all(|l| l == "[runner] bad arguments"),
        "{}",
        w.stderr()
    );
    assert_eq!(w.bws_calls(), 0);
}

#[test]
fn repeated_flag_is_refused() {
    let w = W::new(&pipe(&["a"]));
    let n = 6;
    for flag in ["--repos", "--repo", "--ref", "--project", "--work"] {
        let mut a = w.args();
        let i = a.iter().position(|x| x == flag).unwrap();
        let (f, v) = (a[i].clone(), a[i + 1].clone());
        a.push(f);
        a.push(v);
        assert_eq!(w.go(&a), 1, "{flag} twice with the same value");
    }
    let mut a = w.args();
    a.extend(["--allow-docker".to_string(), "--allow-docker".to_string()]);
    assert_eq!(w.go(&a), 1);
    assert_eq!(w.stderr().lines().count(), n);
    assert!(
        w.stderr().lines().all(|l| l == "[runner] bad arguments"),
        "{}",
        w.stderr()
    );
    assert_eq!(w.bws_calls(), 0);
}

#[test]
fn value_starting_with_dashes_is_refused() {
    let w = W::new(&pipe(&["a"]));
    for flag in ["--repos", "--repo", "--ref", "--project", "--work"] {
        let a = set(w.args(), flag, Some("--x"));
        assert_eq!(w.go(&a), 1, "{flag}");
    }
    // a flag followed by a flag: the value is missing
    assert_eq!(
        w.go(&[
            "--repo".to_string(),
            "--ref".to_string(),
            "main".to_string()
        ]),
        1
    );
    assert!(
        w.stderr().lines().all(|l| l == "[runner] bad arguments"),
        "{}",
        w.stderr()
    );
    assert_eq!(w.bws_calls(), 0);
}

#[test]
fn ref_table() {
    let w = W::new(&pipe(&["a"]));
    let long = "a".repeat(201);
    for bad in [
        "",
        "-x",
        "-",
        "a..b",
        "..",
        "a b",
        "a;b",
        "a$b",
        "a*b",
        "a:b",
        "a\\b",
        "é",
        "a\nb",
        "a\0b",
        "a'b",
        "a|b",
        long.as_str(),
    ] {
        assert_eq!(w.go(&set(w.args(), "--ref", Some(bad))), 1, "{bad:?}");
    }
    assert!(
        w.stderr().lines().all(|l| l == "[runner] bad arguments"),
        "{}",
        w.stderr()
    );
    assert_eq!(w.bws_calls(), 0, "a bad ref never reaches bws");
    // I7 over random strings: every one outside the rule is refused before bws
    let alphabet: Vec<char> = "ab./-_;$ é\n*0".chars().collect();
    let mut x: u64 = 29;
    for _ in 0..150 {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let len = 1 + ((x >> 40) % 8) as usize;
        let mut s = String::new();
        for i in 0..len {
            s.push(alphabet[((x >> (i * 5)) % alphabet.len() as u64) as usize]);
        }
        let ok = (1..=200).contains(&s.len())
            && !s.starts_with('-')
            && !s.contains("..")
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-'));
        if !ok {
            assert_eq!(w.go(&set(w.args(), "--ref", Some(&s))), 1, "{s:?}");
            assert_eq!(w.bws_calls(), 0, "{s:?}");
        }
    }
    // good refs pass the argument check (stderr has no `bad arguments`)
    let w = W::new(&pipe(&["a"]));
    for good in ["main", "feature/x-y_z.1", "a".repeat(200).as_str()] {
        assert_eq!(
            w.go(&set(w.args(), "--ref", Some(good))),
            0,
            "{good}: {}",
            w.stderr()
        );
    }
}

#[test]
fn project_table() {
    let w = W::new(&pipe(&["a"]));
    let long = "p".repeat(101);
    for bad in [
        "",
        "..",
        "-x",
        "a/b",
        "/abs",
        "a b",
        "a;b",
        "svc/",
        "./svc",
        "é",
        long.as_str(),
    ] {
        assert_eq!(w.go(&set(w.args(), "--project", Some(bad))), 1, "{bad:?}");
    }
    assert!(
        w.stderr().lines().all(|l| l == "[runner] bad arguments"),
        "{}",
        w.stderr()
    );
    assert_eq!(w.bws_calls(), 0);
    let w = W::new(&pipe(&["a"]));
    assert_eq!(
        w.go(&set(w.args(), "--project", Some("svc"))),
        0,
        "{}",
        w.stderr()
    );
    let steps = w.steps();
    assert!(
        pair(&steps[0], "-v", &format!("{}/svc:/workspace", w.checkout())),
        "{:?}",
        steps[0]
    );
}

#[test]
fn ci_still_refuses_an_image_project_path() {
    // 012 T014; S2: `docker/<name>` is a tick project only. The manual `ci` command keeps its one-folder rule
    // (`ci.rs` stays unchanged), so a `docker/x` project is refused before the Bitwarden call and any container.
    let w = W::new(&pipe(&["a"]));
    for bad in ["docker/x", "docker/hello-static", "docker/x/Dockerfile"] {
        assert_eq!(w.go(&set(w.args(), "--project", Some(bad))), 1, "{bad:?}");
    }
    assert!(
        w.stderr().lines().all(|l| l == "[runner] bad arguments"),
        "{}",
        w.stderr()
    );
    assert_eq!(w.bws_calls(), 0);
    assert_eq!(w.started_containers(), 0);
}

#[test]
fn work_table() {
    let w = W::new(&pipe(&["a"]));
    std::fs::create_dir_all(w.root.join("a:b")).unwrap();
    std::fs::write(w.root.join("file"), "x").unwrap();
    std::os::unix::fs::symlink(&w.work, w.root.join("link")).unwrap();
    let r = w.root.display();
    let bads = [
        String::new(),
        "work".to_string(),
        format!("{r}/nonexistent"),
        format!("{r}/a:b"),
        format!("{r}/file"),
        format!("{r}/link"),
        format!("{r}/work/"),
        format!("{r}/work/../work"),
        format!("{r}/./work"),
        format!("{r}//work"),
    ];
    for bad in &bads {
        assert_eq!(w.go(&set(w.args(), "--work", Some(bad))), 1, "{bad:?}");
    }
    assert!(
        w.stderr().lines().all(|l| l == "[runner] bad arguments"),
        "{}",
        w.stderr()
    );
    assert_eq!(w.bws_calls(), 0);
    assert_eq!(w.ok(), 0, "the canonical path is accepted: {}", w.stderr());
}

// ---- watch gate (FR-006) ----

#[test]
fn repo_outside_the_watch_list_exits_1_before_bws() {
    let w = W::new(&pipe(&["a"]));
    assert_eq!(w.go(&set(w.args(), "--repo", Some("x/y"))), 1);
    assert_eq!(
        w.stderr(),
        "[runner] repository or ref not in the watch list\n"
    );
    assert_eq!(w.stdout(), "");
    assert_eq!(w.bws_calls() + w.docker().len() + w.gits().len(), 0);
    assert!(w.hub.pushes().is_empty() && w.hub.api_calls().is_empty());
    // an unreadable or invalid watch list is the same line
    let bad = w.root.join("bad.toml");
    std::fs::write(&bad, "[[watch]]\nrepo = \"nope\"\n").unwrap();
    assert_eq!(
        w.go(&set(w.args(), "--repos", Some(&bad.display().to_string()))),
        1
    );
    assert_eq!(
        w.go(&set(
            w.args(),
            "--repos",
            Some(&w.root.join("missing.toml").display().to_string())
        )),
        1
    );
    assert!(
        w.stderr()
            .lines()
            .all(|l| l == "[runner] repository or ref not in the watch list"),
        "{}",
        w.stderr()
    );
    assert_eq!(w.bws_calls(), 0);
}

#[test]
fn ref_outside_branches_exits_1() {
    let w = W::new(&pipe(&["a"]));
    assert_eq!(
        w.go(&set(
            set(w.args(), "--repo", Some("o/mainonly")),
            "--ref",
            Some("dev")
        )),
        1
    );
    assert_eq!(
        w.stderr(),
        "[runner] repository or ref not in the watch list\n"
    );
    assert_eq!(w.bws_calls(), 0);
}

#[test]
fn entry_without_id_exits_1() {
    let w = W::new(&pipe(&["a"]));
    assert_eq!(w.go(&set(w.args(), "--repo", Some("o/noid"))), 1);
    assert_eq!(
        w.stderr(),
        "[runner] repository or ref not in the watch list\n"
    );
    assert_eq!(w.bws_calls(), 0);
}

#[test]
fn missing_access_token_exits_1_without_calling_bws() {
    let mut w = W::new(&pipe(&["a"]));
    w.token = String::new();
    assert_eq!(w.ok(), 1);
    assert_eq!(w.stderr(), "[runner] BWS_ACCESS_TOKEN is not set\n");
    assert_eq!(w.stdout(), "");
    assert_eq!(w.bws_calls(), 0);
}

// ---- the run: exit codes and the grammar (FR-008, FR-018; I4) ----

#[test]
fn passing_run_exits_0() {
    let w = W::new(&pipe(&["a"]));
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    assert_eq!(
        norm(&w.stdout()),
        [
            "[runner] log store answered HTTP 204",
            "[a] started",
            "[a] success in Ns",
            "[runner] pipeline passed"
        ]
    );
    assert_eq!(w.stderr(), "");
    // the engine check is the first container call (I5), the step output went to Loki only
    assert_eq!(w.docker()[0], ["version"]);
    assert!(
        w.hub
            .lines("a", "stdout")
            .iter()
            .any(|l| l.ends_with("step-output")),
        "{:?}",
        w.hub.values()
    );
    assert!(w.hub.lines("probe", "stdout").len() == 1);
    assert!(
        w.hub.lines("ci", "stdout").iter().any(|l| l.contains(SHA)),
        "the full SHA goes to Loki"
    );
    assert_public(&w);
}

#[test]
fn failing_step_exits_1_without_reason() {
    let mut p = pipe(&["good", "bad"]);
    p.push_str("[steps.after]\nimage = \"alpine\"\ncommands = [\"x\"]\ndepends_on = [\"bad\"]\n");
    let mut w = W::new(&p);
    w.docker_body = "case \"$*\" in *-bad*) echo 'boom-reason-XYZ' >&2; exit 3;; esac".to_string();
    assert_eq!(w.ok(), 1);
    let mut got = norm(&w.stdout());
    got.sort();
    let mut want: Vec<String> = [
        "[after] skipped",
        "[bad] failed in Ns",
        "[bad] started",
        "[good] started",
        "[good] success in Ns",
        "[runner] log store answered HTTP 204",
        "[runner] pipeline failed",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    want.sort();
    assert_eq!(got, want);
    assert!(!w.stdout().contains("boom-reason") && !w.stdout().contains("exit code"));
    assert_eq!(w.stderr(), "");
    // Loki keeps the reason
    assert!(
        w.hub
            .lines("bad", "stdout")
            .iter()
            .any(|l| l.contains("failed in") && l.contains("exit code 3")),
        "{:?}",
        w.hub.values()
    );
    assert!(w
        .hub
        .lines("bad", "stderr")
        .iter()
        .any(|l| l.contains("boom-reason-XYZ")));
}

#[test]
fn dropped_lines_on_a_pass_print_the_line_and_exit_1() {
    // the probe is accepted, every later push is refused: the shipper stops at once
    let w = W::with_loki(&pipe(&["a"]), |i| if i == 0 { 204 } else { 401 });
    assert_eq!(w.ok(), 1);
    let out = norm(&w.stdout());
    assert_eq!(
        out.last().unwrap(),
        "[runner] log lines were dropped",
        "{out:?}"
    );
    assert!(out.contains(&"[runner] pipeline passed".to_string()));
    assert_eq!(w.stderr(), "");
    // a failed pipeline stays `pipeline failed`; the dropped line is for a pass only
    let mut w = W::with_loki(&pipe(&["a"]), |i| if i == 0 { 204 } else { 401 });
    w.docker_body = "case \"$1\" in run) exit 1;; esac".to_string();
    assert_eq!(w.ok(), 1);
    assert!(w.stdout().contains("[runner] pipeline failed"));
    assert!(
        !w.stdout().contains("log lines were dropped"),
        "{}",
        w.stdout()
    );
}

// ---- the probe (FR-016) ----

#[test]
fn probe_failure_exits_1_with_the_fixed_line() {
    for code in [401u16, 403] {
        let w = W::with_loki(&pipe(&["a"]), move |_| code);
        assert_eq!(w.ok(), 1);
        assert_eq!(
            w.stdout(),
            format!("[runner] log store answered HTTP {code}\n")
        );
        assert_eq!(w.stderr(), format!("[runner] {HINT}\n"));
        assert_eq!(
            w.docker().len() + w.gits().len() + w.hub.api_calls().len(),
            0,
            "{code}: nothing runs"
        );
    }
    // another answer: no hint, still exit 1 (one retry first)
    let w = W::with_loki(&pipe(&["a"]), |_| 500);
    assert_eq!(w.ok(), 1);
    assert_eq!(w.stdout(), "[runner] log store answered HTTP 500\n");
    assert_eq!(w.stderr(), "[runner] log store check failed\n");
    assert_eq!(w.docker().len() + w.gits().len(), 0);
    // nothing answers: 000, zero-padded
    let mut w = W::new(&pipe(&["a"]));
    w.secrets[1].1 = "http://127.0.0.1:1".to_string();
    assert_eq!(w.ok(), 1);
    assert_eq!(w.stdout(), "[runner] log store answered HTTP 000\n");
    assert_eq!(w.stderr(), "[runner] log store check failed\n");
}

// ---- engine and identity (FR-013, FR-023) ----

#[test]
fn engine_down_exits_1_before_the_fetch() {
    let mut w = W::new(&pipe(&["a"]));
    w.docker_body = "case \"$1\" in version) exit 1;; esac".to_string();
    assert_eq!(w.ok(), 1);
    assert_eq!(w.stderr(), "[runner] cannot reach the container engine\n");
    assert_eq!(
        w.gits().len() + w.hub.api_calls().len(),
        0,
        "no identity call, no git"
    );
    assert_eq!(w.started_containers(), 0);
    // every engine call failing is the same line
    let mut w = W::new(&pipe(&["a"]));
    w.docker_body = "exit 1".to_string();
    assert_eq!(w.ok(), 1);
    assert_eq!(w.stderr(), "[runner] cannot reach the container engine\n");
}

#[test]
fn identity_mismatch_exits_1() {
    for (id, code) in [
        (43u64, 200u16),
        (NO_ID, 200),
        (42, 404),
        (42, 500),
        (0, 200),
    ] {
        let w = W::new(&pipe(&["a"]));
        w.hub.id.store(id, Ordering::SeqCst);
        w.hub.api_code.store(code, Ordering::SeqCst);
        assert_eq!(w.ok(), 1, "id {id} code {code}");
        assert_eq!(
            w.stderr(),
            "[runner] repository identity check failed\n",
            "id {id} code {code}"
        );
        assert_eq!(
            w.gits().len(),
            0,
            "the clone token goes nowhere on a mismatch"
        );
        assert_eq!(w.steps().len(), 0);
        assert!(!w.stdout().contains("pipeline"), "{}", w.stdout());
    }
}

#[test]
fn identity_request_uses_the_bearer_token() {
    let w = W::new(&pipe(&["a"]));
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    let calls = w.hub.api_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].url, "/repos/o/r");
    assert_eq!(calls[0].auth, format!("Bearer {TOK}"));
}

// ---- fetch (FR-013, FR-014) ----

fn env_block(w: &W, file: &str, nth: usize) -> Vec<String> {
    let text = w.text(file);
    text.split("=====\n")
        .nth(nth)
        .unwrap_or("")
        .lines()
        .map(String::from)
        .collect()
}

#[test]
fn fetch_runs_init_fetch_checkout_with_the_header_in_env() {
    let w = W::new(&pipe(&["a"]));
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    let g = w.gits();
    let dir = w.checkout();
    assert!(
        dir.starts_with(&format!("{}/ci-{RUN}-", w.work.display())) && dir.ends_with("/src"),
        "{dir}"
    );
    assert_eq!(g[0], ["init", "-q", &dir]);
    let url = format!("{}/o/r.git", w.base);
    assert_eq!(
        g[1],
        [
            "-C",
            &dir,
            "fetch",
            "-q",
            "--depth",
            "1",
            "--no-tags",
            "--",
            &url,
            "refs/heads/main"
        ]
    );
    assert_eq!(
        g[2],
        ["-C", &dir, "checkout", "-q", "--detach", "FETCH_HEAD"]
    );
    assert!(
        g[3].contains(&"rev-parse".to_string()) && g[3].contains(&"HEAD".to_string()),
        "{:?}",
        g[3]
    );
    // the header travels in the environment of the fetch only as the plan says
    let env = env_block(&w, "git.envs", 1);
    let want = format!(
        "GIT_CONFIG_VALUE_0=Authorization: basic {}",
        base64(format!("x-access-token:{TOK}").as_bytes())
    );
    for l in [
        "GIT_CONFIG_COUNT=1",
        "GIT_CONFIG_KEY_0=http.https://github.com/.extraHeader",
        "GIT_ALLOW_PROTOCOL=file",
        "GIT_TERMINAL_PROMPT=0",
        "GIT_CONFIG_GLOBAL=/dev/null",
        "GIT_CONFIG_SYSTEM=/dev/null",
        "HOME=/tmp",
    ] {
        assert!(env.iter().any(|e| e == l), "{l} missing in {env:?}");
    }
    assert!(env.iter().any(|e| *e == want), "{env:?}");
    // I11: a cleared environment plus the allowlist, for every git call
    let allowed = [
        "PATH",
        "HOME",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_SYSTEM",
        "GIT_TERMINAL_PROMPT",
        "GIT_ALLOW_PROTOCOL",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_KEY_0",
        "GIT_CONFIG_VALUE_0",
        "PWD",
        "OLDPWD",
        "SHLVL",
        "_",
    ];
    for i in 0..4 {
        for l in env_block(&w, "git.envs", i) {
            let name = l.split('=').next().unwrap_or("");
            assert!(
                allowed.contains(&name),
                "git call {i}: {name} is not on the allowlist"
            );
        }
    }
    assert!(
        !w.text("git.envs").contains("CARGO_PKG_NAME")
            && !w.text("git.envs").contains("BWS_ACCESS_TOKEN")
    );
}

#[test]
fn fetch_token_is_not_in_argv() {
    let w = W::new(&pipe(&["a"]));
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    let argv = w.text("git.calls");
    for v in [
        TOK.to_string(),
        base64(format!("x-access-token:{TOK}").as_bytes()),
        base64(TOK.as_bytes()),
    ] {
        assert!(!argv.contains(&v), "{v} in git argv");
    }
    assert!(!argv.contains("Authorization") && !argv.contains("extraHeader"));
}

#[test]
fn fetch_asks_for_refs_heads_only() {
    for r in ["main", "v1.0", "release/2026"] {
        let w = W::new(&pipe(&["a"]));
        w.go(&set(w.args(), "--ref", Some(r)));
        let fetch = w
            .gits()
            .into_iter()
            .find(|c| c.contains(&"fetch".to_string()))
            .expect("fetch");
        assert_eq!(fetch.last().unwrap(), &format!("refs/heads/{r}"));
        assert!(
            fetch.iter().all(|a| a != r && !a.starts_with("refs/tags")),
            "{fetch:?}"
        );
        assert!(fetch.contains(&"--no-tags".to_string()) && fetch.contains(&"--depth".to_string()));
    }
}

#[test]
fn clone_failure_prints_the_fixed_line_and_ships_a_redacted_detail() {
    let mut w = W::new(&pipe(&["a"]));
    w.git_body = format!("case \" $* \" in *\" fetch \"*) echo 'fatal: unable to access with {TOK}' >&2; exit 128;; esac");
    assert_eq!(w.ok(), 1);
    assert_eq!(w.stderr(), "[runner] clone failed\n");
    assert!(
        !w.stdout().contains("fatal") && !w.stdout().contains("pipeline"),
        "{}",
        w.stdout()
    );
    assert_eq!(w.steps().len(), 0);
    let detail = w.hub.lines("ci", "stderr").join("\n");
    assert!(
        detail.contains("fatal: unable to access with ***"),
        "{detail}"
    );
    assert!(!w.hub.bodies().contains(TOK));
}

#[test]
fn real_fetch_from_a_file_repo_with_cleared_env() {
    // A wrapper records argv and env, then runs the real git over file:// (protocol seam `file`).
    let mut w = W::new(&pipe(&["a"]));
    w.real_git = true;
    let src = w.root.join("srcrepo");
    let repo = src.join("o").join("r.git");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join(".factory-ci.toml"), pipe(&["a"])).unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["add", ".factory-ci.toml"]);
    git(&["commit", "-q", "-m", "one"]);
    let sha = git(&["rev-parse", "HEAD"]);
    w.base = format!("file://{}", src.display());
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    // the checkout is a real clone of that commit
    let dir = w.checkout();
    assert!(
        std::path::Path::new(&dir)
            .join(".factory-ci.toml")
            .is_file(),
        "{dir}"
    );
    let head = std::process::Command::new("git")
        .args(["-C", &dir, "rev-parse", "HEAD"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&head.stdout).trim(), sha);
    assert!(
        w.hub.lines("ci", "stdout").iter().any(|l| l.contains(&sha)),
        "the Loki line carries the real SHA"
    );
    // every git call ran with a cleared environment
    let envs = w.text("git.envs");
    assert!(
        !envs.contains("CARGO_PKG_NAME") && !envs.contains("BWS_ACCESS_TOKEN"),
        "{envs}"
    );
    assert!(
        envs.lines().any(|l| l == "HOME=/tmp")
            && envs.lines().any(|l| l == "GIT_ALLOW_PROTOCOL=file")
    );
    // the fetch asked for the branch only
    let fetch = w
        .gits()
        .into_iter()
        .find(|c| c.contains(&"fetch".to_string()))
        .unwrap();
    assert_eq!(fetch.last().unwrap(), "refs/heads/main");
    // the protocol seam is a gate: with `https` the same file:// URL is refused
    let mut w2 = W::new(&pipe(&["a"]));
    w2.real_git = true;
    w2.base = format!("file://{}", src.display());
    w2.protocol = "https".to_string();
    assert_eq!(w2.ok(), 1);
    assert_eq!(w2.stderr(), "[runner] clone failed\n");
}

#[test]
fn symlinked_pipeline_file_is_refused() {
    let w = W::new(&pipe(&["a"]));
    std::fs::remove_file(w.fixture.join(".factory-ci.toml")).unwrap();
    std::fs::write(w.fixture.join("real.toml"), pipe(&["a"])).unwrap();
    std::os::unix::fs::symlink(
        w.fixture.join("real.toml"),
        w.fixture.join(".factory-ci.toml"),
    )
    .unwrap();
    assert_eq!(w.ok(), 1);
    assert_eq!(w.stderr(), "[runner] pipeline file refused\n");
    assert_eq!(w.started_containers(), 0);
}

#[test]
fn symlinked_project_is_refused() {
    let w = W::new(&pipe(&["a"]));
    std::fs::remove_dir_all(w.fixture.join("svc")).unwrap();
    std::fs::create_dir_all(w.fixture.join("real")).unwrap();
    std::fs::write(w.fixture.join("real/.factory-ci.toml"), pipe(&["a"])).unwrap();
    std::os::unix::fs::symlink(w.fixture.join("real"), w.fixture.join("svc")).unwrap();
    assert_eq!(w.go(&set(w.args(), "--project", Some("svc"))), 1);
    assert_eq!(w.stderr(), "[runner] pipeline file refused\n");
    assert_eq!(w.started_containers(), 0);
    // a project folder that is missing is refused the same way
    let w = W::new(&pipe(&["a"]));
    assert_eq!(w.go(&set(w.args(), "--project", Some("nothere"))), 1);
    assert_eq!(w.stderr(), "[runner] pipeline file refused\n");
}

// ---- pipeline file, docker and secret refusals (FR-014, FR-019, FR-021, FR-022; I5) ----

const DOCKER_STEP: &str = "[steps.img]\nimage = \"alpine\"\ncommands = [\"x\"]\ndocker = true\n";

#[test]
fn toml_error_prints_the_fixed_line_and_ships_the_detail() {
    let w = W::new("[steps.a\nimage = \"alpine\" LEAK-TOML-77");
    assert_eq!(w.ok(), 1);
    assert_eq!(w.stderr(), "[runner] pipeline file refused\n");
    assert!(
        !w.stdout().contains("TOML")
            && !w.stdout().contains("LEAK")
            && !w.stdout().contains("pipeline"),
        "{}",
        w.stdout()
    );
    assert_eq!(w.started_containers(), 0);
    let detail = w.hub.lines("ci", "stderr").join("\n");
    assert!(
        detail.contains("invalid TOML"),
        "the parse detail goes to Loki: {detail}"
    );
}

#[test]
fn docker_step_is_refused_before_any_container() {
    let w = W::new(DOCKER_STEP);
    assert_eq!(w.ok(), 1);
    assert_eq!(w.stderr(), "[runner] docker steps are refused\n");
    assert_eq!(w.started_containers(), 0, "{:?}", w.docker());
    assert!(
        w.docker().iter().all(|c| c[0] == "version"),
        "only the engine check ran: {:?}",
        w.docker()
    );
    // the flag alone is not enough: the entry must allow it too (o/r does not)
    let w = W::new(DOCKER_STEP);
    let mut a = w.args();
    a.push("--allow-docker".to_string());
    assert_eq!(w.go(&a), 1);
    assert_eq!(w.started_containers(), 0);
}

#[test]
fn allow_docker_needs_the_entry_and_a_pinned_dind_image() {
    let with = |extra: &[&str], env: &[(&str, &str)], repo: &str| {
        let mut w = W::new(DOCKER_STEP);
        w.hub.id.store(43, Ordering::SeqCst);
        w.env
            .extend(env.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        let mut a = set(w.args(), "--repo", Some(repo));
        a.extend(extra.iter().map(|s| s.to_string()));
        let code = w.go(&a);
        (code, w)
    };
    // the flag on an entry without allow_docker: the watch gate, before bws
    let (code, w) = with(
        &["--allow-docker"],
        &[("FACTORY_CI_DIND_IMAGE", PIN)],
        "o/r",
    );
    assert_eq!(
        (code, w.stderr().as_str(), w.bws_calls()),
        (1, "[runner] repository or ref not in the watch list\n", 0)
    );
    // the entry allows it but the flag is missing
    let (code, w) = with(&[], &[("FACTORY_CI_DIND_IMAGE", PIN)], "o/docker");
    assert_eq!(
        (code, w.stderr().as_str(), w.started_containers()),
        (1, "[runner] docker steps are refused\n", 0)
    );
    // flag and entry, no image or an unpinned one
    for env in [
        vec![],
        vec![("FACTORY_CI_DIND_IMAGE", "docker:27.5.1-dind")],
        vec![("FACTORY_CI_DIND_IMAGE", "")],
    ] {
        let (code, w) = with(&["--allow-docker"], &env, "o/docker");
        assert_eq!(
            (code, w.stderr().as_str(), w.started_containers()),
            (1, "[runner] docker steps are refused\n", 0),
            "{env:?}"
        );
    }
    // all three: the run goes ahead and starts the sidecar from the pinned image
    let (code, w) = with(
        &["--allow-docker"],
        &[("FACTORY_CI_DIND_IMAGE", PIN)],
        "o/docker",
    );
    assert_eq!(code, 0, "{}", w.stderr());
    assert!(
        w.docker().iter().any(|c| c.contains(&PIN.to_string())),
        "{:?}",
        w.docker()
    );
}

// 010 T005 (test-first): a declared name that the watch entry does not list is `step secret not allowed`
// (it was `pipeline file refused`, spec R6). The entry of `o/r` in `WATCH` has no `secrets`.
#[test]
fn declared_secret_fails_before_any_container() {
    let mut w =
        W::new("[steps.a]\nimage = \"alpine\"\ncommands = [\"x\"]\nsecrets = [\"NPM_TOKEN\"]\n");
    // even a defined value is never put in a container
    w.env
        .push(("NPM_TOKEN".to_string(), "npm-secret-77".to_string()));
    assert_eq!(w.ok(), 1);
    assert_eq!(w.stderr(), "[runner] step secret not allowed\n");
    assert_eq!(w.started_containers(), 0);
    assert!(
        !w.text("docker.calls").contains("npm-secret-77")
            && !w.text("docker.envs").contains("npm-secret-77")
    );
}

#[test]
fn step_named_runner_gets_the_pipeline_refused() {
    let w = W::new(&pipe(&["runner"]));
    assert_eq!(w.ok(), 1);
    assert_eq!(w.stderr(), "[runner] pipeline file refused\n");
    assert!(!w.stdout().contains("pipeline passed"));
    assert_eq!(w.started_containers(), 0);
}

#[test]
fn engine_fault_during_run_exits_1_with_runner_fault() {
    let mut w = W::new(&pipe(&["a"]));
    w.docker_body = "case \"$1\" in network) exit 1;; esac".to_string();
    assert_eq!(w.ok(), 1);
    assert_eq!(w.stderr(), "[runner] runner fault\n");
    assert!(!w.stdout().contains("pipeline passed"), "{}", w.stdout());
    assert!(
        w.hub
            .lines("ci", "stderr")
            .iter()
            .any(|l| l.contains("cannot create the run network")),
        "{:?}",
        w.hub.values()
    );
    assert_eq!(w.steps().len(), 0);
}

// ---- containment of the step containers (FR-024; I6, I10) ----

fn two_steps() -> W {
    W::new(&pipe(&["one", "two"]))
}

#[test]
fn mount_source_equals_the_work_path() {
    let w = two_steps();
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    let dir = w.checkout();
    let steps = w.steps();
    assert_eq!(steps.len(), 2);
    for s in &steps {
        assert!(pair(s, "-v", &format!("{dir}:/workspace")), "{s:?}");
        let src = format!("{dir}:/workspace");
        assert!(src.starts_with(&format!("{}/", w.work.display())), "{src}");
    }
    // the path is never rewritten: no `-v` source differs from the one the runner gave git
    for s in &steps {
        for (i, a) in s.iter().enumerate() {
            if a == "-v" {
                assert_eq!(s[i + 1].split(':').next().unwrap(), dir);
            }
        }
    }
}

#[test]
fn step_argv_has_no_forbidden_flag() {
    let w = two_steps();
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    for s in w.steps() {
        for (a, b) in [
            ("--security-opt", "no-new-privileges"),
            ("--pids-limit", "512"),
            ("--cap-drop", "NET_RAW"),
            ("--cap-drop", "MKNOD"),
            ("--cap-drop", "SETFCAP"),
            ("--memory", "5g"),
            ("--memory-swap", "5g"),
        ] {
            assert!(pair(&s, a, b), "{a} {b} missing in {s:?}");
        }
        assert_eq!(
            s.iter().filter(|x| *x == "--cap-drop").count(),
            3,
            "exactly the three drops: {s:?}"
        );
        assert_eq!(
            s.iter().filter(|x| *x == "-v").count(),
            1,
            "the workspace is the only mount: {s:?}"
        );
        for a in &s {
            assert!(
                !a.starts_with("--cap-add")
                    && !a.starts_with("--privileged")
                    && !a.starts_with("--cpus"),
                "{a} in {s:?}"
            );
            assert!(a != "--pid" && !a.starts_with("--pid="), "{a} in {s:?}");
            assert!(
                !a.contains("docker.sock") && !a.contains("DOCKER_"),
                "{a} in {s:?}"
            );
            assert!(
                a != "--network=host" && a != "--userns=host",
                "{a} in {s:?}"
            );
        }
        assert!(
            !pair(&s, "--network", "host") && !pair(&s, "--pid", "host"),
            "{s:?}"
        );
    }
}

#[test]
fn no_docker_config_reaches_step_containers() {
    let w = two_steps();
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    for s in w.steps() {
        assert!(!s.iter().any(|a| a.contains("DOCKER_CONFIG")), "{s:?}");
        assert!(
            !s.iter().any(|a| a.contains(".docker")),
            "no docker config mount: {s:?}"
        );
    }
}

#[test]
fn step_env_sets_git_safe_directory() {
    let w = two_steps();
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    for s in w.steps() {
        for kv in [
            "GIT_CONFIG_COUNT=1",
            "GIT_CONFIG_KEY_0=safe.directory",
            "GIT_CONFIG_VALUE_0=/workspace",
            "CI=true",
            "FACTORY_CI_EVENT=manual",
        ] {
            assert!(pair(&s, "-e", kv), "{kv} missing in {s:?}");
        }
        assert!(pair(&s, "-e", &format!("FACTORY_CI_SHA={SHA}")), "{s:?}");
        assert!(pair(&s, "-e", "FACTORY_CI_BRANCH=main"), "{s:?}");
    }
}

// ---- secrets and hostile children (FR-011, FR-019, FR-030; I1, I2, I11) ----

fn hostile(p: &str) -> W {
    let mut w = W::new(p);
    w.hostile = true;
    w
}

fn assert_no_marker(w: &W) {
    let all = format!("{}{}", w.stdout(), w.stderr());
    for bad in ["::add-mask::", "::error::", "boom"] {
        assert!(!all.contains(bad), "{bad} reached the public log: {all}");
    }
    for v in w.bootstrap() {
        assert!(!all.contains(&v), "{v} reached the public log");
    }
}

#[test]
fn hostile_children_cannot_reach_stdout_or_stderr() {
    // every stub writes the token, its base64 and workflow commands to both streams
    let w = hostile(&pipe(&["a"]));
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    assert_public(&w);
    assert_no_marker(&w);
    assert_eq!(norm(&w.stdout()).len(), 4, "{}", w.stdout());
    // a failing step
    let mut w = hostile(&pipe(&["a"]));
    w.docker_body = "case \"$1\" in run) exit 2;; esac".to_string();
    assert_eq!(w.ok(), 1);
    assert_public(&w);
    assert_no_marker(&w);
    // each child failing: only the fixed line comes out
    let cases: [(&str, fn(&mut W), &str); 3] = [
        (
            "bws",
            |w| w.bws_body = "exit 1".to_string(),
            "[runner] secret store unreachable\n",
        ),
        (
            "git",
            |w| w.git_body = "case \" $* \" in *\" fetch \"*) exit 128;; esac".to_string(),
            "[runner] clone failed\n",
        ),
        (
            "docker",
            |w| w.docker_body = "case \"$1\" in version) exit 1;; esac".to_string(),
            "[runner] cannot reach the container engine\n",
        ),
    ];
    for (label, tweak, want) in cases {
        let mut w = hostile(&pipe(&["a"]));
        tweak(&mut w);
        assert_eq!(w.ok(), 1, "{label}");
        assert_eq!(w.stderr(), want, "{label}");
        assert_no_marker(&w);
        assert_public(&w);
    }
}

#[test]
fn hostile_fixture_prints_only_allowed_lines() {
    // step names that look like grammar, a 64-character name, a failing step with a dependent
    let long = "n".repeat(64);
    let mut p = pipe(&["started", "success", &long]);
    p.push_str("[steps.end]\nimage = \"alpine\"\ncommands = [\"x\"]\ndepends_on = [\"started\"]\n");
    let mut w = hostile(&p);
    w.docker_body =
        "case \"$*\" in *-started*) echo '[runner] pipeline passed' >&2; exit 9;; esac".to_string();
    assert_eq!(w.ok(), 1);
    assert_public(&w);
    assert_no_marker(&w);
    let out = norm(&w.stdout());
    assert_eq!(
        out.iter()
            .filter(|l| *l == "[runner] pipeline failed")
            .count(),
        1,
        "{out:?}"
    );
    assert_eq!(
        out.iter()
            .filter(|l| *l == "[runner] pipeline passed")
            .count(),
        0,
        "{out:?}"
    );
    assert!(
        out.contains(&"[end] skipped".to_string())
            && out.contains(&"[started] failed in Ns".to_string()),
        "{out:?}"
    );
    assert!(out.contains(&format!("[{long}] started")), "{out:?}");
    assert_eq!(w.stderr(), "");
}

#[test]
fn no_bootstrap_value_in_stdout_stderr_argv_or_engine_child_env() {
    // I2: raw values and their base64 forms, including base64("x-access-token:<token>")
    let w = hostile(&pipe(&["a", "b"]));
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    let hay = [
        ("stdout", w.stdout()),
        ("stderr", w.stderr()),
        ("docker argv", w.text("docker.calls")),
        ("docker env", w.text("docker.envs")),
        ("bws argv", w.text("bws.calls")),
        ("git argv", w.text("git.calls")),
        ("loki bodies", w.hub.bodies()),
    ];
    for v in w.bootstrap() {
        for (label, text) in &hay {
            assert!(!text.contains(&v), "{v} found in {label}");
        }
    }
    // names too: the engine and the steps never see a bootstrap variable
    for text in [w.text("docker.envs"), w.text("docker.calls")] {
        for n in ["BWS_", "FACTORY_CI_TOKEN", "FACTORY_CI_LOG_"] {
            assert!(!text.contains(n), "{n} in an engine child: {text}");
        }
    }
    // the access token is in the environment of the bws child and nowhere in its argv
    assert!(w
        .text("bws.envs")
        .lines()
        .any(|l| l == format!("BWS_ACCESS_TOKEN={ACCESS}")));
    // the redactor masked what the hostile steps printed
    let step = w.hub.lines("a", "stdout").join("\n");
    assert!(step.contains("***"), "{step}");
}

#[test]
fn panic_prints_internal_error_only() {
    let mut w = W::new(&pipe(&["a"]));
    w.panic_on_env = true;
    assert_eq!(w.ok(), 1);
    assert_eq!(w.stderr(), "[runner] internal error\n");
    assert!(
        !w.stdout().contains("boom") && !w.stderr().contains("boom"),
        "{}{}",
        w.stdout(),
        w.stderr()
    );
    assert_public(&w);
}

// 010 T005 (test-first): two rows added, `step secret not allowed` and `step secret unavailable`.
#[test]
fn every_stderr_line_is_in_the_fixed_list() {
    type Case = (
        &'static str,
        &'static str,
        fn() -> W,
        fn(Vec<String>) -> Vec<String>,
    );
    let keep: fn(Vec<String>) -> Vec<String> = |a| a;
    let one = || W::new(&pipe(&["a"]));
    let cases: Vec<Case> = vec![
        ("args", "bad arguments", one, |a| {
            set(a, "--bogus", Some("x"))
        }),
        (
            "not listed",
            "repository or ref not in the watch list",
            one,
            |a| set(a, "--repo", Some("x/y")),
        ),
        (
            "no token",
            "BWS_ACCESS_TOKEN is not set",
            || {
                let mut w = W::new(&pipe(&["a"]));
                w.token = String::new();
                w
            },
            keep,
        ),
        (
            "bws down",
            "secret store unreachable",
            || {
                let mut w = W::new(&pipe(&["a"]));
                w.bws_body = "echo 'connection refused' >&2; exit 1".to_string();
                w
            },
            keep,
        ),
        (
            "bws rejects",
            "secret store rejected the access token",
            || {
                let mut w = W::new(&pipe(&["a"]));
                w.bws_body = "echo '401 Unauthorized' >&2; exit 1".to_string();
                w
            },
            keep,
        ),
        (
            "key count",
            "secret FACTORY_CI_LOG_USER: expected 1, found 0",
            || {
                let mut w = W::new(&pipe(&["a"]));
                w.secrets.remove(2);
                w
            },
            keep,
        ),
        (
            "unusable",
            "secret FACTORY_CI_TOKEN: unusable value",
            || {
                let mut w = W::new(&pipe(&["a"]));
                w.secrets[0].1 = "abc".to_string();
                w
            },
            keep,
        ),
        (
            "probe 401",
            HINT,
            || W::with_loki(&pipe(&["a"]), |_| 401),
            keep,
        ),
        (
            "probe none",
            "log store check failed",
            || {
                let mut w = W::new(&pipe(&["a"]));
                w.secrets[1].1 = "http://127.0.0.1:1".to_string();
                w
            },
            keep,
        ),
        (
            "engine",
            "cannot reach the container engine",
            || {
                let mut w = W::new(&pipe(&["a"]));
                w.docker_body = "case \"$1\" in version) exit 1;; esac".to_string();
                w
            },
            keep,
        ),
        (
            "identity",
            "repository identity check failed",
            || {
                let w = W::new(&pipe(&["a"]));
                w.hub.id.store(43, Ordering::SeqCst);
                w
            },
            keep,
        ),
        (
            "clone",
            "clone failed",
            || {
                let mut w = W::new(&pipe(&["a"]));
                w.git_body = "case \" $* \" in *\" fetch \"*) exit 128;; esac".to_string();
                w
            },
            keep,
        ),
        (
            "toml",
            "pipeline file refused",
            || W::new("[steps.a\nimage"),
            keep,
        ),
        (
            "docker",
            "docker steps are refused",
            || W::new(DOCKER_STEP),
            keep,
        ),
        (
            "step secret not allowed",
            "step secret not allowed",
            || {
                W::new("[steps.a]\nimage = \"alpine\"\ncommands = [\"x\"]\nsecrets = [\"NPM_TOKEN\"]\n")
            },
            keep,
        ),
        (
            "step secret unavailable",
            "step secret unavailable",
            || {
                // allowed on `main`, but the store has no such key
                let w = W::new(
                    "[steps.a]\nimage = \"alpine\"\ncommands = [\"x\"]\nsecrets = [\"NPM_TOKEN\"]\n",
                );
                let entry = "[[watch]]\nrepo = \"o/r\"\nid = 42\nbranches = [\"main\"]\nsecret_branches = [\"main\"]\nsecrets = [\"NPM_TOKEN\"]\n";
                std::fs::write(&w.watch, entry).unwrap();
                w
            },
            keep,
        ),
        (
            "runner fault",
            "runner fault",
            || {
                let mut w = W::new(&pipe(&["a"]));
                w.docker_body = "case \"$1\" in network) exit 1;; esac".to_string();
                w
            },
            keep,
        ),
        (
            "panic",
            "internal error",
            || {
                let mut w = W::new(&pipe(&["a"]));
                w.panic_on_env = true;
                w
            },
            keep,
        ),
    ];
    for (label, want, world, args) in cases {
        let w = world();
        let code = w.go(&args(w.args()));
        // I4: the exit code is 0 or 1, and 1 here
        assert_eq!(code, 1, "{label}");
        assert_eq!(w.stderr(), format!("[runner] {want}\n"), "{label}");
        assert_public(&w);
    }
}

// ---- environment the command reads (plan.md "CLI") ----

fn work_entries(w: &W) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(&w.work)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    v.sort();
    v
}

#[test]
fn invalid_run_id_is_replaced_and_runs_get_their_own_directory() {
    let mut w = W::new(&pipe(&["a"]));
    w.env = vec![("GITHUB_RUN_ID".to_string(), "a/../b".to_string())];
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    let e = work_entries(&w);
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(
        e[0].starts_with("ci-")
            && e[0]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "{e:?}"
    );
    // a second run with the same id gets a new directory
    let w = W::new(&pipe(&["a"]));
    assert_eq!(w.ok(), 0);
    assert_eq!(w.ok(), 0);
    let e = work_entries(&w);
    assert_eq!(e.len(), 2, "{e:?}");
    assert!(
        e.iter().all(|n| n.starts_with(&format!("ci-{RUN}-"))),
        "{e:?}"
    );
}

#[test]
fn the_bitwarden_server_url_comes_from_the_env_and_is_checked() {
    let mut w = W::new(&pipe(&["a"]));
    w.env.push((
        "FACTORY_CI_BWS_SERVER_URL".to_string(),
        "https://vault.bitwarden.eu".to_string(),
    ));
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    assert!(w
        .text("bws.envs")
        .lines()
        .any(|l| l == "BWS_SERVER_URL=https://vault.bitwarden.eu"));
    // empty means unset
    let mut w = W::new(&pipe(&["a"]));
    w.env
        .push(("FACTORY_CI_BWS_SERVER_URL".to_string(), String::new()));
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    assert!(!w.text("bws.envs").contains("BWS_SERVER_URL"));
    // a host outside the two allowed ones never reaches bws
    for bad in [
        "http://vault.bitwarden.com",
        "https://evil.example",
        "https://vault.bitwarden.com.evil.example",
    ] {
        let mut w = W::new(&pipe(&["a"]));
        w.env
            .push(("FACTORY_CI_BWS_SERVER_URL".to_string(), bad.to_string()));
        assert_eq!(w.ok(), 1, "{bad}");
        assert_eq!(w.stderr(), "[runner] secret store unreachable\n", "{bad}");
        assert_eq!(w.bws_calls(), 0, "{bad}");
    }
}

#[test]
fn github_server_url_is_ignored() {
    let mut w = W::new(&pipe(&["a"]));
    w.env.push((
        "GITHUB_SERVER_URL".to_string(),
        "https://evil.example".to_string(),
    ));
    w.env.push((
        "GITHUB_API_URL".to_string(),
        "https://evil.example/api".to_string(),
    ));
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    let fetch = w
        .gits()
        .into_iter()
        .find(|c| c.contains(&"fetch".to_string()))
        .unwrap();
    assert!(fetch.contains(&format!("{}/o/r.git", w.base)), "{fetch:?}");
    assert!(!w.text("git.calls").contains("evil"));
    assert_eq!(
        w.hub.api_calls().len(),
        1,
        "the identity call went to the seam, not the env"
    );
}

#[test]
fn a_step_writing_over_10_mb_cannot_hide_the_result_lines() {
    let mut w = W::new(&pipe(&["a"]));
    w.docker_body =
        "case \"$1\" in run) head -c 11000000 /dev/zero | tr '\\0' x; echo;; esac".to_string();
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    assert_eq!(
        norm(&w.stdout()),
        [
            "[runner] log store answered HTTP 204",
            "[a] started",
            "[a] success in Ns",
            "[runner] pipeline passed"
        ]
    );
    assert_public(&w);
}

#[test]
fn the_checkout_directory_is_removed_after_the_run() {
    let mut w = W::new(&pipe(&["a"]));
    w.remove_work = true;
    assert_eq!(w.ok(), 0, "{}", w.stderr());
    assert!(
        !std::path::Path::new(&w.checkout()).exists(),
        "the src directory is still there"
    );
    assert_eq!(
        work_entries(&w),
        Vec::<String>::new(),
        "the ci-<run> directory is still there"
    );
    // a failed run is cleaned up too
    let mut w = W::new(&pipe(&["a"]));
    w.remove_work = true;
    w.docker_body = "case \"$1\" in run) exit 1;; esac".to_string();
    assert_eq!(w.ok(), 1);
    assert_eq!(work_entries(&w), Vec::<String>::new());
}

// R06: production values are fixed, so no environment variable can swap `bws`, the engine, the clone
// host, the protocol or the API host.
#[test]
fn production_seams_are_fixed_and_ignore_the_environment() {
    let s = Seams::production();
    assert_eq!(s.bws, "/usr/local/bin/bws");
    assert_eq!(s.engine, "docker");
    assert_eq!(s.git, "git");
    assert_eq!(s.clone_base, "https://github.com");
    assert_eq!(s.protocol, "https");
    assert_eq!(s.api_base, "https://api.github.com");
    assert!(s.remove_work);
    assert_eq!(s.limits.bws, Duration::from_secs(60));
    assert_eq!(s.limits.probe, Duration::from_secs(15));
    assert_eq!(s.limits.fetch, Duration::from_secs(300));
    assert_eq!(s.limits.identity, Duration::from_secs(15));
    let src = include_str!("ci.rs");
    assert!(
        !src.contains("FACTORY_CI_BWS\"") && !src.contains("FACTORY_CI_ENGINE"),
        "ci.rs reads a variable that R06 removed"
    );
}

// ---- 010 T005: step secrets through `ci::run` ----
//
// FR-001 to FR-013, US1 to US4, SC-002, SC-003; I1 to I3, I5, I6, I8. Planned behaviour (plan.md "Order inside
// `run_checkout`"): after the one `bws` call and the clone, load the pipeline (`pipeline file refused`), docker
// rules (`docker steps are refused`), allowlist (`step secret not allowed`), ref gate (`repository or ref not
// in the watch list`), lookup (`step secret unavailable`), one batch to the redactor, then the run. The
// `WATCH` entry of `o/r` has `branches = ["*"]`, which the gate refuses for secrets, so these tests overwrite
// the watch file with an entry that has `branches = ["main"]`; the fake API answers id 42. The stub `docker`
// runs its `docker_body` with the step's `-e NAME` values in its environment, so a hostile step is a shell
// snippet there. Fake values only.
mod step_secrets {
    use super::*;

    /// The text of each line pushed for `step` on `stream`, without the `run= sha= branch=` label the shipper adds.
    fn lines(w: &W, step: &str, stream: &str) -> Vec<String> {
        let bare = |l: String| l.splitn(4, ' ').nth(3).unwrap_or_default().to_string();
        w.hub.lines(step, stream).into_iter().map(bare).collect()
    }

    const KEY: &str = "SITE_API_KEY";
    const VAL: &str = "fake-site-value-Q7x1";
    const MAIN: &str = "branches = [\"main\"]\nsecret_branches = [\"main\"]\n";
    const RUNNER_NAMES: [&str; 6] = [
        "FACTORY_CI_TOKEN",
        "FACTORY_CI_LOG_URL",
        "FACTORY_CI_LOG_USER",
        "FACTORY_CI_LOG_TOKEN",
        "BW_X",
        "BITWARDEN",
    ];

    fn toml_list(names: &[&str]) -> String {
        names
            .iter()
            .map(|n| format!("{n:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The watch entry of `o/r`: `extra` is more entry lines, each ending in a newline.
    fn entry(extra: &str, secrets: &[&str]) -> String {
        format!(
            "[[watch]]\nrepo = \"o/r\"\nid = 42\n{extra}secrets = [{}]\n",
            toml_list(secrets)
        )
    }

    fn one_step(name: &str, secrets: &[&str]) -> String {
        let s = if secrets.is_empty() {
            String::new()
        } else {
            format!("secrets = [{}]\n", toml_list(secrets))
        };
        format!("[steps.{name}]\nimage = \"alpine\"\ncommands = [\"echo {name}\"]\n{s}")
    }

    fn req(names: &[&str]) -> String {
        one_step("a", names)
    }

    /// A step that lists a secret next to a separate step that needs docker.
    fn docker_and_secrets() -> String {
        format!(
            "{}[steps.d]\nimage = \"alpine\"\ncommands = [\"x\"]\ndocker = true\n",
            one_step("s", &[KEY])
        )
    }

    /// A world with `watch` as the watch file and `store` as extra rows in the secret store.
    fn world(pipeline: &str, watch: &str, store: &[(&str, &str)]) -> W {
        let mut w = W::new(pipeline);
        std::fs::write(&w.watch, watch).unwrap();
        w.secrets
            .extend(store.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        w
    }

    /// The `docker run` call of a step, as an index into `w.docker()` and `docker.envs`.
    fn run_idx(w: &W, step: &str) -> usize {
        let name = format!("factory-ci-{RUN}-{step}");
        w.docker()
            .iter()
            .position(|c| c[0] == "run" && pair(c, "--name", &name))
            .unwrap_or_else(|| panic!("no run call for {step}: {:?}", w.docker()))
    }

    fn step_call(w: &W, step: &str) -> Vec<String> {
        w.docker()[run_idx(w, step)].clone()
    }

    fn step_env(w: &W, step: &str) -> Vec<String> {
        env_block(w, "docker.envs", run_idx(w, step))
    }

    /// The run ended with exactly `line` on stderr and no container.
    fn refused(w: &W, line: &str) {
        assert_eq!(w.stderr(), format!("[runner] {line}\n"));
        assert_eq!(w.started_containers(), 0, "{:?}", w.docker());
        assert!(!w.stdout().contains("pipeline passed"), "{}", w.stdout());
        assert_public(w);
    }

    fn assert_no_name_or_count(w: &W) {
        let all = format!("{}{}", w.stdout(), w.stderr());
        for bad in [KEY, "OTHER_KEY", "expected", "found", "unusable", "#0"] {
            assert!(!all.contains(bad), "{bad} in a public line: {all}");
        }
    }

    fn detail(w: &W) -> Vec<String> {
        [lines(&w, "ci", "stderr"), lines(&w, "ci", "stdout")].concat()
    }

    /// H1: a step that holds secrets ships its state lines (`started`, `success in`, `failed in`) and nothing
    /// it printed.
    fn no_step_output_shipped(w: &W, step: &str) {
        for stream in ["stdout", "stderr"] {
            let out = lines(w, step, stream);
            let state = |l: &String| {
                l == "started"
                    || l.starts_with("success in ")
                    || l.starts_with("failed in ")
                    || l.starts_with("skipped")
            };
            assert!(out.iter().all(state), "{step} {stream}: {out:?}");
        }
    }
    /// A docker stub body: the step prints its secret.
    fn print_secret() -> String {
        "case \"$1\" in run) printf '%s\\n' \"$SITE_API_KEY\";; esac".to_string()
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    fn pct(s: &str) -> String {
        s.bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect()
    }

    // ---- US1: a step gets its secret ----

    #[test]
    fn step_secrets_reach_only_the_step_that_lists_them() {
        let pl = [one_step("one", &[KEY]), one_step("two", &[])].concat();
        let w = world(&pl, &entry(MAIN, &[KEY]), &[(KEY, VAL)]);
        assert_eq!(w.ok(), 0, "{}", w.stderr());
        assert_eq!(w.bws_calls(), 1);
        let one = step_call(&w, "one");
        assert!(pair(&one, "-e", KEY), "{one:?}");
        assert!(
            !one.iter().any(|a| a.contains(VAL)),
            "value in argv: {one:?}"
        );
        let two = step_call(&w, "two");
        assert!(!two.iter().any(|a| a == KEY || a.contains(VAL)), "{two:?}");
        assert!(step_env(&w, "one").contains(&format!("{KEY}={VAL}")));
        assert!(!step_env(&w, "two")
            .iter()
            .any(|l| l.starts_with(&format!("{KEY}=")) || l.contains(VAL)));
        // the four runner values never reach a step (I5)
        let envs = w.text("docker.envs");
        for n in ["FACTORY_CI_", "BWS_"] {
            assert!(!envs.contains(n), "{n} in an engine child env");
        }
        for (_, v) in w.secrets.iter().filter(|(k, _)| k != KEY) {
            assert!(!envs.contains(v.as_str()), "a runner value reached a step");
        }
        assert_eq!(w.stderr(), "");
        assert_public(&w);
    }

    #[test]
    fn two_steps_listing_one_name_get_one_value() {
        let pl = [one_step("one", &[KEY]), one_step("two", &[KEY])].concat();
        let w = world(&pl, &entry(MAIN, &[KEY]), &[(KEY, VAL)]);
        assert_eq!(w.ok(), 0, "{}", w.stderr());
        for s in ["one", "two"] {
            let c = step_call(&w, s);
            assert_eq!(c.iter().filter(|a| *a == KEY).count(), 1, "{s}: {c:?}");
            assert!(pair(&c, "-e", KEY), "{s}");
            assert!(step_env(&w, s).contains(&format!("{KEY}={VAL}")), "{s}");
        }
        assert_eq!(w.bws_calls(), 1);
    }

    #[test]
    fn single_allowed_name_runs() {
        let w = world(&req(&[KEY]), &entry(MAIN, &[KEY]), &[(KEY, VAL)]);
        assert_eq!(w.ok(), 0, "{}", w.stderr());
        assert_eq!(
            norm(&w.stdout()),
            [
                "[runner] log store answered HTTP 204",
                "[a] started",
                "[a] success in Ns",
                "[runner] pipeline passed"
            ]
        );
        assert_eq!(w.stderr(), "");
    }

    #[test]
    fn eight_char_value_runs_and_a_seven_char_value_is_unavailable() {
        let mut w = world(&req(&[KEY]), &entry(MAIN, &[KEY]), &[(KEY, "abcdefgh")]);
        w.docker_body = print_secret();
        assert_eq!(w.ok(), 0, "{}", w.stderr());
        assert!(step_env(&w, "a").contains(&format!("{KEY}=abcdefgh")));
        no_step_output_shipped(&w, "a");
        assert!(!w.hub.bodies().contains("abcdefgh"));
        // the floor is 8 characters (L5): fail closed, no container
        let w = world(&req(&[KEY]), &entry(MAIN, &[KEY]), &[(KEY, "abcdefg")]);
        assert_eq!(w.ok(), 1);
        refused(&w, "step secret unavailable");
    }

    #[test]
    fn blank_inner_line_value_runs_and_masks_each_line() {
        let v = "-----BEGIN FAKE KEY-----\n\nbody-line-1234\n-----END FAKE KEY-----";
        let mut w = world(&req(&[KEY]), &entry(MAIN, &[KEY]), &[(KEY, v)]);
        w.docker_body = print_secret();
        assert_eq!(w.ok(), 0, "{}", w.stderr());
        assert!(
            w.text("docker.envs").contains("body-line-1234"),
            "delivered whole"
        );
        let bodies = w.hub.bodies();
        for part in ["BEGIN FAKE", "body-line-1234", "END FAKE"] {
            assert!(!bodies.contains(part), "{part} in Loki");
        }
        no_step_output_shipped(&w, "a");
    }

    #[test]
    fn an_allowlisted_key_with_a_bad_value_does_not_fail_a_run_that_does_not_request_it() {
        let store = [
            (KEY, VAL),
            ("BAD_KEY", "bad\0value-1"),
            ("DUP_KEY", "fake-d1-1111"),
            ("DUP_KEY", "fake-d2-2222"),
        ];
        let allowed = [KEY, "BAD_KEY", "DUP_KEY", "MISSING_KEY"];
        let w = world(&req(&[KEY]), &entry(MAIN, &allowed), &store);
        assert_eq!(w.ok(), 0, "{}", w.stderr());
        assert_eq!(w.stderr(), "");
    }

    // ---- US2: allowlist ----

    #[test]
    fn unlisted_name_fails_before_any_container() {
        let store = [(KEY, VAL), ("OTHER_KEY", "fake-other-value-9")];
        let w = world(&req(&["OTHER_KEY"]), &entry(MAIN, &[KEY]), &store);
        assert_eq!(w.ok(), 1);
        refused(&w, "step secret not allowed");
        assert!(
            w.docker().iter().all(|c| c[0] == "version"),
            "{:?}",
            w.docker()
        );
        // the name goes to Loki, never to a public line; no value anywhere
        assert!(w.hub.bodies().contains("OTHER_KEY"), "the name is in Loki");
        assert!(!format!("{}{}", w.stdout(), w.stderr()).contains("OTHER_KEY"));
        for v in [VAL, "fake-other-value-9"] {
            assert!(!w.hub.bodies().contains(v), "{v} in Loki");
            assert!(!w.text("docker.calls").contains(v));
        }
        assert_no_name_or_count(&w);
    }

    #[test]
    fn entry_without_secrets_key_refuses_a_requested_name() {
        let watch = format!("[[watch]]\nrepo = \"o/r\"\nid = 42\n{MAIN}");
        let w = world(&req(&[KEY]), &watch, &[(KEY, VAL)]);
        assert_eq!(w.ok(), 1);
        refused(&w, "step secret not allowed");
        // an empty list is the same
        let w = world(&req(&[KEY]), &entry(MAIN, &[]), &[(KEY, VAL)]);
        assert_eq!(w.ok(), 1);
        refused(&w, "step secret not allowed");
    }

    #[test]
    fn name_differing_in_case_is_not_allowed() {
        let store = [(KEY, VAL), ("site_api_key", "fake-lower-value-3")];
        let w = world(&req(&["site_api_key"]), &entry(MAIN, &[KEY]), &store);
        assert_eq!(w.ok(), 1);
        refused(&w, "step secret not allowed");
        assert!(!w.hub.bodies().contains("fake-lower-value-3"));
    }

    #[test]
    fn runner_setting_in_the_pipeline_is_refused() {
        for n in RUNNER_NAMES {
            let w = world(&req(&[n]), &entry(MAIN, &[]), &[]);
            assert_eq!(w.ok(), 1, "{n}");
            refused(&w, "pipeline file refused");
            assert_no_marker(&w);
            assert_eq!(w.bws_calls(), 1, "{n}");
        }
    }

    #[test]
    fn runner_setting_in_the_entry_refuses_the_watch_file() {
        for n in RUNNER_NAMES {
            let w = world(&req(&[KEY]), &entry(MAIN, &[n]), &[(KEY, VAL)]);
            assert_eq!(w.ok(), 1, "{n}");
            assert_eq!(
                w.stderr(),
                "[runner] repository or ref not in the watch list\n"
            );
            assert_eq!(w.bws_calls(), 0, "{n}");
            assert_eq!(w.docker().len() + w.gits().len(), 0, "{n}");
            assert!(
                w.hub.pushes().is_empty() && w.hub.api_calls().is_empty(),
                "{n}"
            );
        }
    }

    // ---- US2: ref gate and docker ----

    #[test]
    fn docker_step_in_a_pipeline_with_secrets_is_refused() {
        // no flag, entry does not allow docker
        let w = world(&docker_and_secrets(), &entry(MAIN, &[KEY]), &[(KEY, VAL)]);
        assert_eq!(w.ok(), 1);
        refused(&w, "docker steps are refused");
        // flag, entry and a pinned image: a docker step alone would run, with secrets in the pipeline it does not
        let allow = format!("{MAIN}allow_docker = true\n");
        let mut w = world(&docker_and_secrets(), &entry(&allow, &[KEY]), &[(KEY, VAL)]);
        w.env
            .push(("FACTORY_CI_DIND_IMAGE".to_string(), PIN.to_string()));
        let mut a = w.args();
        a.push("--allow-docker".to_string());
        assert_eq!(w.go(&a), 1);
        refused(&w, "docker steps are refused");
        assert!(
            !w.docker().iter().any(|c| c.contains(&PIN.to_string())),
            "no sidecar"
        );
        // the docker rule comes before the allowlist
        let w = world(&docker_and_secrets(), &entry(MAIN, &[]), &[]);
        assert_eq!(w.ok(), 1);
        refused(&w, "docker steps are refused");
    }

    #[test]
    fn secrets_with_branches_omitted_or_star_are_refused() {
        for (label, extra) in [
            ("omitted", ""),
            ("star", "branches = [\"*\"]\n"),
            ("question star", "branches = [\"?*\"]\n"),
            ("star question", "branches = [\"*?\"]\n"),
        ] {
            let w = world(&req(&[KEY]), &entry(extra, &[KEY]), &[(KEY, VAL)]);
            assert_eq!(w.ok(), 1, "{label}");
            refused(&w, "repository or ref not in the watch list");
            assert_eq!(w.bws_calls(), 1, "{label}");
            assert!(!w.hub.bodies().contains(VAL), "{label}");
        }
    }

    #[test]
    fn secret_branches_must_match_the_ref() {
        // the ref is `main`
        let w = world(
            &req(&[KEY]),
            &entry("secret_branches = [\"release/*\"]\n", &[KEY]),
            &[(KEY, VAL)],
        );
        assert_eq!(w.ok(), 1);
        refused(&w, "repository or ref not in the watch list");
        for sb in ["[\"main\"]", "[\"*\"]"] {
            let w = world(
                &req(&[KEY]),
                &entry(&format!("secret_branches = {sb}\n"), &[KEY]),
                &[(KEY, VAL)],
            );
            assert_eq!(w.ok(), 0, "{sb}: {}", w.stderr());
            assert!(step_env(&w, "a").contains(&format!("{KEY}={VAL}")), "{sb}");
        }
    }

    #[test]
    fn explicit_branches_main_gets_secrets() {
        let w = world(&req(&[KEY]), &entry(MAIN, &[KEY]), &[(KEY, VAL)]);
        assert_eq!(w.ok(), 0, "{}", w.stderr());
        assert!(step_env(&w, "a").contains(&format!("{KEY}={VAL}")));
        // another ref is not in `branches`: the watch gate, before the store
        let w = world(&req(&[KEY]), &entry(MAIN, &[KEY]), &[(KEY, VAL)]);
        assert_eq!(w.go(&set(w.args(), "--ref", Some("dev"))), 1);
        assert_eq!(
            w.stderr(),
            "[runner] repository or ref not in the watch list\n"
        );
        assert_eq!(w.bws_calls(), 0);
    }

    #[test]
    fn a_pipeline_without_secrets_runs_on_branches_star() {
        // the gate is for pipelines that ask for secrets
        let w = W::new(&pipe(&["a"]));
        assert_eq!(w.ok(), 0, "{}", w.stderr());
        let w = world(&pipe(&["a"]), &entry("", &[KEY]), &[(KEY, VAL)]);
        assert_eq!(w.ok(), 0, "{}", w.stderr());
        assert!(
            !w.text("docker.calls").contains(KEY),
            "an unrequested name is not passed on"
        );
    }

    #[test]
    fn rules_are_checked_in_the_documented_order() {
        // unlisted name and a refused ref: the allowlist comes first
        let w = world(&req(&["OTHER_KEY"]), &entry("", &[KEY]), &[(KEY, VAL)]);
        assert_eq!(w.ok(), 1);
        refused(&w, "step secret not allowed");
        // refused ref and a missing value: the gate comes before the lookup
        let w = world(&req(&[KEY]), &entry("", &[KEY]), &[]);
        assert_eq!(w.ok(), 1);
        refused(&w, "repository or ref not in the watch list");
        // a runner setting and docker: the pipeline is refused first
        let both = format!(
            "{}[steps.d]\nimage = \"alpine\"\ncommands = [\"x\"]\ndocker = true\n",
            one_step("s", &["FACTORY_CI_TOKEN"])
        );
        let w = world(&both, &entry(MAIN, &[]), &[]);
        assert_eq!(w.ok(), 1);
        refused(&w, "pipeline file refused");
    }

    // ---- US3: a missing or bad value stops early ----

    #[test]
    fn unavailable_value_ends_the_run_before_any_container() {
        let big = "a".repeat(32769);
        let none = format!("secret {KEY}: expected 1, found 0");
        let twice = format!("secret {KEY}: expected 1, found 2");
        let bad = format!("secret {KEY}: unusable value");
        let rows: Vec<(&str, Vec<&str>, &str, &str)> = vec![
            ("missing", vec![], none.as_str(), ""),
            (
                "duplicate",
                vec!["fake-dup-1111", "fake-dup-2222"],
                twice.as_str(),
                "fake-dup-1111",
            ),
            ("empty", vec![""], bad.as_str(), ""),
            ("blank", vec!["   "], bad.as_str(), ""),
            ("NUL", vec!["fake\0nul-7777"], bad.as_str(), "nul-7777"),
            (
                "3-character line",
                vec!["good-line-1111\nabc"],
                bad.as_str(),
                "good-line-1111",
            ),
            (
                "trailing newline",
                vec!["fake-trail-5555\n"],
                bad.as_str(),
                "fake-trail-5555",
            ),
            ("over 32 KiB", vec![big.as_str()], bad.as_str(), ""),
        ];
        for (label, vals, want, leak) in rows {
            let store: Vec<(&str, &str)> = vals.iter().map(|v| (KEY, *v)).collect();
            let w = world(&req(&[KEY]), &entry(MAIN, &[KEY]), &store);
            assert_eq!(w.ok(), 1, "{label}");
            refused(&w, "step secret unavailable");
            assert_eq!(w.bws_calls(), 1, "{label}");
            assert!(
                detail(&w).iter().any(|l| l == want),
                "{label}: Loki has {:?}",
                detail(&w)
            );
            if !leak.is_empty() {
                assert!(!w.hub.bodies().contains(leak), "{label}: value in Loki");
            }
            assert_no_name_or_count(&w);
        }
    }

    #[test]
    fn every_run_makes_one_bws_call() {
        let cases: Vec<(&str, W, i32)> = vec![
            ("no secrets", W::new(&pipe(&["a"])), 0),
            (
                "a secret",
                world(&req(&[KEY]), &entry(MAIN, &[KEY]), &[(KEY, VAL)]),
                0,
            ),
            (
                "not allowed",
                world(&req(&[KEY]), &entry(MAIN, &[]), &[(KEY, VAL)]),
                1,
            ),
            ("missing", world(&req(&[KEY]), &entry(MAIN, &[KEY]), &[]), 1),
            (
                "ref gate",
                world(&req(&[KEY]), &entry("", &[KEY]), &[(KEY, VAL)]),
                1,
            ),
            (
                "docker",
                world(&docker_and_secrets(), &entry(MAIN, &[KEY]), &[(KEY, VAL)]),
                1,
            ),
            (
                "runner setting",
                world(&req(&["FACTORY_CI_TOKEN"]), &entry(MAIN, &[]), &[]),
                1,
            ),
        ];
        for (label, w, code) in cases {
            assert_eq!(w.ok(), code, "{label}: {}", w.stderr());
            assert_eq!(w.bws_calls(), 1, "{label}");
        }
    }

    // ---- US4: nothing leaks ----

    #[test]
    fn hostile_step_cannot_print_a_secret_anywhere() {
        let v = "fake hostile/Zq81=key+1";
        let (pv, hv) = (pct(v), hex(v.as_bytes()));
        let mut w = world(&req(&[KEY]), &entry(MAIN, &[KEY]), &[(KEY, v)]);
        w.docker_body = format!(
            "case \"$1\" in run)\n\
             echo \"$SITE_API_KEY\"\n\
             echo \"$SITE_API_KEY\" | base64\n\
             printf 'x%s' \"$SITE_API_KEY\" | base64\n\
             printf '%s\\n' '{pv}' '{hv}'\n\
             echo \"line prefix-$SITE_API_KEY-suffix\"\n\
             ;; esac"
        );
        assert_eq!(w.ok(), 0, "{}", w.stderr());
        let needles = [
            v.to_string(),
            base64(format!("{v}\n").as_bytes()),
            base64(format!("x{v}").as_bytes()),
            base64(v.as_bytes()),
            pv,
            hv,
        ];
        let hay = [
            ("stdout", w.stdout()),
            ("stderr", w.stderr()),
            ("docker argv", w.text("docker.calls")),
            ("bws argv", w.text("bws.calls")),
            ("git argv", w.text("git.calls")),
            ("loki", w.hub.bodies()),
        ];
        for n in &needles {
            for (label, text) in &hay {
                assert!(!text.contains(n.as_str()), "{n} found in {label}");
            }
        }
        no_step_output_shipped(&w, "a");
        assert_public(&w);
    }

    #[test]
    fn a_secret_named_pass_keeps_pipeline_passed_on_stdout() {
        // the floor is 8 characters, so the values are the longest words and phrases of the public lines
        for v in ["pipeline", "pipeline passed", "log store answered HTTP"] {
            let w = world(&req(&[KEY]), &entry(MAIN, &[KEY]), &[(KEY, v)]);
            assert_eq!(w.ok(), 0, "{v}: {}", w.stderr());
            assert_eq!(
                norm(&w.stdout()),
                [
                    "[runner] log store answered HTTP 204",
                    "[a] started",
                    "[a] success in Ns",
                    "[runner] pipeline passed"
                ],
                "{v}"
            );
            assert_eq!(w.stderr(), "", "{v}");
            assert_public(&w);
        }
        // Loki holds the redacted text
        let w = world(&req(&[KEY]), &entry(MAIN, &[KEY]), &[(KEY, "pipeline")]);
        assert_eq!(w.ok(), 0);
        let runner = lines(&w, "runner", "stdout");
        assert!(runner.iter().any(|l| l == "*** passed"), "{runner:?}");
    }

    #[test]
    fn no_public_line_holds_a_secret_name_or_count() {
        let big = "a".repeat(32769);
        let store = [(KEY, VAL), ("OTHER_KEY", "fake-other-value-9")];
        let worlds: Vec<(&str, W)> = vec![
            (
                "not allowed",
                world(&req(&["OTHER_KEY"]), &entry(MAIN, &[KEY]), &store),
            ),
            ("missing", world(&req(&[KEY]), &entry(MAIN, &[KEY]), &[])),
            (
                "duplicate",
                world(
                    &req(&[KEY]),
                    &entry(MAIN, &[KEY]),
                    &[(KEY, "fake-d1-1111"), (KEY, "fake-d2-2222")],
                ),
            ),
            (
                "unusable",
                world(&req(&[KEY]), &entry(MAIN, &[KEY]), &[(KEY, big.as_str())]),
            ),
            ("ref gate", world(&req(&[KEY]), &entry("", &[KEY]), &store)),
            (
                "docker",
                world(&docker_and_secrets(), &entry(MAIN, &[KEY]), &store),
            ),
            ("success", world(&req(&[KEY]), &entry(MAIN, &[KEY]), &store)),
        ];
        for (label, w) in worlds {
            w.ok();
            assert!(w.stderr().lines().count() <= 1, "{label}: {}", w.stderr());
            assert_no_name_or_count(&w);
            assert_public(&w);
            assert!(!w.text("docker.calls").contains(VAL), "{label}");
        }
    }

    // ---- QA: parallel steps, chunk boundaries, wrapped base64 ----

    const K1: &str = "SITE_KEY_ONE";
    const K2: &str = "SITE_KEY_TWO";
    const V1: &str = "fake-one-value-Q7x1-aaaa";
    const V2: &str = "fake-two-value-R8y2-bbbb";

    /// Four steps with no `depends_on`, so the engine starts them together: one needs `K1`, one `K2`, one
    /// both, one nothing. Each container prints both variables.
    #[test]
    fn parallel_steps_get_only_their_own_secrets_and_every_output_is_masked() {
        let pl = [
            one_step("s1", &[K1]),
            one_step("s2", &[K2]),
            one_step("s3", &[K1, K2]),
            one_step("s4", &[]),
        ]
        .concat();
        // repeated: a race between the steps shows up as a missing or a foreign variable
        for round in 0..3 {
            let mut w = world(&pl, &entry(MAIN, &[K1, K2]), &[(K1, V1), (K2, V2)]);
            w.docker_body =
                "case \"$1\" in run) printf '%s\\n' \"$SITE_KEY_ONE\" \"$SITE_KEY_TWO\";; esac"
                    .to_string();
            assert_eq!(w.ok(), 0, "round {round}: {}", w.stderr());
            assert_eq!(w.bws_calls(), 1, "round {round}");
            let want = [
                ("s1", vec![(K1, V1)]),
                ("s2", vec![(K2, V2)]),
                ("s3", vec![(K1, V1), (K2, V2)]),
                ("s4", vec![]),
            ];
            for (step, vars) in want {
                let (call, env) = (step_call(&w, step), step_env(&w, step));
                for (k, v) in [(K1, V1), (K2, V2)] {
                    let listed = vars.contains(&(k, v));
                    assert_eq!(pair(&call, "-e", k), listed, "{step} {k}: {call:?}");
                    assert_eq!(
                        env.contains(&format!("{k}={v}")),
                        listed,
                        "{step} {k}: {env:?}"
                    );
                    assert!(!call.iter().any(|a| a.contains(v)), "{step}: {call:?}");
                    let out = lines(&w, step, "stdout");
                    assert!(!out.iter().any(|l| l.contains(v)), "{step}: {out:?}");
                }
            }
            let (all, bodies) = (format!("{}{}", w.stdout(), w.stderr()), w.hub.bodies());
            for v in [V1, V2] {
                assert!(
                    !all.contains(v) && !bodies.contains(v),
                    "round {round}: {v}"
                );
                assert!(!w.text("docker.calls").contains(v), "round {round}: {v}");
            }
            assert_public(&w);
        }
    }

    /// The value, then its base64 line, each start before the 64 KB read boundary of the output pump and end
    /// after it: `fill` bytes first, with the real engine, shipper and mock Loki.
    fn boundary_world(fill: &str) -> W {
        let mut w = world(&req(&[KEY]), &entry(MAIN, &[KEY]), &[(KEY, VAL)]);
        w.docker_body = format!(
            r#"case "$1" in run)
head -c 65530 /dev/zero | tr '\0' '{fill}'
printf '%s\n' "$SITE_API_KEY"
head -c 65520 /dev/zero | tr '\0' '{fill}'
printf '%s' "$SITE_API_KEY" | base64
;; esac"#
        );
        w
    }

    /// Neither the value nor its base64 form, nor a piece of either that the 64 KB cut left on its own line.
    fn assert_boundary_masked(w: &W) {
        assert_eq!(w.ok(), 0, "{}", w.stderr());
        let b64 = base64(VAL.as_bytes());
        let (all, bodies) = (format!("{}{}", w.stdout(), w.stderr()), w.hub.bodies());
        for n in [VAL, b64.as_str(), &VAL[..6], &b64[..6]] {
            assert!(!all.contains(n) && !bodies.contains(n), "{n} leaked");
        }
        no_step_output_shipped(w, "a");
        assert_public(w);
    }

    #[test]
    fn a_value_across_the_64k_read_boundary_is_masked_in_loki() {
        assert_boundary_masked(&boundary_world(" "));
    }

    // D1 in test-map.md: a run of `a` before the value (selftest run 46115243670) must not push the cut into the
    // value.
    #[test]
    fn a_value_across_the_64k_read_boundary_after_a_run_of_one_character_is_masked_in_loki() {
        assert_boundary_masked(&boundary_world("a"));
    }

    /// `echo "$V" | base64` of a value over 57 bytes wraps at 76 columns: each line is masked by itself.
    #[test]
    fn wrapped_base64_of_a_long_value_is_masked_line_by_line() {
        let v = format!("fake-wrap-Zq81-{}", "k9".repeat(43));
        assert_eq!(v.len(), 101);
        let mut w = world(&req(&[KEY]), &entry(MAIN, &[KEY]), &[(KEY, v.as_str())]);
        w.docker_body = r#"case "$1" in run)
echo "$SITE_API_KEY" | base64
echo "$SITE_API_KEY" | base64 -w 0
;; esac"#
            .to_string();
        assert_eq!(w.ok(), 0, "{}", w.stderr());
        let enc = base64(v.as_bytes());
        // the whole groups of the value: 33 groups of 4 characters, then 76 and 56 columns
        let (first, second) = (&enc[..76], &enc[76..132]);
        let (all, bodies) = (format!("{}{}", w.stdout(), w.stderr()), w.hub.bodies());
        for n in [v.as_str(), first, second, &enc[..132]] {
            assert!(!all.contains(n) && !bodies.contains(n), "{n} leaked");
        }
        no_step_output_shipped(&w, "a");
        assert_public(&w);
    }

    /// US1 scenario 1: the entry allows three names, the store holds each once, one step lists all three.
    #[test]
    fn three_names_in_one_step_all_reach_it() {
        let names = [KEY, K1, K2];
        let vals = [VAL, V1, V2];
        let store: Vec<(&str, &str)> = names.iter().copied().zip(vals).collect();
        let w = world(&req(&names), &entry(MAIN, &names), &store);
        assert_eq!(w.ok(), 0, "{}", w.stderr());
        assert_eq!(w.bws_calls(), 1);
        let (call, env) = (step_call(&w, "a"), step_env(&w, "a"));
        for (n, v) in store {
            assert!(pair(&call, "-e", n), "{n}: {call:?}");
            assert!(env.contains(&format!("{n}={v}")), "{n}: {env:?}");
            assert!(!w.text("docker.calls").contains(v), "{v} in argv");
        }
        assert!(
            w.stdout().ends_with("[runner] pipeline passed\n"),
            "{}",
            w.stdout()
        );
        assert_public(&w);
    }

    /// H1: the output of a step that holds secrets is not shipped; a step without secrets ships it.
    #[test]
    fn output_of_a_step_with_secrets_is_not_shipped() {
        let pl = [one_step("one", &[KEY]), one_step("two", &[])].concat();
        let mut w = world(&pl, &entry(MAIN, &[KEY]), &[(KEY, VAL)]);
        w.docker_body =
            "case \"$1\" in run) echo visible-output-line; echo visible-error-line >&2;; esac"
                .to_string();
        assert_eq!(w.ok(), 0, "{}", w.stderr());
        no_step_output_shipped(&w, "one");
        let one = lines(&w, "one", "stdout");
        assert!(one.contains(&"started".to_string()), "{one:?}");
        assert!(one.iter().any(|l| l.starts_with("success in ")), "{one:?}");
        let two = lines(&w, "two", "stdout");
        assert!(two.contains(&"visible-output-line".to_string()), "{two:?}");
        assert!(lines(&w, "two", "stderr").contains(&"visible-error-line".to_string()));
        // a failing step ships its state and exit code, not its output
        let pl =
            "[steps.one]\nimage = \"alpine\"\ncommands = [\"x\"]\nsecrets = [\"SITE_API_KEY\"]\n";
        let mut w = world(pl, &entry(MAIN, &[KEY]), &[(KEY, VAL)]);
        w.docker_body = "case \"$1\" in run) echo failing-output-line; exit 3;; esac".to_string();
        assert_eq!(w.ok(), 1);
        no_step_output_shipped(&w, "one");
        assert!(!w.hub.bodies().contains("failing-output-line"));
        let out = lines(&w, "one", "stdout");
        assert!(out.iter().any(|l| l.starts_with("failed in ")), "{out:?}");
    }

    /// M3: `branches` never opens secrets; only `secret_branches` does.
    #[test]
    fn explicit_branches_without_secret_branches_get_no_secrets() {
        for (branches, git_ref) in [("main", "main"), ("release/*", "release/1")] {
            let extra = format!("branches = [{branches:?}]\n");
            let w = world(&req(&[KEY]), &entry(&extra, &[KEY]), &[(KEY, VAL)]);
            assert_eq!(w.go(&set(w.args(), "--ref", Some(git_ref))), 1, "{git_ref}");
            refused(&w, "repository or ref not in the watch list");
            assert_eq!(w.bws_calls(), 1, "{git_ref}");
            assert!(!w.hub.bodies().contains(VAL), "{git_ref}");
        }
        // with `secret_branches` the release branch gets them, and `main` does not
        let extra = "branches = [\"main\", \"release/*\"]\nsecret_branches = [\"release/*\"]\n";
        let w = world(&req(&[KEY]), &entry(extra, &[KEY]), &[(KEY, VAL)]);
        assert_eq!(
            w.go(&set(w.args(), "--ref", Some("release/1"))),
            0,
            "{}",
            w.stderr()
        );
        assert!(step_env(&w, "a").contains(&format!("{KEY}={VAL}")));
        let w = world(&req(&[KEY]), &entry(extra, &[KEY]), &[(KEY, VAL)]);
        assert_eq!(w.go(&w.args()), 1);
        refused(&w, "repository or ref not in the watch list");
    }

    /// 012 T022; FR-023, 010 FR-010: the `sudtanj/infrastructure` entry shape (`branches = ["main"]`,
    /// `allow_docker`, both Docker Hub names in `secrets`, no `secret_branches`) unlocks nothing for a manual
    /// `ci` run on any ref. The names are only for the tick's built-in publish.
    #[test]
    fn entry_secrets_without_secret_branches_unlock_nothing() {
        let both = ["DOCKERHUB_USERNAME", "DOCKERHUB_TOKEN"];
        let extra = "branches = [\"main\"]\nallow_docker = true\n";
        for git_ref in ["main", "release/1"] {
            let w = world(
                &req(&["DOCKERHUB_TOKEN"]),
                &entry(extra, &both),
                &[("DOCKERHUB_TOKEN", VAL), ("DOCKERHUB_USERNAME", "sudtanj")],
            );
            assert_eq!(w.go(&set(w.args(), "--ref", Some(git_ref))), 1, "{git_ref}");
            refused(&w, "repository or ref not in the watch list");
            assert!(!w.hub.bodies().contains(VAL), "{git_ref}");
        }
    }

    // ---- QA, after T017a: masking proven through a step that does ship ----
    // H1 makes a step with secrets ship nothing, so the tests above pass without any masking in Loki. Here step
    // `b` lists no secret and prints the value of step `a`, so its lines ship and the shared redactor must mask
    // them.

    /// Step `a` holds the secret, step `b` holds none; both containers run `body`.
    fn plain_step_world(body: &str) -> W {
        let pl = [one_step("a", &[KEY]), one_step("b", &[])].concat();
        let mut w = world(&pl, &entry(MAIN, &[KEY]), &[(KEY, VAL)]);
        w.docker_body = format!("case \"$1\" in run)\n{body}\n;; esac");
        w
    }

    #[test]
    fn a_plain_step_printing_a_value_across_the_64k_cut_is_masked_in_loki() {
        for fill in [" ", "a"] {
            let body = format!(
                r#"head -c 65530 /dev/zero | tr '\0' '{fill}'
printf '%s\n' '{VAL}'
head -c 65520 /dev/zero | tr '\0' '{fill}'
printf '%s' '{VAL}' | base64"#
            );
            let w = plain_step_world(&body);
            assert_eq!(w.ok(), 0, "{fill:?}: {}", w.stderr());
            let b64 = base64(VAL.as_bytes());
            let bodies = w.hub.bodies();
            for n in [VAL, b64.as_str(), &VAL[..6], &b64[..6]] {
                assert!(!bodies.contains(n), "{fill:?}: {n} reached Loki");
            }
            let b = lines(&w, "b", "stdout");
            assert!(
                b.iter().filter(|l| l.contains("***")).count() >= 2,
                "{fill:?}: no masked line, so nothing proves the cut: {:?}",
                b.iter().map(String::len).collect::<Vec<_>>()
            );
            no_step_output_shipped(&w, "a");
            assert_public(&w);
        }
    }

    #[test]
    fn a_plain_step_printing_the_review_forms_ships_only_masked_lines() {
        let body = format!(
            r#"printf '%s\n' '{VAL}'
printf 'prefix-%s-suffix\n' '{VAL}'
echo '{VAL}' | base64
printf '%s' '{VAL}' | od -An -tx1 | tr -d ' \n' | tr a-f A-F; echo"#
        );
        let w = plain_step_world(&body);
        assert_eq!(w.ok(), 0, "{}", w.stderr());
        // the other lines are the state lines and the stub's own `step-output` marker
        let out = lines(&w, "b", "stdout");
        let masked: Vec<_> = out.iter().filter(|l| l.contains("***")).collect();
        assert_eq!(
            masked,
            ["***", "prefix-***-suffix", "***", "***"],
            "{out:?}"
        );
        assert!(!w.hub.bodies().contains(VAL));
        no_step_output_shipped(&w, "a");
        assert_public(&w);
    }

    /// L6 through the binary: a step named like a secret value prints as `step`, never as its name.
    #[test]
    fn a_step_named_like_a_secret_value_prints_as_step() {
        let name = "deploy-prod";
        let w = world(
            &one_step(name, &[KEY]),
            &entry(MAIN, &[KEY]),
            &[(KEY, name)],
        );
        assert_eq!(w.ok(), 0, "{}", w.stderr());
        assert_eq!(
            norm(&w.stdout()),
            [
                "[runner] log store answered HTTP 204",
                "[step] started",
                "[step] success in Ns",
                "[runner] pipeline passed"
            ]
        );
        assert!(!format!("{}{}", w.stdout(), w.stderr()).contains(name));
        assert_public(&w);
    }
}
