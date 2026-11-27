//! 008 T005, T006 (test-first). Targets `Engine::command` (made `pub(crate)`) and `store::{fetch, parse_list,
//! Secrets, StoreError}` with a stub `bws` script. FR-009 to FR-012; I8, I9, I11.
//! Not wired yet: T005 adds `#[cfg(test)] mod store_tests;` to lib.rs, T006 adds `pub mod store;`.
//! Assumed shapes (plan.md "Test-first interface notes"): `Secrets { token, log_url, log_user, log_token }`
//! (all `String`, hand-written `Debug` without values); `fetch(&str, &str, Option<&str>, Duration) ->
//! Result<Secrets, StoreError>`; `parse_list(&str) -> Result<Secrets, StoreError>`; `StoreError` is `Debug`
//! and its `Display` is exactly the fault text of plan.md (`secret store unreachable`, `secret store rejected
//! the access token`, `secret <KEY>: expected 1, found <N>`, `secret <KEY>: unusable value`). Garbage output
//! and a refused server URL display as `secret store unreachable`.

use crate::engine::Engine;
use crate::store::{fetch, parse_list};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const KEYS: [&str; 4] = ["FACTORY_CI_TOKEN", "FACTORY_CI_LOG_URL", "FACTORY_CI_LOG_USER", "FACTORY_CI_LOG_TOKEN"];
const GOOD: [(&str, &str); 4] = [
    ("FACTORY_CI_TOKEN", "ghp_clonetoken_AAAA1"),
    ("FACTORY_CI_LOG_URL", "https://logs.example.com"),
    ("FACTORY_CI_LOG_USER", "123456"),
    ("FACTORY_CI_LOG_TOKEN", "glc_logtoken_BBBB2"),
];
const ACCESS: &str = "0.access-token-QQ77.secret";
const SHORT: Duration = Duration::from_millis(300);
const LONG: Duration = Duration::from_secs(10);

fn list(pairs: &[(&str, &str)]) -> String {
    let rows: Vec<_> = pairs
        .iter()
        .map(|(k, v)| serde_json::json!({"id": "i", "organizationId": "o", "projectId": "p", "key": k, "value": v, "note": "", "creationDate": "x", "revisionDate": "x"}))
        .collect();
    serde_json::Value::Array(rows).to_string()
}

struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new() -> Sandbox {
        let dir = std::env::temp_dir().join(format!("fci-store-{}", crate::util::random_id()));
        std::fs::create_dir_all(&dir).unwrap();
        Sandbox { dir }
    }

    /// A `bws` stand-in. It records argv and its environment per call, then runs `body`
    /// (`$n` is the call number, `$d` the sandbox, `$d/out` the prepared stdout).
    fn bws(&self, out: &str, body: &str) -> String {
        std::fs::write(self.dir.join("out"), out).unwrap();
        let script = format!(
            "#!/bin/sh\nd='{}'\nn=$(( $(cat \"$d/count\" 2>/dev/null || echo 0) + 1 )); echo $n > \"$d/count\"\n\
             printf '%s\\n' \"$@\" > \"$d/argv.$n\"\n/usr/bin/env > \"$d/env.$n\"\n{body}\n",
            self.dir.display()
        );
        let p = self.dir.join("bws.sh");
        std::fs::write(&p, script).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p.display().to_string()
    }

    fn ok(&self) -> String {
        self.bws(&list(&GOOD), "cat \"$d/out\"")
    }

    fn calls(&self) -> usize {
        std::fs::read_to_string(self.dir.join("count")).map_or(0, |t| t.trim().parse().unwrap())
    }

    fn argv(&self, n: usize) -> String {
        std::fs::read_to_string(self.dir.join(format!("argv.{n}"))).unwrap_or_default()
    }

    fn env(&self, n: usize) -> String {
        std::fs::read_to_string(self.dir.join(format!("env.{n}"))).unwrap_or_default()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn shown(r: Result<crate::store::Secrets, crate::store::StoreError>) -> String {
    let e = r.err().expect("expected an error");
    format!("{e}")
}

fn names(env: &str) -> Vec<String> {
    env.lines().filter_map(|l| l.split_once('=')).map(|(k, _)| k.to_string()).collect()
}

// ---- engine children (T005): FR-012, I9, I11 ----

#[test]
fn command_removes_the_inherited_token() {
    // `env_remove` acts on the inherited environment: no entry with a value may name the token.
    let e = Engine::new("docker");
    let c = e.command(&["version".to_string()], &[]);
    assert!(!c.get_envs().any(|(k, v)| k == "BWS_ACCESS_TOKEN" && v.is_some()));
}

#[test]
fn explicit_env_entry_still_passes() {
    let e = Engine::new("docker");
    let c = e.command(&[], &[("BWS_ACCESS_TOKEN".to_string(), "explicit".to_string()), ("FCI_TOKEN".to_string(), "t".to_string())]);
    let got: Vec<_> = c.get_envs().filter_map(|(k, v)| v.map(|v| (k.to_string_lossy().to_string(), v.to_string_lossy().to_string()))).collect();
    assert!(got.contains(&("BWS_ACCESS_TOKEN".to_string(), "explicit".to_string())), "{got:?}");
    assert!(got.contains(&("FCI_TOKEN".to_string(), "t".to_string())), "{got:?}");
}

#[test]
fn engine_child_env_is_the_allowlist() {
    // A stub engine dumps its environment. cargo sets CARGO_PKG_NAME for every test process.
    let sb = Sandbox::new();
    let stub = sb.bws("", "exit 0");
    let e = Engine::new(&stub);
    let args = vec!["ps".to_string()];
    e.capture(&args, &[("EXPLICIT_ONE".to_string(), "yes".to_string())], LONG, None).unwrap();
    let env = sb.env(1);
    assert!(env.lines().any(|l| l == "EXPLICIT_ONE=yes"), "{env}");
    assert!(env.lines().any(|l| l.starts_with("PATH=")), "{env}");
    assert!(!env.contains("CARGO_PKG_NAME"), "{env}");
    assert!(!env.contains("BWS_ACCESS_TOKEN"), "{env}");
    let allowed = [
        "PATH", "HOME", "DOCKER_HOST", "DOCKER_CONFIG", "DOCKER_CERT_PATH", "DOCKER_TLS_VERIFY", "XDG_RUNTIME_DIR", "EXPLICIT_ONE",
        // set by sh itself
        "PWD", "OLDPWD", "SHLVL", "_",
    ];
    for n in names(&env) {
        assert!(allowed.contains(&n.as_str()), "{n} is not on the allowlist");
    }
}

// ---- parse_list (T006): FR-010, I8 ----

#[test]
#[ignore = "test-first"]
fn one_secret_per_key_is_returned() {
    let mut rows = GOOD.to_vec();
    rows.push(("SOMETHING_ELSE", "ignored-value"));
    rows.push(("FACTORY_CI_LOG_URL_OLD", "https://old.example.com"));
    let s = parse_list(&list(&rows)).ok().expect("a valid list");
    assert_eq!(s.token, GOOD[0].1);
    assert_eq!(s.log_url, GOOD[1].1);
    assert_eq!(s.log_user, GOOD[2].1);
    assert_eq!(s.log_token, GOOD[3].1);
    // fetch passes the same list through and calls `bws secret list -o json` with no project argument
    let sb = Sandbox::new();
    let got = fetch(&sb.ok(), ACCESS, None, LONG).ok().expect("fetch");
    assert_eq!(got.token, GOOD[0].1);
    assert_eq!(sb.argv(1), "secret\nlist\n-o\njson\n");
}

#[test]
#[ignore = "test-first"]
fn zero_or_two_matches_name_key_and_count_only() {
    let without = |k: &str| GOOD.iter().filter(|(n, _)| *n != k).cloned().collect::<Vec<_>>();
    for k in KEYS {
        let e = shown(parse_list(&list(&without(k))));
        assert_eq!(e, format!("secret {k}: expected 1, found 0"));
        let mut twice = GOOD.to_vec();
        twice.push((k, "second-value-ZZZ9"));
        let e = shown(parse_list(&list(&twice)));
        assert_eq!(e, format!("secret {k}: expected 1, found 2"));
        assert!(!e.contains("second-value-ZZZ9") && !e.contains("logs.example.com"));
    }
    // a near match (case, spaces) does not count
    let mut near = without("FACTORY_CI_LOG_USER");
    near.push(("factory_ci_log_user", "123456"));
    assert_eq!(shown(parse_list(&list(&near))), "secret FACTORY_CI_LOG_USER: expected 1, found 0");
}

#[test]
#[ignore = "test-first"]
fn value_shapes_are_refused() {
    for k in KEYS {
        for bad in ["", "abc", "a\nb", "a\rb", "a\0b", " lead-value", "trail-value ", "\tvalue-tab", "line\n"] {
            let rows: Vec<(&str, &str)> = GOOD.iter().map(|(n, v)| if *n == k { (*n, bad) } else { (*n, *v) }).collect();
            let e = shown(parse_list(&list(&rows)));
            assert_eq!(e, format!("secret {k}: unusable value"), "{bad:?}");
        }
        let rows: Vec<(&str, &str)> = GOOD.iter().map(|(n, v)| if *n == k { (*n, "abcd") } else { (*n, *v) }).collect();
        assert!(parse_list(&list(&rows)).is_ok(), "4 characters is enough for {k}");
    }
}

#[test]
#[ignore = "test-first"]
fn matches_other_than_one_never_return_a_value() {
    // I8: every key count in 0..=3, in random order. Ok only when all four counts are 1.
    let mut x: u64 = 17;
    let mut next = |m: u64| {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (x >> 33) % m
    };
    for _ in 0..300 {
        let counts: Vec<u64> = KEYS.iter().map(|_| next(4)).collect();
        let mut rows: Vec<(&str, &str)> = vec![];
        for (k, c) in KEYS.iter().zip(&counts) {
            for _ in 0..*c {
                rows.push((*k, "value-1234"));
            }
        }
        rows.push(("NOISE", "value-1234"));
        let r = parse_list(&list(&rows));
        if counts.iter().all(|c| *c == 1) {
            assert!(r.is_ok(), "{counts:?}");
        } else {
            let e = r.err().expect("an error");
            let text = format!("{e} {e:?}");
            assert!(!text.contains("value-1234"), "{text}");
            let bad = KEYS.iter().zip(&counts).find(|(_, c)| **c != 1).unwrap();
            assert!(format!("{e}").starts_with(&format!("secret {}: expected 1, found {}", bad.0, bad.1)), "{counts:?} {e}");
        }
    }
}

#[test]
#[ignore = "test-first"]
fn secrets_debug_prints_no_value() {
    let s = parse_list(&list(&GOOD)).ok().unwrap();
    let d = format!("{s:?}");
    for (_, v) in GOOD {
        assert!(!d.contains(v), "{d}");
    }
}

// ---- fetch (T006): FR-009 ----

#[test]
#[ignore = "test-first"]
fn token_goes_in_env_not_argv() {
    let sb = Sandbox::new();
    fetch(&sb.ok(), ACCESS, Some("https://vault.bitwarden.eu"), LONG).ok().expect("fetch");
    assert!(!sb.argv(1).contains(ACCESS), "token in argv");
    assert!(sb.env(1).lines().any(|l| l == format!("BWS_ACCESS_TOKEN={ACCESS}")), "{}", sb.env(1));
}

#[test]
#[ignore = "test-first"]
fn bws_child_env_is_the_allowlist() {
    let sb = Sandbox::new();
    fetch(&sb.ok(), ACCESS, None, LONG).ok().expect("fetch");
    let env = sb.env(1);
    assert!(!env.contains("CARGO_PKG_NAME") && !env.contains("BWS_SERVER_URL"), "{env}");
    assert!(env.lines().any(|l| l.starts_with("PATH=")), "{env}");
    let allowed = ["PATH", "HOME", "BWS_ACCESS_TOKEN", "BWS_SERVER_URL", "PWD", "OLDPWD", "SHLVL", "_"];
    for n in names(&env) {
        assert!(allowed.contains(&n.as_str()), "{n} is not on the allowlist");
    }
    // with a server URL the child gets BWS_SERVER_URL, still nothing else
    let sb = Sandbox::new();
    fetch(&sb.ok(), ACCESS, Some("https://vault.bitwarden.com"), LONG).ok().expect("fetch");
    let env = sb.env(1);
    assert!(env.lines().any(|l| l == "BWS_SERVER_URL=https://vault.bitwarden.com"), "{env}");
    for n in names(&env) {
        assert!(allowed.contains(&n.as_str()), "{n} is not on the allowlist");
    }
}

#[test]
#[ignore = "test-first"]
fn rejected_token_is_not_retried() {
    for msg in [
        "Error: 400 Bad Request",
        "Error: 401 Unauthorized",
        "Error: 403 Forbidden",
        "invalid_client",
        "unauthorized",
        "forbidden",
        "Error: invalid access token",
    ] {
        let sb = Sandbox::new();
        let bws = sb.bws("", &format!("echo '{msg}' >&2; exit 1"));
        let e = shown(fetch(&bws, ACCESS, None, LONG));
        assert_eq!(e, "secret store rejected the access token", "{msg}");
        assert_eq!(sb.calls(), 1, "{msg}: retried");
    }
}

#[test]
#[ignore = "test-first"]
fn unreachable_is_retried_once() {
    let sb = Sandbox::new();
    let bws = sb.bws("", "echo 'Error: connection refused' >&2; exit 1");
    assert_eq!(shown(fetch(&bws, ACCESS, None, LONG)), "secret store unreachable");
    assert_eq!(sb.calls(), 2);
    // the second attempt can succeed
    let sb = Sandbox::new();
    let bws = sb.bws(&list(&GOOD), "[ \"$n\" = 1 ] && { echo 'Error: timed out' >&2; exit 1; }\ncat \"$d/out\"");
    assert!(fetch(&bws, ACCESS, None, LONG).is_ok());
    assert_eq!(sb.calls(), 2);
    // a missing binary is unreachable too, not a panic
    let e = shown(fetch("/nonexistent/bws", ACCESS, None, LONG));
    assert_eq!(e, "secret store unreachable");
}

#[test]
#[ignore = "test-first"]
fn slow_bws_times_out_within_the_limit() {
    let sb = Sandbox::new();
    // `exec`: the sleep is the child, so the kill at the limit also closes the pipes
    let bws = sb.bws("", "exec sleep 5");
    let start = Instant::now();
    assert_eq!(shown(fetch(&bws, ACCESS, None, SHORT)), "secret store unreachable");
    assert!(start.elapsed() < Duration::from_secs(4), "{:?}", start.elapsed());
    assert_eq!(sb.calls(), 2, "one retry after a timeout");
}

#[test]
#[ignore = "test-first"]
fn server_url_must_be_https_on_an_allowed_host() {
    for ok in ["https://vault.bitwarden.com", "https://vault.bitwarden.eu"] {
        let sb = Sandbox::new();
        assert!(fetch(&sb.ok(), ACCESS, Some(ok), LONG).is_ok(), "{ok}");
        assert!(sb.env(1).lines().any(|l| l == format!("BWS_SERVER_URL={ok}")), "{ok}");
    }
    for bad in [
        "http://vault.bitwarden.com",
        "https://vault.bitwarden.com.evil.example",
        "https://evil.example",
        "https://vault.bitwarden.com@evil.example",
        "https://evil.example/vault.bitwarden.com",
        "https://vault.bitwarden.comx",
        "https://evil.example#@vault.bitwarden.com",
        "vault.bitwarden.com",
        "ftp://vault.bitwarden.com",
    ] {
        let sb = Sandbox::new();
        assert_eq!(shown(fetch(&sb.ok(), ACCESS, Some(bad), LONG)), "secret store unreachable", "{bad}");
        assert_eq!(sb.calls(), 0, "{bad}: bws was called");
    }
}

#[test]
#[ignore = "test-first"]
fn garbage_output_is_unreadable_and_not_echoed() {
    let cases: [(&str, &str); 5] = [
        ("not json LEAK-OUT-77", "exit 0"),
        ("{\"key\":\"FACTORY_CI_TOKEN\"}", "exit 0"),
        ("[{\"nokey\": 1}]", "exit 0"),
        ("", "exit 0"),
        ("LEAK-OUT-77", "echo 'LEAK-ERR-88 ghp_leak' >&2; exit 1"),
    ];
    for (out, tail) in cases {
        let sb = Sandbox::new();
        let bws = sb.bws(out, &format!("cat \"$d/out\"\n{tail}"));
        let r = fetch(&bws, ACCESS, None, LONG);
        let e = r.err().expect("an error");
        let text = format!("{e} {e:?}");
        assert!(!text.contains("LEAK-") && !text.contains("ghp_leak"), "{text}");
        assert_eq!(format!("{e}"), "secret store unreachable", "{out:?}");
    }
}
