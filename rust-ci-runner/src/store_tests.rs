//! 008 T005, T006 (test-first). Targets `Engine::command` (made `pub(crate)`) and `store::{fetch, parse_list,
//! Secrets, StoreError}` with a stub `bws` script. FR-009 to FR-012; I8, I9, I11.
//! Not wired yet: T005 adds `#[cfg(test)] mod store_tests;` to lib.rs, T006 adds `pub mod store;`.
//! Assumed shapes (plan.md "Test-first interface notes"): `Secrets { token, log_url, log_user, log_token }`
//! (all `String`, hand-written `Debug` without values); `fetch(&str, &str, Option<&str>, Duration) ->
//! Result<Secrets, StoreError>`; `parse_list(&str) -> Result<Secrets, StoreError>`; `StoreError` is `Debug`
//! and its `Display` is exactly the fault text of plan.md (`secret store unreachable`, `secret store rejected
//! the access token`, `secret <KEY>: expected 1, found <N>`, `secret <KEY>: unusable value`). Garbage output
//! and a refused server URL display as `secret store unreachable`.
//!
//! 010 T001 (test-first, each new test is `#[ignore = "test-first"]`): the keyed lookup. Planned shapes
//! (plan.md "Lookup interface v1", data-model.md): `parse_list_with(&str, &[String]) -> Result<Fetched,
//! StoreError>`, `fetch_with(&str, &str, Option<&str>, Duration, &[String]) -> Result<Fetched, StoreError>`,
//! `Fetched { secrets: Secrets, held: Held }`, `Held::take(&self, &[String]) -> Result<StepSecrets, NamedError>`,
//! `StepSecrets(pub(crate) BTreeMap<String, String>)` (`Debug` prints names only), `NamedError::{Count {
//! index, found }, Unusable { index }}` (`Debug` and `Display`, no `PartialEq` needed), `REQUIRED_KEYS`.
//! FR-001, FR-003, FR-005 to FR-007, FR-018.

use crate::engine::Engine;
use crate::store::{fetch, fetch_with, parse_list, parse_list_with, Held, NamedError};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const KEYS: [&str; 4] = [
    "FACTORY_CI_TOKEN",
    "FACTORY_CI_LOG_URL",
    "FACTORY_CI_LOG_USER",
    "FACTORY_CI_LOG_TOKEN",
];
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
    env.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, _)| k.to_string())
        .collect()
}

// ---- engine children (T005): FR-012, I9, I11 ----

#[test]
fn command_removes_the_inherited_token() {
    // `env_remove` acts on the inherited environment: no entry with a value may name the token.
    let e = Engine::new("docker");
    let c = e.command(&["version".to_string()], &[]);
    assert!(!c
        .get_envs()
        .any(|(k, v)| k == "BWS_ACCESS_TOKEN" && v.is_some()));
}

#[test]
fn explicit_env_entry_still_passes() {
    let e = Engine::new("docker");
    let c = e.command(
        &[],
        &[
            ("BWS_ACCESS_TOKEN".to_string(), "explicit".to_string()),
            ("FCI_TOKEN".to_string(), "t".to_string()),
        ],
    );
    let got: Vec<_> = c
        .get_envs()
        .filter_map(|(k, v)| {
            v.map(|v| {
                (
                    k.to_string_lossy().to_string(),
                    v.to_string_lossy().to_string(),
                )
            })
        })
        .collect();
    assert!(
        got.contains(&("BWS_ACCESS_TOKEN".to_string(), "explicit".to_string())),
        "{got:?}"
    );
    assert!(
        got.contains(&("FCI_TOKEN".to_string(), "t".to_string())),
        "{got:?}"
    );
}

