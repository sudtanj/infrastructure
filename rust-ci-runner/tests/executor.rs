//! Runs the executor against the real container engine on this machine.

mod common;

use rust_ci_runner::engine::DIND_IMAGE;
use rust_ci_runner::engine::{execute, Engine, Outcome, RunCtx, State};
use rust_ci_runner::log::Log;
use rust_ci_runner::pipeline;
use rust_ci_runner::redact::Redactor;
use rust_ci_runner::util::random_id;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
struct Buf(Arc<Mutex<Vec<u8>>>);

impl Write for Buf {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Buf {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).to_string()
    }
}

fn docker(args: &[&str]) -> String {
    let out = Command::new("docker").args(args).output().unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn pull_once() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        for image in ["busybox", "docker:27.5.1-cli", "alpine/git", DIND_IMAGE] {
            let _ = docker(&["pull", "-q", image]);
        }
    });
}

fn tmp() -> PathBuf {
    let d = std::env::temp_dir().join(format!("fci-it-{}", random_id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

struct Run {
    out: Outcome,
    log: String,
    id: String,
}

struct Opts<'a> {
    parallel: usize,
    secrets: &'a [(&'a str, &'a str)],
    deadline_secs: u64,
    files: &'a [(&'a str, &'a str)],
}

fn opts() -> Opts<'static> {
    Opts {
        parallel: 4,
        secrets: &[],
        deadline_secs: 600,
        files: &[],
    }
}

fn go(toml: &str, o: Opts) -> Run {
    pull_once();
    let p = pipeline::parse(toml).unwrap();
    let ws = tmp();
    for (name, text) in o.files {
        std::fs::write(ws.join(name), text).unwrap();
    }
    let buf = Buf::default();
    let log = Log::new(
        Box::new(buf.clone()),
        Redactor::new(o.secrets.iter().map(|(_, v)| v.to_string())),
        1 << 20,
    );
    let engine = Engine::new("docker");
    let id = random_id();
    let ctx = RunCtx {
        engine: &engine,
        run_id: &id,
        workspace: &ws,
        env: vec![("FACTORY_CI_SHA".to_string(), "abc123".to_string())],
        secrets: o
            .secrets
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        max_parallel: o.parallel,
        deadline: Instant::now() + Duration::from_secs(o.deadline_secs),
        log: &log,
        dind_image: DIND_IMAGE,
    };
    let out = execute(&ctx, &p).unwrap();
    engine.cleanup_run(&id);
    // Files made by root in a container may not be removable here; leave them in /tmp.
    let _ = std::fs::remove_dir_all(&ws);
    Run {
        out,
        log: buf.text(),
        id,
    }
}

/// Containers, networks and volumes of the run that still exist.
fn leftovers(id: &str) -> Vec<String> {
    let label = format!("label=factory-ci.run={id}");
    let mut found = Vec::new();
    let kinds: [&[&str]; 3] = [
        &["ps", "-aq"],
        &["network", "ls", "-q"],
        &["volume", "ls", "-q"],
    ];
    for kind in kinds {
        let mut a: Vec<&str> = kind.to_vec();
        a.push("--filter");
        a.push(&label);
        found.extend(docker(&a).split_whitespace().map(str::to_string));
    }
    found
}

/// start and end seconds of a step, printed by the step named `report`.
fn times(log: &str, step: &str) -> (u64, u64) {
    let line = log
        .lines()
        .find(|l| l.starts_with("[report] T "))
        .expect("report line");
    let words: Vec<&str> = line.split_whitespace().collect();
    let at = words.iter().position(|w| *w == step).unwrap();
    (
        words[at + 1].parse().unwrap(),
        words[at + 2].parse().unwrap(),
    )
}

const TWO_PARALLEL: &str = r#"
[steps.a]
image = "busybox"
commands = ["date +%s > a.start", "sleep 4", "date +%s > a.end"]

[steps.b]
image = "busybox"
commands = ["date +%s > b.start", "sleep 4", "date +%s > b.end"]

[steps.report]
image = "busybox"
depends_on = ["a", "b"]
commands = ['echo T a $(cat a.start) $(cat a.end) b $(cat b.start) $(cat b.end)']
"#;

#[test]
fn independent_steps_overlap_and_the_limit_is_honoured() {
    let r = go(TWO_PARALLEL, opts());
    assert!(r.out.ok(), "{}", r.log);
    let (a, b) = (times(&r.log, "a"), times(&r.log, "b"));
    assert!(
        a.0 < b.1 && b.0 < a.1,
        "steps did not overlap: {a:?} {b:?}\n{}",
        r.log
    );

    let r = go(
        TWO_PARALLEL,
        Opts {
            parallel: 1,
            ..opts()
        },
    );
    assert!(r.out.ok(), "{}", r.log);
    let (a, b) = (times(&r.log, "a"), times(&r.log, "b"));
    assert!(
        a.1 <= b.0 || b.1 <= a.0,
        "steps overlapped with a limit of 1: {a:?} {b:?}"
    );
}

#[test]
fn a_dependent_step_sees_files_written_by_an_earlier_step() {
    let r = go(
        r#"
[steps.write]
image = "busybox"
commands = ["echo hello > out.txt"]

[steps.read]
image = "busybox"
depends_on = ["write"]
commands = ['test "$(cat out.txt)" = hello', "echo [read] saw it"]
"#,
        opts(),
    );
    assert!(r.out.ok(), "{}", r.log);
    assert!(r.log.contains("[read] saw it"));
}

#[test]
fn failure_skips_dependents_but_not_independent_steps() {
    let r = go(
        r#"
[steps.bad]
image = "busybox"
commands = ["echo before", "exit 3", "echo SHOULD_NOT_PRINT"]

[steps.after]
image = "busybox"
depends_on = ["bad"]
commands = ["echo never"]

[steps.free]
image = "busybox"
commands = ["echo free-ran"]
"#,
        opts(),
    );
    assert!(!r.out.ok());
    assert_eq!(
        r.out.failed,
        Some(("bad".to_string(), "exit code 3".to_string()))
    );
    assert!(matches!(r.out.states["after"], State::Skipped(_)));
    assert_eq!(r.out.states["free"], State::Success);
    assert!(r.log.contains("[bad] before"));
    assert!(r.log.contains("[free] free-ran"));
    assert!(r.log.contains("[after] skipped"));
    assert!(!r.log.contains("SHOULD_NOT_PRINT") && !r.log.contains("never"));
}

#[test]
fn a_timeout_kills_the_step_and_leaves_no_container() {
    let start = Instant::now();
    let r = go(
        r#"
[steps.hang]
image = "busybox"
timeout = 3
commands = ["sleep 120"]
"#,
        opts(),
    );
    assert!(
        start.elapsed() < Duration::from_secs(60),
        "the step was not killed"
    );
    assert_eq!(
        r.out.failed.as_ref().map(|f| f.1.as_str()),
        Some("timed out after 3s")
    );
    assert!(leftovers(&r.id).is_empty(), "{:?}", leftovers(&r.id));
}

#[test]
fn the_run_deadline_kills_running_steps_and_skips_the_rest() {
    let r = go(
        r#"
[steps.hang]
image = "busybox"
commands = ["sleep 120"]

[steps.next]
image = "busybox"
depends_on = ["hang"]
commands = ["echo x"]
"#,
        Opts {
            deadline_secs: 3,
            ..opts()
        },
    );
    assert!(r.out.failed.is_some());
    assert!(matches!(r.out.states["next"], State::Skipped(_)));
    assert!(leftovers(&r.id).is_empty());
}

#[test]
fn steps_have_no_engine_socket_and_only_their_own_secrets() {
    let r = go(
        r#"
[steps.nosock]
image = "busybox"
secrets = ["TOKEN"]
env = { PLAIN = "visible" }
commands = [
  'test ! -e /var/run/docker.sock',
  'test ! -e /run/docker.sock',
  'test -z "$DOCKER_HOST"',
  'test "$FACTORY_CI_SHA" = abc123',
  'test "$PLAIN" = visible',
  'test "$TOKEN" = s3cret-value',
  'echo token is $TOKEN',
]

[steps.other]
image = "busybox"
commands = ['test -z "$TOKEN"']
"#,
        Opts {
            secrets: &[("TOKEN", "s3cret-value")],
            ..opts()
        },
    );
    assert!(r.out.ok(), "{}", r.log);
    assert!(r.log.contains("[nosock] token is ***"));
    assert!(!r.log.contains("s3cret-value"));
}

#[test]
fn a_docker_step_builds_and_runs_an_image_and_everything_is_cleaned_up() {
    let r = go(
        r#"
[steps.img]
image = "docker:27.5.1-cli"
docker = true
commands = ["docker build -q -t fci-built .", "docker run --rm fci-built"]

[steps.plain]
image = "busybox"
commands = ['test -z "$DOCKER_HOST"', 'test ! -e /var/run/docker.sock']
"#,
        Opts {
            files: &[("Dockerfile", "FROM busybox\nCMD [\"echo\", \"built-ok\"]\n")],
            ..opts()
        },
    );
    assert!(r.out.ok(), "{}", r.log);
    assert!(r.log.contains("[img] built-ok"), "{}", r.log);
    assert!(leftovers(&r.id).is_empty(), "{:?}", leftovers(&r.id));
}

#[test]
fn concurrent_runs_cannot_see_each_others_images() {
    let pipeline = |tag: &str| {
        format!(
            "[steps.img]\nimage = \"docker:27.5.1-cli\"\ndocker = true\ncommands = [\"docker build -q -t {tag} .\", \"sleep 8\", \"docker images\"]\n"
        )
    };
    let files: &'static [(&str, &str)] = &[("Dockerfile", "FROM busybox\n")];
    let (a, b) = std::thread::scope(|s| {
        let a = s.spawn(|| go(&pipeline("fci-run-one"), Opts { files, ..opts() }));
        let b = s.spawn(|| go(&pipeline("fci-run-two"), Opts { files, ..opts() }));
        (a.join().unwrap(), b.join().unwrap())
    });
    assert!(a.out.ok() && b.out.ok(), "{}\n{}", a.log, b.log);
    assert!(
        a.log.contains("fci-run-one") && !a.log.contains("fci-run-two"),
        "{}",
        a.log
    );
    assert!(
        b.log.contains("fci-run-two") && !b.log.contains("fci-run-one"),
        "{}",
        b.log
    );
}

// ---- T031 and T039: real Docker, run in CI where an engine exists (as every test above) ----

const RUNNER_VARS: [&str; 4] = [
    "FACTORY_CI_TOKEN",
    "FACTORY_CI_LOG_URL",
    "FACTORY_CI_LOG_USER",
    "FACTORY_CI_LOG_TOKEN",
];

#[test]
fn a_step_sees_none_of_the_runner_process_environment() {
    // AS 7.4, SC-007: the token and the log credentials are in the runner's environment only
    for (k, v) in [
        ("FACTORY_CI_TOKEN", "runner-token-AAA111"),
        ("FACTORY_CI_LOG_URL", "http://loki.invalid-AAA111"),
        ("FACTORY_CI_LOG_USER", "loki-user-AAA111"),
        ("FACTORY_CI_LOG_TOKEN", "loki-pass-AAA111"),
    ] {
        std::env::set_var(k, v);
    }
    let r = go(
        "[steps.dump]\nimage = \"busybox\"\ncommands = [\"env\"]\n",
        Opts {
            secrets: &[],
            ..opts()
        },
    );
    // the other tests of this binary spawn the runner: leave no runner variable behind
    for k in RUNNER_VARS {
        std::env::remove_var(k);
    }
    assert!(r.out.ok(), "{}", r.log);
    assert!(r.log.contains("[dump] FACTORY_CI_SHA=abc123"), "{}", r.log);
    assert!(
        !r.log.contains("AAA111"),
        "a runner value reached a step:\n{}",
        r.log
    );
    assert!(
        !r.log.contains("FACTORY_CI_TOKEN") && !r.log.contains("FACTORY_CI_LOG"),
        "{}",
        r.log
    );
}

/// A `docker` on PATH that behaves like the real one except that the clone container copies
/// `fixture` into the workspace (there is no git server here).
fn docker_with_fixture_clone(dir: &Path, fixture: &Path) -> String {
    let real = Command::new("sh")
        .args(["-c", "command -v docker"])
        .output()
        .unwrap();
    let real = String::from_utf8_lossy(&real.stdout).trim().to_string();
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = format!(
        "#!/bin/sh\ncase \"$*\" in *-clone*)\n  prev=\"\"; for a in \"$@\"; do [ \"$prev\" = \"-v\" ] && ws=\"${{a%%:/workspace}}\"; prev=\"$a\"; done\n  cp -a '{}'/. \"$ws/\"; exit 0;;\nesac\nexec '{real}' \"$@\"\n",
        fixture.display()
    );
    let path = bin.join("docker");
    std::fs::write(&path, script).unwrap();
    let mode = <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755);
    std::fs::set_permissions(&path, mode).unwrap();
    format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

fn started(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            l.strip_prefix('[')?
                .split_once("] started")
                .map(|(n, _)| n.to_string())
        })
        .collect()
}

