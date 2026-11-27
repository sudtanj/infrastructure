//! T033. Targets `ship::{LogConfig, Shipper, Labels, Meta, Timing, link}` against a fake Loki
//! (contracts/loki-push.md, data-model.md LogConfig and LogLine). FR-028, FR-029, FR-033, FR-034,
//! FR-036; AS 11.4, 11.5, 11.7; SC-011; I7, I15, I16.
//! Assumed shapes: `LogConfig::from_env(&dyn Fn(&str) -> Option<String>) -> Result<Option<LogConfig>,
//! Vec<String>>`; `Shipper::start_with(cfg, Labels { repo, project }, Meta { run_id, sha, branch },
//! Timing) -> Arc<Shipper>`; `Timing { batch, retry_window, backoff, request_timeout: Duration,
//! max_batch: usize, max_queue: usize }` with `Default` (1 s, 60 s, 1 s, 10 s, 1 MB, 32 MiB);
//! `Shipper::finish(Duration) -> bool` (delivered) and `Shipper::dropped() -> bool`.

use crate::log::Sink;
use crate::ship::{link, Labels, LogConfig, Meta, Shipper, Timing};
use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const USER: &str = "loki-user";
const TOKEN: &str = "loki-token-QQ77";

#[derive(Clone)]
struct Seen {
    url: String,
    auth: String,
    ctype: String,
    body: String,
}