#[test]
fn engine_child_env_is_the_allowlist() {
    // A stub engine dumps its environment. cargo sets CARGO_PKG_NAME for every test process.
    let sb = Sandbox::new();
    let stub = sb.bws("", "exit 0");
    let e = Engine::new(&stub);
    let args = vec!["ps".to_string()];
    e.capture(
        &args,
        &[("EXPLICIT_ONE".to_string(), "yes".to_string())],
        LONG,
        None,
    )
    .unwrap();
    let env = sb.env(1);
    assert!(env.lines().any(|l| l == "EXPLICIT_ONE=yes"), "{env}");
    assert!(env.lines().any(|l| l.starts_with("PATH=")), "{env}");
    assert!(!env.contains("CARGO_PKG_NAME"), "{env}");
    assert!(!env.contains("BWS_ACCESS_TOKEN"), "{env}");
    let allowed = [
        "PATH",
        "HOME",
        "DOCKER_HOST",
        "DOCKER_CONFIG",
        "DOCKER_CERT_PATH",
        "DOCKER_TLS_VERIFY",
        "XDG_RUNTIME_DIR",
        "EXPLICIT_ONE",
        // set by sh itself
        "PWD",
        "OLDPWD",
        "SHLVL",
        "_",
    ];
    for n in names(&env) {
        assert!(allowed.contains(&n.as_str()), "{n} is not on the allowlist");
    }
}

// ---- parse_list (T006): FR-010, I8 ----

#[test]
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
fn zero_or_two_matches_name_key_and_count_only() {
    let without = |k: &str| {
        GOOD.iter()
            .filter(|(n, _)| *n != k)
            .cloned()
            .collect::<Vec<_>>()
    };
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
    assert_eq!(
        shown(parse_list(&list(&near))),
        "secret FACTORY_CI_LOG_USER: expected 1, found 0"
    );
}

#[test]
fn value_shapes_are_refused() {
    for k in KEYS {
        for bad in [
            "",
            "abc",
            "a\nb",
            "a\rb",
            "a\0b",
            " lead-value",
            "trail-value ",
            "\tvalue-tab",
            "line\n",
        ] {
            let rows: Vec<(&str, &str)> = GOOD
                .iter()
                .map(|(n, v)| if *n == k { (*n, bad) } else { (*n, *v) })
                .collect();
            let e = shown(parse_list(&list(&rows)));
            assert_eq!(e, format!("secret {k}: unusable value"), "{bad:?}");
        }
        let rows: Vec<(&str, &str)> = GOOD
            .iter()
            .map(|(n, v)| if *n == k { (*n, "abcd") } else { (*n, *v) })
            .collect();
        assert!(
            parse_list(&list(&rows)).is_ok(),
            "4 characters is enough for {k}"
        );
    }
}

#[test]
fn matches_other_than_one_never_return_a_value() {
    // I8: every key count in 0..=3, in random order. Ok only when all four counts are 1.
    let mut x: u64 = 17;
    let mut next = |m: u64| {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
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
            assert!(
                format!("{e}")
                    .starts_with(&format!("secret {}: expected 1, found {}", bad.0, bad.1)),
                "{counts:?} {e}"
            );
        }
    }
}

#[test]
fn secrets_debug_prints_no_value() {
    let s = parse_list(&list(&GOOD)).ok().unwrap();
    let d = format!("{s:?}");
    for (_, v) in GOOD {
        assert!(!d.contains(v), "{d}");
    }
}

// ---- fetch (T006): FR-009 ----

#[test]
fn token_goes_in_env_not_argv() {
    let sb = Sandbox::new();
    fetch(&sb.ok(), ACCESS, Some("https://vault.bitwarden.eu"), LONG)
        .ok()
        .expect("fetch");
    assert!(!sb.argv(1).contains(ACCESS), "token in argv");
    assert!(
        sb.env(1)
            .lines()
            .any(|l| l == format!("BWS_ACCESS_TOKEN={ACCESS}")),
        "{}",
        sb.env(1)
    );
}