#[test]
fn local_run_and_job_give_the_same_step_order_and_result() {
    // SC-007, AS 7.4: a chain, so the order is fixed
    pull_once();
    let dir = tmp();
    let fixture = dir.join("checkout");
    std::fs::create_dir_all(&fixture).unwrap();
    let toml = "[steps.a]\nimage = \"busybox\"\ncommands = [\"echo one\"]\n[steps.b]\nimage = \"busybox\"\ncommands = [\"echo two\"]\ndepends_on = [\"a\"]\n[steps.c]\nimage = \"busybox\"\ncommands = [\"echo three\"]\ndepends_on = [\"b\"]\n";
    std::fs::write(fixture.join(".factory-ci.toml"), toml).unwrap();
    let mut local = Command::new(common::BIN);
    local.args(["run", "--dir", fixture.to_str().unwrap()]);
    for k in RUNNER_VARS {
        local.env_remove(k);
    }
    let local = local.output().unwrap();
    assert!(local.status.success(), "{}", common::out_text(&local));

    let api = common::Fake::start();
    let path = docker_with_fixture_clone(&dir, &fixture);
    let mut job = Command::new(common::BIN);
    for k in RUNNER_VARS {
        job.env_remove(k);
    }
    let job = job
        .args(common::job_args(&[]))
        .env("PATH", path)
        .env("TMPDIR", &dir)
        .env("FACTORY_CI_TOKEN", "tok-x")
        .env("GITHUB_API_URL", &api.url)
        .env(
            "FACTORY_CI_CLONE_IMAGE",
            "alpine/git@sha256:0000000000000000000000000000000000000000000000000000000000000000",
        )
        .output()
        .unwrap();
    assert!(job.status.success(), "{}", common::out_text(&job));
    assert_eq!(started(&common::stdout(&local)), ["a", "b", "c"]);
    assert_eq!(
        started(&common::stdout(&job)),
        started(&common::stdout(&local))
    );
    assert_eq!(api.statuses().last().unwrap()["state"], "success");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn local_run_of_a_docker_pipeline_needs_no_opt_in_and_leaves_nothing() {
    // AS 8.4, FR-025
    pull_once();
    let dir = tmp();
    std::fs::write(
        dir.join("Dockerfile"),
        "FROM busybox\nCMD [\"echo\", \"built-ok\"]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join(".factory-ci.toml"),
        "[steps.img]\nimage = \"docker:27.5.1-cli\"\ndocker = true\ncommands = [\"echo RUNID=$FACTORY_CI_RUN_ID\", \"docker build -q -t fci-local-built .\", \"docker run --rm fci-local-built\"]\n",
    )
    .unwrap();
    let o = Command::new(common::BIN)
        .args(["run", "--dir", dir.to_str().unwrap()])
        .output()
        .unwrap();
    let out = common::out_text(&o);
    assert!(o.status.success(), "{out}");
    assert!(out.contains("built-ok"), "{out}");
    let id = out
        .split("RUNID=")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .expect("run id")
        .to_string();
    assert!(leftovers(&id).is_empty(), "{:?}", leftovers(&id));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_real_hello_static_builds_both_platforms_into_archives_that_pass_the_publish_policy() {
    // 012 FR-003, FR-012, FR-028, FR-030, T018: run the committed pipeline of docker/hello-static
    // against the real engine and read what the publish step would read.
    pull_once();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docker/hello-static");
    let dir = tmp();
    for e in std::fs::read_dir(&src).unwrap() {
        let p = e.unwrap().path();
        if p.is_file() {
            std::fs::copy(&p, dir.join(p.file_name().unwrap())).unwrap();
        }
    }
    let o = Command::new(common::BIN)
        .args(["run", "--dir", dir.to_str().unwrap()])
        .output()
        .unwrap();
    let out = common::out_text(&o);
    assert!(o.status.success(), "{out}");
    for (arch, file) in [("amd64", "image-amd64.tar"), ("arm64", "image-arm64.tar")] {
        let c = rust_ci_runner::archive::read_config(&dir.join(file))
            .unwrap_or_else(|e| panic!("{file}: {e}"));
        assert_eq!((c.os.as_str(), c.architecture.as_str()), ("linux", arch));
        assert_eq!(c.user, "65532:65532", "{file}");
        assert_eq!(c.entrypoint, vec!["/hello".to_string()], "{file}");
        assert!(c.cmd.is_empty(), "{file}: {:?}", c.cmd);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
