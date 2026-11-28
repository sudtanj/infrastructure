//! 012 T009 (plan step S4). Targets `Engine::capture_in(args, env, input, limit, container)` and
//! `Engine::run_left(run_id)` with a stub engine script (FR-015: the token reaches a child on standard input only).
//!
//! `input` is passed as `&[u8]` through `feed`; change that one helper if the signature takes `&str`.
//! The tests of `capture` pin it as it behaves today and must stay green.

use crate::engine::Engine;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const TOKEN: &str = "fake-token-AAAA-0123456789";

struct Dir(PathBuf);

impl Dir {
    fn new() -> Dir {
        let p = std::env::temp_dir().join(format!("fci-engine-{}", crate::util::random_id()));
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    fn p(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn put(&self, name: &str, text: &str) {
        std::fs::write(self.p(name), text).unwrap();
    }

    fn get(&self, name: &str) -> String {
        std::fs::read_to_string(self.p(name)).unwrap_or_default()
    }

    /// A stub engine: `body` runs under `/bin/sh`; `$D` is this directory.
    fn engine(&self, body: &str) -> Engine {
        let script = format!("#!/bin/sh\nD='{}'\n{body}\n", self.0.display());
        let bin = self.p("engine");
        std::fs::write(&bin, script).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        Engine::new(bin.to_str().unwrap())
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn feed(
    e: &Engine,
    a: &[&str],
    env: &[(String, String)],
    input: &str,
    limit: Duration,
    container: Option<&str>,
) -> Result<String, String> {
    e.capture_in(&args(a), env, input.as_bytes(), limit, container)
}

const SEC: Duration = Duration::from_secs(20);

// ---- capture_in ----

#[test]
fn capture_in_delivers_the_input_bytes_to_standard_input() {
    // 012 T009; S4 "writes `input` to the child's standard input ... and closes it, returns stdout"
    let d = Dir::new();
    let e = d.engine("cat > \"$D/in\"\necho ok");
    let input = format!("{TOKEN}\nsecond line\r\n\0tail é");
    let out = feed(&e, &["login"], &[], &input, SEC, None).unwrap();
    assert_eq!(out, "ok\n");
    assert_eq!(std::fs::read(d.p("in")).unwrap(), input.as_bytes());
}

#[test]
fn capture_in_closes_standard_input_after_the_write() {
    // 012 T009: `cat` only ends at EOF; a pipe left open would hit the limit
    let d = Dir::new();
    let e = d.engine("cat > \"$D/in\"");
    let t = Instant::now();
    feed(&e, &["x"], &[], "abc", SEC, None).unwrap();
    assert!(
        t.elapsed() < Duration::from_secs(10),
        "stdin was not closed"
    );
    feed(&e, &["x"], &[], "", SEC, None).unwrap();
    assert_eq!(d.get("in"), "", "an empty input is an immediate EOF");
}

#[test]
fn the_input_is_in_no_argv_and_no_environment() {
    // 012 T009; S4 "argv and the client env hold none of them"; FR-015
    let d = Dir::new();
    let e = d.engine("printf '%s\\n' \"$@\" > \"$D/argv\"\nenv > \"$D/env\"\ncat > \"$D/in\"");
    let env = [("EXTRA".to_string(), "visible".to_string())];
    feed(&e, &["push", "--flag", "x"], &env, TOKEN, SEC, None).unwrap();
    assert_eq!(d.get("in"), TOKEN);
    assert!(!d.get("argv").contains(TOKEN), "argv: {}", d.get("argv"));
    assert!(!d.get("env").contains(TOKEN), "env holds the input");
    assert!(d.get("argv").contains("--flag"), "argv is passed as given");
    assert!(
        d.get("env").contains("EXTRA=visible"),
        "explicit env entries pass"
    );
}

#[test]
fn capture_in_keeps_the_env_clearing_of_command() {
    // 012 T009; "keeps the env-clearing of `Engine::command`"
    std::env::set_var("FCI_QA_LEAK_PROBE", "leak-value");
    let d = Dir::new();
    let e = d.engine("env > \"$D/env\"\ncat > /dev/null");
    feed(&e, &["x"], &[], "in", SEC, None).unwrap();
    let env = d.get("env");
    assert!(
        !env.contains("FCI_QA_LEAK_PROBE") && !env.contains("leak-value"),
        "{env}"
    );
    let allowed = [
        "PATH",
        "HOME",
        "DOCKER_HOST",
        "DOCKER_CONFIG",
        "DOCKER_CERT_PATH",
        "DOCKER_TLS_VERIFY",
        "XDG_RUNTIME_DIR",
        "PWD",
        "OLDPWD",
        "SHLVL",
        "_",
        "D",
    ];
    for line in env.lines() {
        let key = line.split('=').next().unwrap_or("");
        assert!(
            allowed.contains(&key),
            "unexpected variable in the child: {line}"
        );
    }
}

#[test]
fn large_input_and_large_output_do_not_deadlock() {
    // 012 T009: the child fills its output pipe before it reads; a write on the calling thread would block forever
    let d = Dir::new();
    let e = d.engine("head -c 300000 /dev/zero | tr '\\0' x\ncat > \"$D/in\"");
    let input = "y".repeat(1 << 20);
    let out = feed(&e, &["x"], &[], &input, SEC, None).unwrap();
    assert_eq!(out.len(), 300_000);
    assert_eq!(d.get("in").len(), 1 << 20);
}

#[test]
fn a_child_that_never_reads_standard_input_does_not_panic_or_hang() {
    // 012 T009: the write fails with a broken pipe; the exit status decides the result
    let d = Dir::new();
    let e = d.engine("echo done");
    let input = "z".repeat(1 << 20);
    let t = Instant::now();
    assert_eq!(feed(&e, &["x"], &[], &input, SEC, None).unwrap(), "done\n");
    assert!(t.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_nonzero_exit_is_reported_as_exit_status_n_with_stderr() {
    // 012 T009; S4 "reports a non-zero exit as `exit status: <n>`"; publish maps 41, 43, 45 on this text
    let d = Dir::new();
    let e = d.engine("cat > /dev/null\necho 'boom happened' >&2\nexit 41");
    let err = feed(&e, &["x"], &[], TOKEN, SEC, None).unwrap_err();
    assert!(err.contains("exit status: 41"), "{err}");
    assert!(err.contains("boom happened"), "{err}");
    assert!(!err.contains(TOKEN), "{err}");
    for n in [1, 43, 45, 255] {
        let e = d.engine(&format!("cat > /dev/null\nexit {n}"));
        let err = feed(&e, &["x"], &[], "", SEC, None).unwrap_err();
        assert!(err.contains(&format!("exit status: {n}")), "{n}: {err}");
    }
}

#[test]
fn a_timeout_is_reported_and_the_container_is_killed_and_removed() {
    // 012 T009; S4 "kills the container on timeout"
    let d = Dir::new();
    let e = d.engine(
        "echo \"$*\" >> \"$D/log\"\ncase \"$1\" in run) cat > /dev/null; exec sleep 30;; esac",
    );
    let t = Instant::now();
    let err = feed(
        &e,
        &["run", "--name", "pub-a"],
        &[],
        TOKEN,
        Duration::from_secs(1),
        Some("pub-a"),
    )
    .unwrap_err();
    assert!(err.contains("timed out"), "{err}");
    assert!(
        t.elapsed() < Duration::from_secs(15),
        "took {:?}",
        t.elapsed()
    );
    let log = d.get("log");
    assert!(log.lines().any(|l| l == "kill pub-a"), "{log}");
    assert!(log.lines().any(|l| l == "rm -f pub-a"), "{log}");
}

#[test]
fn a_timeout_without_a_container_name_kills_only_the_child() {
    // 012 T009: `container` None means no kill call
    let d = Dir::new();
    let e = d.engine("echo \"$*\" >> \"$D/log\"\ncat > /dev/null; exec sleep 30");
    let err = feed(&e, &["run"], &[], "", Duration::from_secs(1), None).unwrap_err();
    assert!(err.contains("timed out"), "{err}");
    assert_eq!(d.get("log").lines().count(), 1, "{}", d.get("log"));
}

#[test]
fn a_missing_engine_binary_is_the_spawn_failure_text() {
    // 012 T009: same fixed prefix as `capture`
    let e = Engine::new("/nonexistent/engine-bin");
    let err = feed(&e, &["x"], &[], TOKEN, SEC, None).unwrap_err();
    assert!(err.starts_with(crate::engine::SPAWN_FAIL), "{err}");
    assert!(!err.contains(TOKEN), "{err}");
}

#[test]
fn the_engine_source_still_has_one_command_new_and_never_formats_the_input() {
    // 012 T009; S4 constraints "no `Command::new` outside `command()`; the input never reaches a `format!`"
    let src = include_str!("engine.rs");
    let prod = src.split("#[cfg(test)]\nmod tests").next().unwrap();
    assert_eq!(
        prod.matches("Command::new").count(),
        1,
        "only Engine::command spawns"
    );
    if let Some(start) = prod.find("fn capture_in") {
        let body = &prod[start..];
        let end = body[1..]
            .find("\n    pub fn ")
            .map_or(body.len(), |i| i + 1);
        for line in body[..end].lines().filter(|l| l.contains("format!")) {
            assert!(!line.contains("input"), "input in a format!: {line}");
        }
    }
}

#[test]
fn a_limit_too_big_for_an_instant_does_not_panic() {
    // 012 T016; review of S5a: `Instant::now() + limit` panics on `Duration::MAX`
    let d = Dir::new();
    let e = d.engine("cat > /dev/null\necho ok");
    assert_eq!(
        feed(&e, &["x"], &[], "", Duration::MAX, None),
        Ok("ok\n".to_string())
    );
}

// ---- capture, unchanged ----

#[test]
fn capture_reads_stdout_and_gives_the_child_no_input() {
    // 012 T009; "`capture` behaves as before"
    let d = Dir::new();
    let e = d.engine("cat\necho done");
    let out = e.capture(&args(&["x"]), &[], SEC, None).unwrap();
    assert_eq!(out, "done\n");
}

#[test]
fn capture_reports_a_failure_as_status_and_trimmed_stderr() {
    // 012 T009: the exact `exit status: <n>: <stderr>` shape that `capture_in` must keep
    let d = Dir::new();
    let e = d.engine("echo '  oops  ' >&2\nexit 3");
    let err = e.capture(&args(&["x"]), &[], SEC, None).unwrap_err();
    assert_eq!(err, "exit status: 3: oops");
}

#[test]
fn capture_times_out_with_the_engine_name() {
    // 012 T009
    let d = Dir::new();
    let e = d.engine("exec sleep 30");
    let err = e
        .capture(&args(&["x"]), &[], Duration::from_secs(1), None)
        .unwrap_err();
    assert!(err.ends_with("timed out"), "{err}");
}

// ---- run_left ----

/// Stub: `ps`, `network` and `volume` list the ids in `D/ps`, `D/net`, `D/vol` only when asked for the label
/// `factory-ci.run=run-a`; every call is logged.
fn lister(d: &Dir) -> Engine {
    d.engine(
        "echo \"$*\" >> \"$D/log\"\nfor a in \"$@\"; do\n  if [ \"$a\" = \"label=factory-ci.run=run-a\" ]; then\n    case \"$1\" in\n      ps) cat \"$D/ps\" 2>/dev/null;;\n      network) cat \"$D/net\" 2>/dev/null;;\n      volume) cat \"$D/vol\" 2>/dev/null;;\n    esac\n  fi\ndone\nexit 0",
    )
}

#[test]
fn run_left_is_false_when_nothing_carries_the_label() {
    // 012 T009; S4 "`run_left(run_id)` is true while any container, network or volume with the label exists"
    let d = Dir::new();
    assert!(!lister(&d).run_left("run-a"));
}

#[test]
fn run_left_is_true_for_a_container_a_network_or_a_volume() {
    // 012 T009
    for (file, text) in [("ps", "abc123\n"), ("net", "net987\n"), ("vol", "vol555\n")] {
        let d = Dir::new();
        let e = lister(&d);
        assert!(!e.run_left("run-a"), "{file}: before");
        d.put(file, text);
        assert!(e.run_left("run-a"), "{file}: one resource is enough");
        std::fs::remove_file(d.p(file)).unwrap();
        assert!(!e.run_left("run-a"), "{file}: gone again");
    }
}

#[test]
fn run_left_matches_the_exact_run_id() {
    // 012 T009: another run's resources, and an id that only shares a prefix, do not count
    let d = Dir::new();
    d.put("ps", "abc123\n");
    d.put("net", "n1\n");
    d.put("vol", "v1\n");
    let e = lister(&d);
    assert!(e.run_left("run-a"));
    assert!(!e.run_left("run-b"));
    assert!(!e.run_left("run-ab"));
    assert!(!e.run_left("run-"));
}

#[test]
fn run_left_asks_each_kind_with_the_label_filter_and_includes_stopped_containers() {
    // 012 T009: a stopped container still holds the label, so `ps` must list all
    let d = Dir::new();
    let e = lister(&d);
    assert!(!e.run_left("run-a"));
    let log = d.get("log");
    for kind in ["ps", "network", "volume"] {
        let line = log
            .lines()
            .find(|l| l.split(' ').next() == Some(kind))
            .unwrap_or_else(|| panic!("no `{kind}` call: {log}"));
        assert!(line.contains("label=factory-ci.run=run-a"), "{line}");
    }
    let ps = log.lines().find(|l| l.starts_with("ps")).unwrap();
    assert!(
        ps.split(' ')
            .any(|a| a == "--all" || (a.starts_with("-a") && !a.starts_with("--"))),
        "ps without -a misses stopped containers: {ps}"
    );
}

#[test]
fn run_left_only_reads() {
    // 012 T009: it never removes, kills or stops anything
    let d = Dir::new();
    d.put("ps", "abc123\n");
    let e = lister(&d);
    assert!(e.run_left("run-a"));
    for line in d.get("log").lines() {
        let words: Vec<&str> = line.split(' ').collect();
        let bad = ["rm", "kill", "stop", "prune"];
        assert!(
            !words.iter().any(|w| bad.contains(w)),
            "mutating call: {line}"
        );
    }
}