#[test]
fn bws_child_env_is_the_allowlist() {
    let sb = Sandbox::new();
    fetch(&sb.ok(), ACCESS, None, LONG).ok().expect("fetch");
    let env = sb.env(1);
    assert!(
        !env.contains("CARGO_PKG_NAME") && !env.contains("BWS_SERVER_URL"),
        "{env}"
    );
    assert!(env.lines().any(|l| l.starts_with("PATH=")), "{env}");
    let allowed = [
        "PATH",
        "HOME",
        "BWS_ACCESS_TOKEN",
        "BWS_SERVER_URL",
        "PWD",
        "OLDPWD",
        "SHLVL",
        "_",
    ];
    for n in names(&env) {
        assert!(allowed.contains(&n.as_str()), "{n} is not on the allowlist");
    }
    // with a server URL the child gets BWS_SERVER_URL, still nothing else
    let sb = Sandbox::new();
    fetch(&sb.ok(), ACCESS, Some("https://vault.bitwarden.com"), LONG)
        .ok()
        .expect("fetch");
    let env = sb.env(1);
    assert!(
        env.lines()
            .any(|l| l == "BWS_SERVER_URL=https://vault.bitwarden.com"),
        "{env}"
    );
    for n in names(&env) {
        assert!(allowed.contains(&n.as_str()), "{n} is not on the allowlist");
    }
}

#[test]
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
fn unreachable_is_retried_once() {
    let sb = Sandbox::new();
    let bws = sb.bws("", "echo 'Error: connection refused' >&2; exit 1");
    assert_eq!(
        shown(fetch(&bws, ACCESS, None, LONG)),
        "secret store unreachable"
    );
    assert_eq!(sb.calls(), 2);
    // the second attempt can succeed
    let sb = Sandbox::new();
    let bws = sb.bws(
        &list(&GOOD),
        "[ \"$n\" = 1 ] && { echo 'Error: timed out' >&2; exit 1; }\ncat \"$d/out\"",
    );
    assert!(fetch(&bws, ACCESS, None, LONG).is_ok());
    assert_eq!(sb.calls(), 2);
    // a missing binary is unreachable too, not a panic
    let e = shown(fetch("/nonexistent/bws", ACCESS, None, LONG));
    assert_eq!(e, "secret store unreachable");
}

#[test]
fn slow_bws_times_out_within_the_limit() {
    let sb = Sandbox::new();
    // `exec`: the sleep is the child, so the kill at the limit also closes the pipes
    let bws = sb.bws("", "exec sleep 5");
    let start = Instant::now();
    assert_eq!(
        shown(fetch(&bws, ACCESS, None, SHORT)),
        "secret store unreachable"
    );
    assert!(
        start.elapsed() < Duration::from_secs(4),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(sb.calls(), 2, "one retry after a timeout");
}

#[test]
fn server_url_must_be_https_on_an_allowed_host() {
    for ok in ["https://vault.bitwarden.com", "https://vault.bitwarden.eu"] {
        let sb = Sandbox::new();
        assert!(fetch(&sb.ok(), ACCESS, Some(ok), LONG).is_ok(), "{ok}");
        assert!(
            sb.env(1)
                .lines()
                .any(|l| l == format!("BWS_SERVER_URL={ok}")),
            "{ok}"
        );
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
        assert_eq!(
            shown(fetch(&sb.ok(), ACCESS, Some(bad), LONG)),
            "secret store unreachable",
            "{bad}"
        );
        assert_eq!(sb.calls(), 0, "{bad}: bws was called");
    }
}

#[test]
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
        assert!(
            !text.contains("LEAK-") && !text.contains("ghp_leak"),
            "{text}"
        );
        assert_eq!(format!("{e}"), "secret store unreachable", "{out:?}");
    }
}

// ---- run_capped (R02): a grandchild that holds the pipe cannot block the return ----

#[test]
fn a_backgrounded_child_holding_stdout_does_not_block_run_capped() {
    use crate::engine::run_capped;
    let sb = Sandbox::new();
    // the shell exits at once; the `sleep` it leaves behind keeps stdout open
    let exits = sb.bws("", "sleep 20 &\necho done");
    let start = Instant::now();
    let r = run_capped(std::process::Command::new(&exits), LONG);
    assert!(r.is_some_and(|(ok, _, _)| ok));
    assert!(
        start.elapsed() < Duration::from_secs(8),
        "{:?}",
        start.elapsed()
    );
    // the shell waits for it: the limit kills the shell and the call still returns
    let waits = sb.bws("", "sleep 20 &\nwait");
    let start = Instant::now();
    assert!(run_capped(std::process::Command::new(&waits), SHORT).is_none());
    assert!(
        start.elapsed() < Duration::from_secs(8),
        "{:?}",
        start.elapsed()
    );
}