struct Loki {
    url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Loki {
    fn count(&self) -> usize {
        self.seen.lock().unwrap().len()
    }

    /// `(labels, ts, line)` of every value received, in arrival order.
    fn values(&self) -> Vec<(serde_json::Value, String, String)> {
        let mut out = vec![];
        for r in self.seen.lock().unwrap().iter() {
            let v: serde_json::Value = serde_json::from_str(&r.body).unwrap();
            for s in v["streams"].as_array().unwrap() {
                for val in s["values"].as_array().unwrap() {
                    out.push((s["stream"].clone(), val[0].as_str().unwrap().into(), val[1].as_str().unwrap().into()));
                }
            }
        }
        out
    }
}

/// `f(index)` returns `(status, delay)`.
fn loki(f: impl Fn(usize) -> (u16, Duration) + Send + 'static) -> Loki {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
    let log = seen.clone();
    std::thread::spawn(move || {
        for mut rq in server.incoming_requests() {
            let mut body = String::new();
            let _ = rq.as_reader().read_to_string(&mut body);
            let hdr = |n: &str| {
                rq.headers().iter().find(|h| h.field.to_string().eq_ignore_ascii_case(n)).map(|h| h.value.to_string()).unwrap_or_default()
            };
            let s = Seen { url: rq.url().to_string(), auth: hdr("Authorization"), ctype: hdr("Content-Type"), body };
            let n = {
                let mut g = log.lock().unwrap();
                g.push(s);
                g.len() - 1
            };
            let (code, delay) = f(n);
            std::thread::sleep(delay);
            let _ = rq.respond(tiny_http::Response::from_string("").with_status_code(code));
        }
    });
    Loki { url: format!("http://127.0.0.1:{port}"), seen }
}

fn ok_loki() -> Loki {
    loki(|_| (204, Duration::ZERO))
}

fn cfg(url: &str) -> LogConfig {
    let env: HashMap<&str, String> = [
        ("FACTORY_CI_LOG_URL", url.to_string()),
        ("FACTORY_CI_LOG_USER", USER.to_string()),
        ("FACTORY_CI_LOG_TOKEN", TOKEN.to_string()),
    ]
    .into_iter()
    .collect();
    LogConfig::from_env(&|k| env.get(k).cloned()).unwrap().unwrap()
}

fn fast() -> Timing {
    Timing {
        batch: Duration::from_millis(100),
        retry_window: Duration::from_millis(600),
        backoff: Duration::from_millis(20),
        request_timeout: Duration::from_millis(500),
        ..Timing::default()
    }
}

fn start(l: &Loki, t: Timing) -> Arc<Shipper> {
    let labels = Labels { repo: "o/r".into(), project: "svc".into() };
    let meta = Meta { run_id: "0123456789abcdef".into(), sha: "a".repeat(40), branch: "feature/x".into() };
    Shipper::start_with(cfg(&l.url), labels, meta, t)
}

fn wait_for(what: &str, f: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while !f() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

// ---- LogConfig::from_env (FR-028, FR-029) ----

fn from(pairs: &[(&str, &str)]) -> Result<Option<LogConfig>, Vec<String>> {
    let m: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    LogConfig::from_env(&|k| m.get(k).cloned())
}

const U: (&str, &str) = ("FACTORY_CI_LOG_URL", "https://loki.example.com/");
const N: (&str, &str) = ("FACTORY_CI_LOG_USER", "u-secret-1");
const T: (&str, &str) = ("FACTORY_CI_LOG_TOKEN", "t-secret-2");

#[test]
fn none_set_is_off_and_all_set_is_on() {
    assert!(from(&[]).unwrap().is_none());
    let c = from(&[U, N, T]).unwrap().unwrap();
    assert_eq!(c.url, "https://loki.example.com", "a trailing slash is trimmed");
    assert_eq!((c.user.as_str(), c.token.as_str()), ("u-secret-1", "t-secret-2"));
    assert!(c.link.is_none());
}

#[test]
fn a_partial_set_names_each_missing_variable_and_no_value() {
    // AS 11.4
    let e = from(&[U]).err().unwrap().join("\n");
    assert!(e.contains("FACTORY_CI_LOG_USER") && e.contains("FACTORY_CI_LOG_TOKEN"), "{e}");
    let e = from(&[U, T]).err().unwrap().join("\n");
    assert!(e.contains("FACTORY_CI_LOG_USER") && !e.contains("FACTORY_CI_LOG_TOKEN"), "{e}");
    for v in ["loki.example.com", "u-secret-1", "t-secret-2"] {
        assert!(!e.contains(v), "{v} leaked: {e}");
    }
    let e = from(&[N, T]).err().unwrap().join("\n");
    assert!(e.contains("FACTORY_CI_LOG_URL"), "{e}");
}

#[test]
fn an_empty_value_counts_as_missing() {
    let e = from(&[U, N, ("FACTORY_CI_LOG_TOKEN", "")]).err().unwrap().join("\n");
    assert!(e.contains("FACTORY_CI_LOG_TOKEN"), "{e}");
}

#[test]
fn the_url_must_be_https_without_userinfo() {
    // data-model.md LogConfig.url, FR-028, R8
    for bad in ["ftp://h/", "http://loki.example.com", "http://127.0.0.1.example.com", "h.example.com", "https://user:pw@h.example.com", "https://user@h.example.com", ""] {
        let e = from(&[("FACTORY_CI_LOG_URL", bad), N, T]);
        assert!(e.is_err(), "{bad:?} accepted");
        let text = e.err().unwrap().join("\n");
        assert!(!text.contains("pw@") && !text.contains("user:pw"), "{text}");
    }
    let e = from(&[("FACTORY_CI_LOG_URL", "http://loki.example.com"), N, T]).err().unwrap().join("\n");
    assert!(e.contains("log URL must be https"), "{e}");
    assert!(from(&[("FACTORY_CI_LOG_URL", "https://loki.example.com:3100"), N, T]).is_ok());
    // the runner's own machine may use plain http
    assert!(from(&[("FACTORY_CI_LOG_URL", "http://127.0.0.1:3100"), N, T]).is_ok());
    assert!(from(&[("FACTORY_CI_LOG_URL", "http://localhost"), N, T]).is_ok());
}

#[test]
fn the_link_template_needs_the_run_id_placeholder_and_a_web_scheme() {
    // FR-029
    let l = |v: &'static str| from(&[U, N, T, ("FACTORY_CI_LOG_LINK", v)]);
    assert!(l("https://grafana.example.com/explore?q={run_id}").is_ok());
    assert!(l("https://grafana.example.com/explore").is_err());
    assert!(l("ftp://x/{run_id}").is_err());
    assert!(l("javascript:alert({run_id})").is_err());
    let c = l("https://g/{run_id}").unwrap().unwrap();
    assert_eq!(c.link.as_deref(), Some("https://g/{run_id}"));
}

#[test]
fn link_fills_every_placeholder() {
    assert_eq!(link("https://g/x?a={run_id}&b={run_id}", "abc"), "https://g/x?a=abc&b=abc");
}

// ---- payload (FR-033, FR-034, FR-036) ----

#[test]
fn the_payload_has_four_labels_a_prefix_nanosecond_timestamps_and_basic_auth() {
    let l = ok_loki();
    let s = start(&l, fast());
    s.push("build", "stdout", "compiling");
    s.push("build", "stderr", "warning: x");
    s.push("runner", "stdout", "clone done");
    assert!(s.finish(Duration::from_secs(5)));
    let seen = l.seen.lock().unwrap().clone();
    assert!(!seen.is_empty());
    let want_auth = format!("Basic {}", crate::util::base64(format!("{USER}:{TOKEN}").as_bytes()));
    for r in &seen {
        assert_eq!(r.url, "/loki/api/v1/push");
        assert!(r.ctype.starts_with("application/json"), "{}", r.ctype);
        assert_eq!(r.auth, want_auth);
    }
    let vals = l.values();
    assert_eq!(vals.len(), 3);
    for (labels, ts, line) in &vals {
        let mut keys: Vec<_> = labels.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["project", "repo", "service_name", "step", "stream"]);
        assert_eq!(labels["service_name"].as_str(), Some("factory-ci"));
        assert_eq!((labels["repo"].as_str(), labels["project"].as_str()), (Some("o/r"), Some("svc")));
        assert!(ts.len() >= 18 && ts.bytes().all(|b| b.is_ascii_digit()), "{ts}");
        assert!(line.starts_with("run=0123456789abcdef sha=aaaaaaaa branch=feature/x "), "{line}");
    }
    let find = |step: &str, stream: &str| vals.iter().find(|v| v.0["step"] == step && v.0["stream"] == stream).unwrap();
    assert!(find("build", "stdout").2.ends_with("compiling"));
    assert!(find("build", "stderr").2.ends_with("warning: x"));
    assert!(find("runner", "stdout").2.ends_with("clone done"));
}

#[test]
fn values_are_sorted_by_timestamp_inside_each_stream() {
    let l = ok_loki();
    let s = start(&l, fast());
    for i in 0..50 {
        s.push("build", "stdout", &format!("line {i}"));
    }
    s.finish(Duration::from_secs(5));
    let mut last: Option<u128> = None;
    for (_, ts, _) in l.values() {
        let t: u128 = ts.parse().unwrap();
        assert!(last.is_none_or(|p| p <= t), "unsorted");
        last = Some(t);
    }
}

#[test]
fn the_branch_in_the_prefix_is_cut_to_40_characters() {
    let l = ok_loki();
    let labels = Labels { repo: "o/r".into(), project: ".".into() };
    let meta = Meta { run_id: "0123456789abcdef".into(), sha: "b".repeat(40), branch: "x".repeat(80) };
    let s = Shipper::start_with(cfg(&l.url), labels, meta, fast());
    s.push("a", "stdout", "hi");
    s.finish(Duration::from_secs(5));
    let line = &l.values()[0].2;
    assert!(line.contains(&format!("branch={} ", "x".repeat(40))) && !line.contains(&"x".repeat(41)), "{line}");
}

#[test]
fn nul_becomes_the_replacement_character_at_ship_time() {
    let l = ok_loki();
    let s = start(&l, fast());
    s.push("a", "stdout", "a\0b");
    s.finish(Duration::from_secs(5));
    assert!(l.values()[0].2.ends_with("a\u{fffd}b"));
}

#[test]
fn payload_invariants_hold_for_random_pushes() {
    // I15
    let l = ok_loki();
    let s = start(&l, fast());
    let mut x: u64 = 5;
    for _ in 0..200 {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let len = ((x >> 33) % 16_000) as usize;
        let step = ["a", "b", "runner"][((x >> 20) % 3) as usize];
        let stream = if (x >> 10) % 2 == 0 { "stdout" } else { "stderr" };
        s.push(step, stream, &"z".repeat(len));
    }
    assert!(s.finish(Duration::from_secs(10)));
    let vals = l.values();
    assert_eq!(vals.len(), 200);
    for (labels, _, line) in &vals {
        assert_eq!(labels.as_object().unwrap().len(), 4);
        assert!(line.len() <= 16 * 1024 + 128, "{} bytes", line.len());
    }
}

// ---- batching ----

#[test]
fn a_batch_leaves_after_the_batch_interval_without_finish() {
    // FR-033
    let l = ok_loki();
    let s = start(&l, Timing { batch: Duration::from_millis(150), ..fast() });
    s.push("a", "stdout", "hello");
    wait_for("the timed batch", || l.count() == 1);
    assert_eq!(l.values().len(), 1);
}

#[test]
fn a_batch_leaves_when_it_reaches_the_size_limit() {
    let l = ok_loki();
    let s = start(&l, Timing { batch: Duration::from_secs(60), max_batch: 64 * 1024, ..fast() });
    for _ in 0..8 {
        s.push("a", "stdout", &"q".repeat(15_000));
    }
    wait_for("the size-triggered batch", || l.count() >= 1);
}

// ---- failure handling (FR-034) ----

#[test]
fn retryable_failures_are_retried_until_the_window_then_dropped() {
    // AS 11.5, I16
    for code in [429u16, 500, 503] {
        let l = loki(move |_| (code, Duration::ZERO));
        let s = start(&l, fast());
        s.push("a", "stdout", "x");
        let t = Instant::now();
        assert!(!s.finish(Duration::from_secs(5)), "{code}");
        assert!(t.elapsed() < Duration::from_secs(3), "{code}: {:?}", t.elapsed());
        assert!(l.count() >= 2, "{code}: {} attempts", l.count());
        assert!(s.dropped());
        let seen = l.count();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(l.count(), seen, "{code}: retries continued after the window");
    }
}

#[test]
fn a_transient_failure_that_recovers_is_delivered() {
    let l = loki(|n| (if n < 2 { 503 } else { 204 }, Duration::ZERO));
    let s = start(&l, fast());
    s.push("a", "stdout", "x");
    assert!(s.finish(Duration::from_secs(5)));
    assert!(!s.dropped());
    assert_eq!(l.count(), 3);
}

#[test]
fn auth_failures_stop_at_once_and_later_lines_make_no_request() {
    for code in [401u16, 403] {
        let l = loki(move |_| (code, Duration::ZERO));
        let s = start(&l, fast());
        s.push("a", "stdout", "first");
        assert!(!s.finish(Duration::from_secs(5)));
        assert_eq!(l.count(), 1, "{code}: no retry");
        s.push("a", "stdout", "second");
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(l.count(), 1, "{code}: later lines must be dropped without a request");
        assert!(s.dropped());
    }
}

#[test]
fn a_400_drops_the_batch_without_retry_and_shipping_goes_on() {
    let l = loki(|n| (if n == 0 { 400 } else { 204 }, Duration::ZERO));
    let s = start(&l, fast());
    s.push("a", "stdout", "bad batch");
    wait_for("the first attempt", || l.count() == 1);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(l.count(), 1, "no retry on 400");
    s.push("a", "stdout", "next");
    s.finish(Duration::from_secs(5));
    assert_eq!(l.count(), 2);
    assert!(s.dropped());
}

#[test]
fn finish_returns_within_the_window_against_a_hanging_server() {
    // SC-011
    let l = loki(|_| (204, Duration::from_secs(10)));
    let s = start(&l, fast());
    s.push("a", "stdout", "x");
    let t = Instant::now();
    assert!(!s.finish(Duration::from_millis(500)));
    assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
}

#[test]
fn a_full_queue_drops_lines_without_blocking_push() {
    // FR-036: the 32 MiB cap, scaled down through Timing
    let l = loki(|_| (204, Duration::from_secs(10)));
    let s = start(&l, Timing { batch: Duration::from_secs(60), max_queue: 64 * 1024, ..fast() });
    let t = Instant::now();
    for _ in 0..500 {
        s.push("a", "stdout", &"w".repeat(15_000));
    }
    assert!(t.elapsed() < Duration::from_secs(2), "push blocked for {:?}", t.elapsed());
    assert!(s.dropped());
}

#[test]
fn nothing_goes_out_when_nothing_was_pushed() {
    let l = ok_loki();
    let s = start(&l, fast());
    assert!(s.finish(Duration::from_secs(2)));
    assert_eq!(l.count(), 0);
    assert!(!s.dropped());
}
