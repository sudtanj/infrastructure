//! Shared fixtures for the binary-level tests: a stub `docker` on PATH, a fake GitHub status
//! server and a fake Loki. Test support only; each test file includes it with `mod common;`.
#![allow(dead_code)]

use rust_ci_runner::util::random_id;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const BIN: &str = env!("CARGO_BIN_EXE_rust-ci-runner");
pub const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
pub const RUN_ID: &str = "0123456789abcdef";
pub const PIN: &str =
    "registry.example/img@sha256:0000000000000000000000000000000000000000000000000000000000000000";
pub const JOB_TOKEN: &str = "tok-JOB-CRED-1";

/// Records every call (arguments, one per line, calls separated by `--`) and the client
/// environment of each call. `STUB_FAIL` (substring of the arguments) makes a call exit with
/// `STUB_FAIL_CODE` (default 1). A call whose arguments hold `-clone` copies `STUB_FIXTURE`
/// into the mounted workspace, or fails when `STUB_CLONE_FAIL` is set. Other `run` calls print
/// `STUB_OUT` on stdout and `STUB_ERR` on stderr.
const STUB: &str = r#"#!/bin/sh
d="$STUB_DIR"
printf '%s\n' "$@" >> "$d/calls"
echo -- >> "$d/calls"
/usr/bin/env >> "$d/envs"
echo -- >> "$d/envs"
case "$*" in
  *-clone*)
    echo "stub clone output"
    [ -n "$STUB_CLONE_FAIL" ] && exit 1
    prev=""
    for a in "$@"; do
      [ "$prev" = "-v" ] && ws="${a%%:/workspace}"
      prev="$a"
    done
    [ -n "$STUB_FIXTURE" ] && cp -a "$STUB_FIXTURE/." "$ws/"
    exit 0;;
esac
case "$1" in
  run)
    [ -n "$STUB_OUT" ] && echo "$STUB_OUT"
    [ -n "$STUB_ERR" ] && echo "$STUB_ERR" >&2;;
esac
if [ -n "$STUB_FAIL" ]; then
  case "$*" in *"$STUB_FAIL"*) exit "${STUB_FAIL_CODE:-1}";; esac
fi
exit 0
"#;

pub struct Sandbox {
    pub dir: PathBuf,
    pub bin: PathBuf,
    pub stub: PathBuf,
    pub tmp: PathBuf,
}

impl Sandbox {
    pub fn new() -> Sandbox {
        let dir = std::env::temp_dir().join(format!("fci-sb-{}", random_id()));
        let (bin, stub, tmp) = (dir.join("bin"), dir.join("stub"), dir.join("tmp"));
        for d in [&bin, &stub, &tmp] {
            std::fs::create_dir_all(d).unwrap();
        }
        let docker = bin.join("docker");
        std::fs::write(&docker, STUB).unwrap();
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
        Sandbox {
            dir,
            bin,
            stub,
            tmp,
        }
    }