// ---- 010 T001: the keyed lookup (FR-001, FR-003, FR-005 to FR-007, FR-018; I10) ----

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// The held rows of a list that has the four runner keys plus `rows`, for the allowlist `allowed`.
fn held(rows: &[(&str, &str)], allowed: &[&str]) -> Held {
    let mut all = GOOD.to_vec();
    all.extend_from_slice(rows);
    parse_list_with(&list(&all), &strs(allowed))
        .ok()
        .expect("a valid list")
        .held
}

/// Takes `SITE_API_KEY` alone from a list where it holds `value`.
fn one(value: &str) -> Result<std::collections::BTreeMap<String, String>, NamedError> {
    held(&[("SITE_API_KEY", value)], &["SITE_API_KEY"])
        .take(&strs(&["SITE_API_KEY"]))
        .map(|s| s.0)
}

fn shown_with(r: Result<crate::store::Fetched, crate::store::StoreError>) -> String {
    format!("{}", r.err().expect("expected an error"))
}

#[test]
#[ignore = "test-first"]
fn held_take_returns_values_by_exact_key() {
    let h = held(
        &[
            ("SITE_API_KEY", "fake-site-value-1111"),
            ("OTHER_KEY", "fake-other-value-2222"),
        ],
        &["SITE_API_KEY", "OTHER_KEY"],
    );
    let got = h.take(&strs(&["SITE_API_KEY"])).ok().expect("one name").0;
    assert_eq!(got.len(), 1);
    assert_eq!(got["SITE_API_KEY"], "fake-site-value-1111");
    let both = h
        .take(&strs(&["OTHER_KEY", "SITE_API_KEY"]))
        .ok()
        .expect("two names")
        .0;
    assert_eq!(both.keys().collect::<Vec<_>>(), ["OTHER_KEY", "SITE_API_KEY"]);
    assert_eq!(both["OTHER_KEY"], "fake-other-value-2222");
    // an empty request is an empty map
    assert!(h.take(&[]).ok().expect("no names").0.is_empty());
}

#[test]
#[ignore = "test-first"]
fn held_take_missing_key_is_found_0() {
    let h = held(&[], &["SITE_API_KEY"]);
    let e = h.take(&strs(&["SITE_API_KEY"])).err().expect("missing key");
    assert!(matches!(e, NamedError::Count { index: 0, found: 0 }), "{e:?}");
    assert_eq!(format!("{e}"), "secret #0: expected 1, found 0");
    // a name outside the allowlist is found 0 although the store holds the key
    let h = held(&[("NOT_LISTED", "fake-unlisted-3333")], &["SITE_API_KEY"]);
    let e = h.take(&strs(&["NOT_LISTED"])).err().expect("not allowlisted");
    assert!(matches!(e, NamedError::Count { index: 0, found: 0 }), "{e:?}");
    // the four runner values are never reachable through take (I5)
    let e = h.take(&strs(&["FACTORY_CI_TOKEN"])).err().expect("runner key");
    assert!(matches!(e, NamedError::Count { index: 0, found: 0 }), "{e:?}");
}

#[test]
#[ignore = "test-first"]
fn held_take_duplicate_key_is_found_2() {
    let h = held(
        &[
            ("B_KEY", "fake-b-value-1111"),
            ("A_KEY", "fake-a1-value-2222"),
            ("A_KEY", "fake-a2-value-3333"),
        ],
        &["A_KEY", "B_KEY"],
    );
    let e = h.take(&strs(&["B_KEY", "A_KEY"])).err().expect("duplicate");
    assert!(matches!(e, NamedError::Count { index: 1, found: 2 }), "{e:?}");
    assert_eq!(format!("{e}"), "secret #1: expected 1, found 2");
    let text = format!("{e} {e:?}");
    assert!(!text.contains("fake-"), "{text}");
    // three rows
    let h = held(
        &[
            ("A_KEY", "fake-1-1111"),
            ("A_KEY", "fake-2-2222"),
            ("A_KEY", "fake-3-3333"),
        ],
        &["A_KEY"],
    );
    let e = h.take(&strs(&["A_KEY"])).err().expect("triplicate");
    assert!(matches!(e, NamedError::Count { index: 0, found: 3 }), "{e:?}");
}

#[test]
#[ignore = "test-first"]
fn held_take_key_differing_in_case_does_not_match() {
    for key in [
        "site_api_key",
        "Site_Api_Key",
        "SITE_API_KEY_2",
        "XSITE_API_KEY",
        "SITE_API_KEY ",
        " SITE_API_KEY",
    ] {
        let h = held(&[(key, "fake-site-value-1111")], &["SITE_API_KEY"]);
        let e = h.take(&strs(&["SITE_API_KEY"])).err().expect(key);
        assert!(
            matches!(e, NamedError::Count { index: 0, found: 0 }),
            "{key:?}: {e:?}"
        );
    }
}

#[test]
#[ignore = "test-first"]
fn held_take_refuses_empty_blank_and_each_control_character() {
    for v in [
        "",
        "   ",
        "\n",
        "abcd\0efgh",
        "abcd\tefgh",
        "abcd\refgh",
        "abcd\x0befgh",
        "abcd\x0cefgh",
        "abcd\u{85}efgh",
        "abcd\u{2028}efgh",
        "abcd\u{2029}efgh",
    ] {
        let e = one(v).err().unwrap_or_else(|| panic!("{v:?} was accepted"));
        assert!(matches!(e, NamedError::Unusable { index: 0 }), "{v:?}: {e:?}");
        assert_eq!(format!("{e}"), "secret #0: unusable value", "{v:?}");
    }
    // a value that is not a string is unusable too
    let mut rows: Vec<serde_json::Value> = serde_json::from_str(&list(&GOOD)).unwrap();
    rows.push(serde_json::json!({"key": "SITE_API_KEY", "value": 12345}));
    rows.push(serde_json::json!({"key": "NULL_KEY", "value": null}));
    let h = parse_list_with(
        &serde_json::Value::Array(rows).to_string(),
        &strs(&["SITE_API_KEY", "NULL_KEY"]),
    )
    .ok()
    .expect("the list parses")
    .held;
    for n in ["SITE_API_KEY", "NULL_KEY"] {
        let e = h.take(&strs(&[n])).err().expect(n);
        assert!(matches!(e, NamedError::Unusable { index: 0 }), "{n}: {e:?}");
    }
}

#[test]
#[ignore = "test-first"]
fn held_take_line_boundary() {
    // multi-line: every non-blank trimmed line has 4 or more characters; blank inner lines, CRLF and a
    // trailing newline pass; the value comes back unchanged
    for v in [
        "good-line\nabcd",
        "good-line\n\nabcd",
        "good-line\r\nabcd",
        "good-line\nabcd\n",
        "good-line\r\nabcd\r\n",
        "good-line\n  abcd  \nlast-line",
        "-----BEGIN KEY-----\n\nbody-1234\n\n-----END KEY-----\n",
    ] {
        let got = one(v).ok().unwrap_or_else(|| panic!("{v:?} was refused"));
        assert_eq!(got["SITE_API_KEY"], v);
    }
    for v in [
        "good-line\nabc",
        "good-line\n  abc  \nlast-line",
        "abc\ngood-line",
        "good-line\nabcd\nxy\n",
        "good-line\n\n\nab\n\nlast-line",
    ] {
        let e = one(v).err().unwrap_or_else(|| panic!("{v:?} was accepted"));
        assert!(matches!(e, NamedError::Unusable { index: 0 }), "{v:?}: {e:?}");
    }
    // a single line of 4 characters passes
    assert_eq!(one("abcd").ok().unwrap()["SITE_API_KEY"], "abcd");
}

#[test]
#[ignore = "test-first"]
fn held_take_refuses_surrounding_whitespace_on_a_single_line() {
    for v in [" abcd", "abcd ", "abcd\n", "abcd\r\n", "  abcd  ", "\nabcd"] {
        let e = one(v).err().unwrap_or_else(|| panic!("{v:?} was accepted"));
        assert!(matches!(e, NamedError::Unusable { index: 0 }), "{v:?}: {e:?}");
    }
    assert_eq!(one("abcd efgh").ok().unwrap()["SITE_API_KEY"], "abcd efgh");
}