    /// The binary with an empty environment except PATH (stub first), the stub directory and
    /// a private temp directory. No token, no network settings.
    pub fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.env_clear()
            .args(args)
            .env("PATH", format!("{}:/usr/bin:/bin", self.bin.display()))
            .env("STUB_DIR", &self.stub)
            .env("TMPDIR", &self.tmp)
            .env("RUNNER_TEMP", &self.tmp)
            .env("HOME", &self.dir);
        c
    }

    /// A checkout the stub clone copies into the workspace. Set `STUB_FIXTURE` to the result.
    pub fn fixture(&self, files: &[(&str, &str)]) -> PathBuf {
        let d = self.dir.join("fixture");
        std::fs::create_dir_all(&d).unwrap();
        for (name, text) in files {
            let p = d.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        d
    }

    pub fn write(&self, name: &str, text: &str) -> PathBuf {
        let p = self.dir.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        p
    }

    /// Every stub call as its argument list.
    pub fn calls(&self) -> Vec<Vec<String>> {
        let text = std::fs::read_to_string(self.stub.join("calls")).unwrap_or_default();
        let mut out = vec![vec![]];
        for l in text.lines() {
            if l == "--" {
                out.push(vec![]);
            } else {
                out.last_mut().unwrap().push(l.to_string());
            }
        }
        out.retain(|c| !c.is_empty());
        out
    }

    /// `run` calls that are not the clone container.
    pub fn step_runs(&self) -> Vec<Vec<String>> {
        self.calls()
            .into_iter()
            .filter(|c| {
                c.first().map(String::as_str) == Some("run")
                    && !c.iter().any(|a| a.ends_with("-clone"))
            })
            .collect()
    }

    /// The environments of every stub call, concatenated.
    pub fn client_envs(&self) -> String {
        std::fs::read_to_string(self.stub.join("envs")).unwrap_or_default()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub fn out_text(o: &Output) -> String {
    format!("{}{}", stdout(o), stderr(o))
}

pub fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

pub fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

#[derive(Clone, Debug)]
pub struct Req {
    pub method: String,
    pub url: String,
    pub auth: String,
    pub body: String,
}

/// A local HTTP server that records requests. `code` is the status of the answers;
/// `hang_ms` delays every answer; the first `fail_first` answers are 500.
pub struct Fake {
    pub url: String,
    pub reqs: Arc<Mutex<Vec<Req>>>,
    pub code: Arc<AtomicU16>,
    pub hang_ms: Arc<AtomicU16>,
    pub fail_first: Arc<AtomicU16>,
}

impl Fake {
    pub fn start() -> Fake {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let f = Fake {
            url: format!("http://127.0.0.1:{port}"),
            reqs: Arc::default(),
            code: Arc::new(AtomicU16::new(201)),
            hang_ms: Arc::default(),
            fail_first: Arc::default(),
        };
        let (reqs, code, hang, ff) = (
            f.reqs.clone(),
            f.code.clone(),
            f.hang_ms.clone(),
            f.fail_first.clone(),
        );
        std::thread::spawn(move || {
            for mut rq in server.incoming_requests() {
                let mut body = String::new();
                let _ = rq.as_reader().read_to_string(&mut body);
                let auth = rq
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Authorization"))
                    .map(|h| h.value.to_string())
                    .unwrap_or_default();
                reqs.lock().unwrap().push(Req {
                    method: rq.method().to_string(),
                    url: rq.url().to_string(),
                    auth,
                    body,
                });
                std::thread::sleep(Duration::from_millis(hang.load(Ordering::Relaxed) as u64));
                let status = if ff.load(Ordering::Relaxed) > 0 {
                    ff.fetch_sub(1, Ordering::Relaxed);
                    500
                } else {
                    code.load(Ordering::Relaxed)
                };
                let _ = rq.respond(tiny_http::Response::from_string("{}").with_status_code(status));
            }
        });
        f
    }

    pub fn requests(&self) -> Vec<Req> {
        self.reqs.lock().unwrap().clone()
    }

    /// Bodies of status posts as JSON.
    pub fn statuses(&self) -> Vec<serde_json::Value> {
        self.requests()
            .iter()
            .filter(|r| r.method == "POST" && r.url.contains("/statuses/"))
            .map(|r| serde_json::from_str(&r.body).unwrap_or_default())
            .collect()
    }

    /// Every `(labels, line)` received on `/loki/api/v1/push`.
    pub fn loki_lines(&self) -> Vec<(serde_json::Value, String)> {
        let mut out = vec![];
        for r in self
            .requests()
            .iter()
            .filter(|r| r.url.ends_with("/loki/api/v1/push"))
        {
            let v: serde_json::Value = serde_json::from_str(&r.body).unwrap_or_default();
            for s in v["streams"].as_array().cloned().unwrap_or_default() {
                for val in s["values"].as_array().cloned().unwrap_or_default() {
                    out.push((
                        s["stream"].clone(),
                        val[1].as_str().unwrap_or("").to_string(),
                    ));
                }
            }
        }
        out
    }
}

/// A pipeline file with one step per `(name, extra toml)`; each runs `echo <name>`.
pub fn pipeline(steps: &[(&str, &str)]) -> String {
    steps
        .iter()
        .map(|(n, extra)| {
            format!("[steps.{n}]\nimage = \"alpine\"\ncommands = [\"echo {n}\"]\n{extra}\n")
        })
        .collect()
}

/// What every `job` call needs: the token, the API base and pinned images.
pub fn job_env(c: &mut Command, api: &str) {
    c.env("FACTORY_CI_TOKEN", JOB_TOKEN)
        .env("GITHUB_API_URL", api)
        .env("FACTORY_CI_CLONE_IMAGE", PIN)
        .env("FACTORY_CI_DIND_IMAGE", PIN);
}

/// The log store variables for a fake Loki at `url`.
pub fn log_env(c: &mut Command, url: &str) {
    c.env("FACTORY_CI_LOG_URL", url)
        .env("FACTORY_CI_LOG_USER", "loki-user")
        .env("FACTORY_CI_LOG_TOKEN", "loki-pass-77");
}

pub fn job_args<'a>(extra: &[&'a str]) -> Vec<&'a str> {
    let mut v = vec![
        "job",
        "--repo",
        "o/r",
        "--sha",
        SHA,
        "--project",
        ".",
        "--branch",
        "main",
        "--run-id",
        RUN_ID,
    ];
    v.extend_from_slice(extra);
    v
}

pub fn dir_str(p: &Path) -> &str {
    p.to_str().unwrap()
}