#[test]
#[ignore = "test-first"]
fn held_take_size_cap() {
    let at = "a".repeat(32768);
    assert_eq!(one(&at).ok().expect("32768 bytes")["SITE_API_KEY"], at);
    let e = one(&"a".repeat(32769)).err().expect("32769 bytes");
    assert!(matches!(e, NamedError::Unusable { index: 0 }), "{e:?}");
}

#[test]
#[ignore = "test-first"]
fn held_take_passes_unicode_and_equals_untouched() {
    for v in [
        "pä€漢字=x==",
        "====",
        "key=value=more",
        "emoji-😀-ok",
        "a'b\"c\\d$e`f",
    ] {
        let got = one(v).ok().unwrap_or_else(|| panic!("{v:?}"));
        assert_eq!(got["SITE_API_KEY"], v);
    }
}

#[test]
#[ignore = "test-first"]
fn held_take_validates_only_the_names_taken() {
    let h = held(
        &[
            ("GOOD_KEY", "fake-good-value-1111"),
            ("BAD_KEY", "bad\0value-2222"),
            ("DUP_KEY", "fake-dup1-value-3333"),
            ("DUP_KEY", "fake-dup2-value-4444"),
        ],
        &["GOOD_KEY", "BAD_KEY", "DUP_KEY", "MISSING_KEY"],
    );
    let got = h.take(&strs(&["GOOD_KEY"])).ok().expect("only the good key").0;
    assert_eq!(got.keys().collect::<Vec<_>>(), ["GOOD_KEY"]);
    // asking for the bad one fails the whole call, with the position of the bad name
    let e = h.take(&strs(&["GOOD_KEY", "BAD_KEY"])).err().expect("bad value");
    assert!(matches!(e, NamedError::Unusable { index: 1 }), "{e:?}");
    // and fetch_with does not fail on such a list either
    let sb = Sandbox::new();
    let mut rows = GOOD.to_vec();
    rows.push(("BAD_KEY", "bad\0value-2222"));
    rows.push(("DUP_KEY", "fake-dup1-value-3333"));
    rows.push(("DUP_KEY", "fake-dup2-value-4444"));
    let bws = sb.bws(&list(&rows), "cat \"$d/out\"");
    let allowed = strs(&["BAD_KEY", "DUP_KEY", "MISSING_KEY"]);
    assert!(fetch_with(&bws, ACCESS, None, LONG, &allowed).is_ok());
}

#[test]
#[ignore = "test-first"]
fn held_take_looks_up_a_repeated_name_once() {
    let h = held(
        &[
            ("SITE_API_KEY", "fake-site-value-1111"),
            ("DUP_KEY", "fake-d1-1111"),
            ("DUP_KEY", "fake-d2-2222"),
        ],
        &["SITE_API_KEY", "DUP_KEY"],
    );
    let got = h
        .take(&strs(&["SITE_API_KEY", "SITE_API_KEY"]))
        .ok()
        .expect("a repeated name")
        .0;
    assert_eq!(got.len(), 1);
    assert_eq!(got["SITE_API_KEY"], "fake-site-value-1111");
    let e = h.take(&strs(&["DUP_KEY", "DUP_KEY"])).err().expect("duplicate key");
    assert!(matches!(e, NamedError::Count { index: 0, found: 2 }), "{e:?}");
}

#[test]
#[ignore = "test-first"]
fn fetch_with_keeps_only_allowlisted_rows_and_still_needs_the_four_runner_keys() {
    let mut rows = GOOD.to_vec();
    rows.push(("SITE_API_KEY", "fake-site-value-1111"));
    rows.push(("NOT_LISTED", "fake-unlisted-3333"));
    let sb = Sandbox::new();
    let bws = sb.bws(&list(&rows), "cat \"$d/out\"");
    let got = fetch_with(&bws, ACCESS, None, LONG, &strs(&["SITE_API_KEY"]))
        .ok()
        .expect("fetch_with");
    assert_eq!(got.secrets.token, GOOD[0].1);
    assert_eq!(got.secrets.log_url, GOOD[1].1);
    assert_eq!(got.secrets.log_user, GOOD[2].1);
    assert_eq!(got.secrets.log_token, GOOD[3].1);
    assert_eq!(sb.calls(), 1, "one bws call");
    assert_eq!(sb.argv(1), "secret\nlist\n-o\njson\n");
    let ok = got.held.take(&strs(&["SITE_API_KEY"])).ok().expect("allowlisted").0;
    assert_eq!(ok["SITE_API_KEY"], "fake-site-value-1111");
    // a row that is not on the allowlist was dropped
    let e = got.held.take(&strs(&["NOT_LISTED"])).err().expect("dropped row");
    assert!(matches!(e, NamedError::Count { index: 0, found: 0 }), "{e:?}");
    // the four runner keys stay required, with the errors of `fetch`
    for k in KEYS {
        let without: Vec<_> = rows.iter().filter(|(n, _)| *n != k).cloned().collect();
        let sb = Sandbox::new();
        let bws = sb.bws(&list(&without), "cat \"$d/out\"");
        let r = fetch_with(&bws, ACCESS, None, LONG, &strs(&["SITE_API_KEY"]));
        assert_eq!(shown_with(r), format!("secret {k}: expected 1, found 0"));
    }
}

#[test]
#[ignore = "test-first"]
fn fetch_with_empty_allowlist_equals_fetch() {
    let (sa, sb) = (Sandbox::new(), Sandbox::new());
    let eu = Some("https://vault.bitwarden.eu");
    let a = fetch(&sa.ok(), ACCESS, eu, LONG).ok().expect("fetch");
    let b = fetch_with(&sb.ok(), ACCESS, eu, LONG, &[])
        .ok()
        .expect("fetch_with");
    assert_eq!(
        (&a.token, &a.log_url, &a.log_user, &a.log_token),
        (
            &b.secrets.token,
            &b.secrets.log_url,
            &b.secrets.log_user,
            &b.secrets.log_token
        )
    );
    assert_eq!(sa.argv(1), sb.argv(1));
    let (mut ea, mut eb) = (names(&sa.env(1)), names(&sb.env(1)));
    ea.sort();
    eb.sort();
    assert_eq!(ea, eb);
    assert_eq!((sa.calls(), sb.calls()), (1, 1));
    // nothing is held
    let e = b.held.take(&strs(&["ANY_KEY"])).err().expect("empty allowlist");
    assert!(matches!(e, NamedError::Count { index: 0, found: 0 }), "{e:?}");
    // the same errors and the same number of calls for the same failures
    let missing: Vec<_> = GOOD.iter().filter(|(n, _)| *n != KEYS[2]).cloned().collect();
    let cases = [
        (list(&missing), "cat \"$d/out\""),
        ("not json".to_string(), "cat \"$d/out\""),
        (String::new(), "echo 'Error: 401 Unauthorized' >&2; exit 1"),
        (String::new(), "echo 'connection refused' >&2; exit 1"),
    ];
    for (out, body) in cases {
        let (x, y) = (Sandbox::new(), Sandbox::new());
        let (bx, by) = (x.bws(&out, body), y.bws(&out, body));
        let ex = format!("{}", fetch(&bx, ACCESS, None, LONG).err().expect("fails"));
        let ey = shown_with(fetch_with(&by, ACCESS, None, LONG, &[]));
        assert_eq!(ex, ey, "{body}");
        assert_eq!(x.calls(), y.calls(), "{body}");
    }
}

#[test]
#[ignore = "test-first"]
fn fetch_with_retries_once_and_a_rejected_token_is_final() {
    let allowed = strs(&["SITE_API_KEY"]);
    let sb = Sandbox::new();
    let bws = sb.bws("", "echo 'Error: 403 Forbidden' >&2; exit 1");
    assert_eq!(
        shown_with(fetch_with(&bws, ACCESS, None, LONG, &allowed)),
        "secret store rejected the access token"
    );
    assert_eq!(sb.calls(), 1, "a rejected token is not retried");
    let sb = Sandbox::new();
    let bws = sb.bws("", "echo 'Error: connection refused' >&2; exit 1");
    assert_eq!(
        shown_with(fetch_with(&bws, ACCESS, None, LONG, &allowed)),
        "secret store unreachable"
    );
    assert_eq!(sb.calls(), 2, "one retry");
    // the second attempt can succeed
    let sb = Sandbox::new();
    let bws = sb.bws(
        &list(&GOOD),
        "[ \"$n\" = 1 ] && { echo 'Error: timed out' >&2; exit 1; }\ncat \"$d/out\"",
    );
    assert!(fetch_with(&bws, ACCESS, None, LONG, &allowed).is_ok());
    assert_eq!(sb.calls(), 2);
    // a row without a string key is unreadable output
    let sb = Sandbox::new();
    let bws = sb.bws("[{\"nokey\": 1}]", "cat \"$d/out\"");
    assert_eq!(
        shown_with(fetch_with(&bws, ACCESS, None, LONG, &allowed)),
        "secret store unreachable"
    );
}

#[test]
#[ignore = "test-first"]
fn fetch_with_refuses_a_foreign_server_url() {
    let allowed = strs(&["SITE_API_KEY"]);
    for bad in [
        "http://vault.bitwarden.com",
        "https://vault.bitwarden.com.evil.example",
        "https://evil.example",
        "https://vault.bitwarden.com@evil.example",
    ] {
        let sb = Sandbox::new();
        assert_eq!(
            shown_with(fetch_with(&sb.ok(), ACCESS, Some(bad), LONG, &allowed)),
            "secret store unreachable",
            "{bad}"
        );
        assert_eq!(sb.calls(), 0, "{bad}: bws was called");
    }
}

#[test]
#[ignore = "test-first"]
fn fetch_with_child_env_is_cleared() {
    let sb = Sandbox::new();
    let com = Some("https://vault.bitwarden.com");
    fetch_with(&sb.ok(), ACCESS, com, LONG, &strs(&["SITE_API_KEY"]))
        .ok()
        .expect("fetch_with");
    assert!(!sb.argv(1).contains(ACCESS), "token in argv");
    let env = sb.env(1);
    let want = format!("BWS_ACCESS_TOKEN={ACCESS}");
    assert!(env.lines().any(|l| l == want), "{env}");
    assert!(!env.contains("CARGO_PKG_NAME"), "{env}");
    let allowed = [
        "PATH",
        "HOME",
        "BWS_ACCESS_TOKEN",
        "BWS_SERVER_URL",
        "PWD",
        "OLDPWD",
        "SHLVL",
        "_",
    ];
    for n in names(&env) {
        assert!(allowed.contains(&n.as_str()), "{n} is not on the allowlist");
    }
}

#[test]
#[ignore = "test-first"]
fn debug_and_display_of_held_results_and_errors_hold_no_value_and_no_name() {
    let h = held(
        &[
            ("SITE_API_KEY", "fake-held-value-4444"),
            ("DUP_KEY", "fake-dup1-value-5555"),
            ("DUP_KEY", "fake-dup2-value-6666"),
            ("BAD_KEY", "bad\0value-7777"),
        ],
        &["SITE_API_KEY", "DUP_KEY", "BAD_KEY"],
    );
    let no_leak = |what: &str, text: &str| {
        for needle in ["fake-", "value-", "SITE_API_KEY", "DUP_KEY", "BAD_KEY"] {
            assert!(!text.contains(needle), "{what} holds {needle}: {text}");
        }
    };
    no_leak("Held", &format!("{h:?}"));
    for n in ["DUP_KEY", "BAD_KEY"] {
        let e = h.take(&strs(&[n])).err().expect(n);
        no_leak("NamedError", &format!("{e} {e:?}"));
    }
    // the map prints no value; it may print names
    let s = h.take(&strs(&["SITE_API_KEY"])).ok().expect("one name");
    let text = format!("{s:?}");
    assert!(!text.contains("fake-held-value-4444"), "{text}");
}
