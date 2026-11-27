//! T004. Targets `github::{Api, HttpApi, ApiError, RATE_RESERVE, compose_description}` against a
//! local `tiny_http` fake (contracts/github-api.md). FR-006, FR-013, FR-014, FR-035, FR-027; I11.
//! Assumed shapes: `HttpApi::new(base, token)` and `.with_backoff(Duration)` (base delay of the
//! 1 s / 2 s retries), trait methods `user`, `repo`, `branches`, `tree(r, sha, recursive)`,
//! `status`, `commits`, `compare` (`Ok(None)` when still full after 3 pages), `post_status`;
//! `ApiError` derives Debug.

use crate::github::{compose_description, Api, ApiError, HttpApi, RATE_RESERVE};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const TOKEN: &str = "tok-GH-SECRET-42";

#[derive(Clone, Debug)]
struct Seen {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

struct Resp(u16, Vec<(&'static str, String)>, String);

fn ok(body: &str) -> Resp {
    Resp(200, vec![], body.to_string())
}

struct Srv {
    url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Srv {
    fn urls(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.url.clone())
            .collect()
    }
}

/// `f(request, index of this request)` decides the answer.
fn serve(f: impl Fn(&Seen, usize) -> Resp + Send + 'static) -> Srv {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
    let log = seen.clone();
    std::thread::spawn(move || {
        for mut rq in server.incoming_requests() {
            let mut body = String::new();
            let _ = rq.as_reader().read_to_string(&mut body);
            let s = Seen {
                method: rq.method().to_string(),
                url: rq.url().to_string(),
                headers: rq
                    .headers()
                    .iter()
                    .map(|h| (h.field.to_string(), h.value.to_string()))
                    .collect(),
                body,
            };
            let n = {
                let mut g = log.lock().unwrap();
                g.push(s.clone());
                g.len() - 1
            };
            let Resp(code, headers, text) = f(&s, n);
            let mut r = tiny_http::Response::from_string(text).with_status_code(code);
            for (k, v) in headers {
                r.add_header(tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap());
            }
            let _ = rq.respond(r);
        }
    });
    Srv {
        url: format!("http://127.0.0.1:{port}"),
        seen,
    }
}

fn api(s: &Srv) -> HttpApi {
    HttpApi::new(&s.url, TOKEN).with_backoff(Duration::from_millis(1))
}

fn sha(n: u32) -> String {
    format!("{n:040x}")
}

fn branches_json(n: usize) -> String {
    let v: Vec<String> = (0..n)
        .map(|i| {
            format!(
                r#"{{"name":"b{i}","commit":{{"sha":"{}"}}}}"#,
                sha(i as u32)
            )
        })
        .collect();
    format!("[{}]", v.join(","))
}

// ---- request shape on every call (FR-006) ----

#[test]
fn every_call_has_the_documented_request_line_and_headers() {
    let s = serve(|r, _| {
        let u = &r.url;
        if u.contains("/branches") {
            ok(&branches_json(1))
        } else if u.contains("/git/trees/") {
            ok(r#"{"tree":[],"truncated":false}"#)
        } else if u.contains("/status") && !u.contains("/statuses/") {
            ok(r#"{"total_count":0,"statuses":[]}"#)
        } else if u.contains("/compare/") {
            ok(r#"{"files":[]}"#)
        } else if u.contains("/commits?") {
            ok("[]")
        } else if u.contains("/statuses/") {
            Resp(201, vec![], "{}".into())
        } else if u == "/user" {
            ok(r#"{"login":"ci-bot"}"#)
        } else {
            ok(r#"{"id":7,"private":false,"default_branch":"main"}"#)
        }
    });
    let a = api(&s);
    let (c, d) = (sha(1), sha(2));
    a.user().unwrap();
    a.repo("o/r").unwrap();
    a.branches("o/r").unwrap();
    a.tree("o/r", &c, true).unwrap();
    a.status("o/r", &c).unwrap();
    a.commits("o/r", &c).unwrap();
    a.compare("o/r", &c, &d).unwrap();
    a.post_status("o/r", &c, "factory-ci", "pending", "queued", None)
        .unwrap();
    let seen = s.seen.lock().unwrap().clone();
    let lines: Vec<String> = seen
        .iter()
        .map(|r| format!("{} {}", r.method, r.url))
        .collect();
    let want = [
        "GET /user".to_string(),
        "GET /repos/o/r".to_string(),
        "GET /repos/o/r/branches?per_page=100&page=1".to_string(),
        format!("GET /repos/o/r/git/trees/{c}?recursive=1"),
        format!("GET /repos/o/r/commits/{c}/status?per_page=100&page=1"),
        format!("GET /repos/o/r/commits?sha={c}&per_page=100"),
        format!("GET /repos/o/r/compare/{c}...{d}?per_page=100&page=1"),
        format!("POST /repos/o/r/statuses/{c}"),
    ];
    assert_eq!(lines, want);
    for r in &seen {
        assert_eq!(
            r.header("authorization"),
            Some(format!("Bearer {TOKEN}").as_str()),
            "{r:?}"
        );
        assert_eq!(r.header("accept"), Some("application/vnd.github+json"));
        assert_eq!(r.header("user-agent"), Some("rust-ci-runner"));
        assert_eq!(r.header("x-github-api-version"), Some("2022-11-28"));
    }
}

#[test]
fn user_and_repo_answers_are_decoded() {
    let s = serve(|r, _| {
        if r.url == "/user" {
            ok(r#"{"login":"ci-bot"}"#)
        } else {
            ok(r#"{"id":123456789,"private":true,"default_branch":"trunk"}"#)
        }
    });
    let a = api(&s);
    assert_eq!(a.user().unwrap(), "ci-bot");
    let r = a.repo("o/r").unwrap();
    assert_eq!(
        (r.id, r.private, r.default_branch.as_str()),
        (123_456_789, true, "trunk")
    );
}

// ---- paging ----

#[test]
fn paging_stops_at_a_short_page() {
    let s = serve(|r, _| {
        ok(&branches_json(if r.url.ends_with("page=1") {
            100
        } else {
            3
        }))
    });
    let v = api(&s).branches("o/r").unwrap();
    assert_eq!(v.len(), 103);
    assert_eq!(s.urls().len(), 2);
}

#[test]
fn a_sixth_page_tells_500_branches_from_more_and_more_is_an_error() {
    // R9: 500 branches pass, 501 and 600 fail
    for (total, fine) in [(500usize, true), (501, false), (600, false)] {
        let s = serve(move |r, _| {
            let page: usize = r.url.rsplit("page=").next().unwrap().parse().unwrap();
            ok(&branches_json(
                total.saturating_sub((page - 1) * 100).min(100),
            ))
        });
        let got = api(&s).branches("o/r");
        assert_eq!(got.is_ok(), fine, "{total}");
        match got {
            Ok(v) => assert_eq!(v.len(), 500),
            Err(e) => assert!(format!("{e:?}").contains("more than 500 branches"), "{e:?}"),
        }
    }
}

#[test]
fn combined_status_pages_until_short_or_total_count() {
    let page = |n: usize| {
        let v: Vec<String> = (0..n)
            .map(|i| {
                format!(r#"{{"context":"c{i}","state":"success","updated_at":"2026-11-26T00:00:00Z","description":"d","creator":{{"login":"ci-bot"}}}}"#)
            })
            .collect();
        v.join(",")
    };
    // 150 in total: a full page then a short one
    let s = serve(move |r, _| {
        let n = if r.url.ends_with("page=1") { 100 } else { 50 };
        ok(&format!(
            r#"{{"total_count":150,"statuses":[{}]}}"#,
            page(n)
        ))
    });
    let v = api(&s).status("o/r", &sha(1)).unwrap();
    assert_eq!(v.len(), 150);
    assert_eq!(s.urls().len(), 2);
    assert!(v[0].creator_login == "ci-bot" && v[0].state == "success");
    // exactly 100 and total_count 100: no second request
    let s = serve(move |_, _| {
        ok(&format!(
            r#"{{"total_count":100,"statuses":[{}]}}"#,
            page(100)
        ))
    });
    assert_eq!(api(&s).status("o/r", &sha(1)).unwrap().len(), 100);
    assert_eq!(s.urls().len(), 1);
}

#[test]
fn compare_that_is_still_full_after_three_pages_is_unknown() {
    let full = {
        let f: Vec<String> = (0..100)
            .map(|i| format!(r#"{{"filename":"f{i}"}}"#))
            .collect();
        format!(r#"{{"files":[{}]}}"#, f.join(","))
    };
    let s = serve(move |_, _| ok(&full));
    assert!(api(&s).compare("o/r", &sha(1), &sha(2)).unwrap().is_none());
    assert_eq!(s.urls().len(), 3);
}

#[test]
fn compare_reports_renames_with_the_previous_name() {
    let s = serve(|_, _| {
        ok(r#"{"files":[{"filename":"b/new","previous_filename":"a/old"},{"filename":"c"}]}"#)
    });
    let v = api(&s).compare("o/r", &sha(1), &sha(2)).unwrap().unwrap();
    assert_eq!(v[0].previous_filename.as_deref(), Some("a/old"));
    assert_eq!(v[1].previous_filename, None);
}

#[test]
fn commits_decode_parents_in_order_and_the_committer_date() {
    let p = sha(5);
    let q = sha(6);
    let body = format!(
        r#"[{{"sha":"{}","parents":[{{"sha":"{p}"}},{{"sha":"{q}"}}],"commit":{{"committer":{{"date":"2026-11-26T00:00:00Z"}}}}}}]"#,
        sha(1)
    );
    let s = serve(move |_, _| ok(&body));
    let v = api(&s).commits("o/r", &sha(1)).unwrap();
    assert_eq!(v[0].parents, [p, q]);
    assert_eq!(v[0].date, "2026-11-26T00:00:00Z");
}

// ---- errors and retries (FR-013, FR-014) ----

#[test]
fn a_5xx_is_retried_twice_then_becomes_http() {
    let s = serve(|_, _| Resp(503, vec![], "no".into()));
    let e = api(&s).user().unwrap_err();
    assert!(matches!(e, ApiError::Http(503, _)), "{e:?}");
    assert_eq!(s.urls().len(), 3);
}

#[test]
fn a_5xx_followed_by_success_succeeds() {
    let s = serve(|_, n| {
        if n == 0 {
            Resp(502, vec![], "".into())
        } else {
            ok(r#"{"login":"x"}"#)
        }
    });
    assert_eq!(api(&s).user().unwrap(), "x");
    assert_eq!(s.urls().len(), 2);
}

#[test]
fn a_transport_error_is_net_after_retries_and_hides_the_token() {
    let a = HttpApi::new("http://127.0.0.1:1", TOKEN).with_backoff(Duration::from_millis(1));
    let e = a.user().unwrap_err();
    assert!(matches!(e, ApiError::Net(_)), "{e:?}");
    assert!(!format!("{e:?}").contains(TOKEN));
}

#[test]
fn not_found_and_forbidden_are_http_and_not_retried() {
    for code in [404u16, 403, 401, 422] {
        let s = serve(move |_, _| Resp(code, vec![], "denied".into()));
        let e = api(&s).branches("o/r").unwrap_err();
        assert!(
            matches!(e, ApiError::Http(c, _) if c == code),
            "{code}: {e:?}"
        );
        assert_eq!(s.urls().len(), 1, "{code}");
    }
}

#[test]
fn rate_limit_answers_are_classified() {
    let cases: Vec<Resp> = vec![
        Resp(429, vec![], "slow down".into()),
        Resp(403, vec![("x-ratelimit-remaining", "0".into())], "x".into()),
        Resp(403, vec![("retry-after", "30".into())], "x".into()),
        Resp(
            403,
            vec![],
            "You have exceeded a secondary rate limit".into(),
        ),
    ];
    for (i, c) in cases.into_iter().enumerate() {
        let c = Mutex::new(Some(c));
        let s = serve(move |_, _| {
            c.lock()
                .unwrap()
                .take()
                .unwrap_or(Resp(429, vec![], String::new()))
        });
        let e = api(&s).user().unwrap_err();
        assert!(matches!(e, ApiError::RateLimited(_)), "case {i}: {e:?}");
    }
}

#[test]
fn error_text_never_holds_the_token() {
    for code in [500u16, 404, 429, 403] {
        let s = serve(move |r, _| {
            let echo = r.header("authorization").unwrap_or_default().to_string();
            Resp(code, vec![], format!("bad credentials {echo}"))
        });
        let e = api(&s).user().unwrap_err();
        assert!(!format!("{e:?}").contains(TOKEN), "{code}: {e:?}");
    }
}

// ---- local rate reserve (FR-006) ----

#[test]
fn reads_stop_below_the_reserve_while_writes_still_go_out() {
    assert_eq!(RATE_RESERVE, 60);
    let s = serve(|r, _| {
        if r.method == "POST" {
            Resp(201, vec![], "{}".into())
        } else {
            Resp(
                200,
                vec![("x-ratelimit-remaining", "59".into())],
                r#"{"login":"x"}"#.into(),
            )
        }
    });
    let a = api(&s);
    a.user().unwrap();
    let before = s.urls().len();
    assert!(matches!(a.user(), Err(ApiError::RateLimited(_))));
    assert_eq!(
        s.urls().len(),
        before,
        "a refused read must not reach the server"
    );
    a.post_status("o/r", &sha(1), "factory-ci", "pending", "queued", None)
        .unwrap();
    assert_eq!(s.urls().len(), before + 1);
    a.remaining.store(RATE_RESERVE as i64, Ordering::Relaxed);
    assert!(a.user().is_ok());
}

// ---- validation on receipt ----

#[test]
fn a_sha_that_is_not_40_lowercase_hex_is_rejected() {
    for bad in [
        "ABCDEF0123456789ABCDEF0123456789ABCDEF01",
        "abc",
        "zz23456789abcdef0123456789abcdef01234567",
        "",
    ] {
        let body = format!(r#"[{{"name":"main","commit":{{"sha":"{bad}"}}}}]"#);
        let s = serve(move |_, _| ok(&body));
        assert!(api(&s).branches("o/r").is_err(), "{bad:?}");
    }
}

#[test]
fn a_bad_sha_argument_is_refused_before_any_request() {
    let s = serve(|_, _| ok("{}"));
    let a = api(&s);
    assert!(a.tree("o/r", "../x", true).is_err());
    assert!(a.status("o/r", "HEAD").is_err());
    assert!(a
        .post_status("o/r", "main", "factory-ci", "pending", "q", None)
        .is_err());
    assert!(s.urls().is_empty());
}

#[test]
fn a_truncated_tree_is_reported_not_an_error() {
    let s = serve(|_, _| ok(r#"{"tree":[{"path":"a","type":"tree","sha":"s"}],"truncated":true}"#));
    let t = api(&s).tree("o/r", &sha(1), true).unwrap();
    assert!(t.truncated);
    assert_eq!(
        (t.entries[0].path.as_str(), t.entries[0].kind.as_str()),
        ("a", "tree")
    );
}

#[test]
fn the_truncated_tree_fallback_asks_the_root_then_each_top_level_folder() {
    // contracts/github-api.md; the fallback lives in projects::discover
    let c = sha(1);
    let (t1, t2) = (sha(11), sha(12));
    let root = format!(
        r#"{{"truncated":false,"tree":[{{"path":".factory-ci.toml","type":"blob","sha":"{}"}},{{"path":"a","type":"tree","sha":"{t1}"}},{{"path":"b","type":"tree","sha":"{t2}"}}]}}"#,
        sha(13)
    );
    let (t1c, t2c) = (t1.clone(), t2.clone());
    let s = serve(move |r, _| {
        if r.url.ends_with("recursive=1") {
            ok(r#"{"tree":[],"truncated":true}"#)
        } else if r.url.ends_with(&t1c) {
            ok(&format!(
                r#"{{"truncated":false,"tree":[{{"path":".factory-ci.toml","type":"blob","sha":"{}"}}]}}"#,
                sha(14)
            ))
        } else if r.url.ends_with(&t2c) {
            ok(r#"{"truncated":false,"tree":[]}"#)
        } else {
            ok(&root)
        }
    });
    let found = crate::projects::discover(&api(&s), "o/r", &c).unwrap();
    let mut paths: Vec<_> = found.iter().map(|f| f.path.clone()).collect();
    paths.sort();
    assert_eq!(paths, [".", "a"]);
    let urls = s.urls();
    assert_eq!(urls[0], format!("/repos/o/r/git/trees/{c}?recursive=1"));
    assert!(
        urls.contains(&format!("/repos/o/r/git/trees/{c}")),
        "{urls:?}"
    );
    assert!(
        urls.contains(&format!("/repos/o/r/git/trees/{t1}"))
            && urls.contains(&format!("/repos/o/r/git/trees/{t2}"))
    );
    assert_eq!(urls.len(), 4);
}

// ---- post_status (FR-035) ----

#[test]
fn post_status_sends_the_documented_body() {
    let s = serve(|_, _| Resp(201, vec![], "{}".into()));
    let a = api(&s);
    a.post_status(
        "o/r",
        &sha(1),
        "factory-ci/a",
        "success",
        "all steps passed",
        Some("https://x/run/1"),
    )
    .unwrap();
    a.post_status("o/r", &sha(1), "factory-ci", "pending", "queued", None)
        .unwrap();
    let seen = s.seen.lock().unwrap().clone();
    let b0: serde_json::Value = serde_json::from_str(&seen[0].body).unwrap();
    assert_eq!(b0["state"], "success");
    assert_eq!(b0["context"], "factory-ci/a");
    assert_eq!(b0["description"], "all steps passed");
    assert_eq!(b0["target_url"], "https://x/run/1");
    let b1: serde_json::Value = serde_json::from_str(&seen[1].body).unwrap();
    assert!(b1.get("target_url").is_none(), "{b1}");
    assert_eq!(
        seen[0]
            .header("content-type")
            .map(|c| c.starts_with("application/json")),
        Some(true)
    );
}

#[test]
fn post_status_tries_three_times_then_fails_without_the_token() {
    let s = serve(|_, _| Resp(500, vec![], "no".into()));
    let e = api(&s)
        .post_status("o/r", &sha(1), "factory-ci", "pending", "q", None)
        .unwrap_err();
    assert_eq!(s.urls().len(), 3);
    assert!(!format!("{e:?}").contains(TOKEN));
}

#[test]
fn post_status_classifies_a_secondary_limit_as_rate_limited() {
    let s = serve(|_, _| {
        Resp(
            403,
            vec![("retry-after", "0".into())],
            "abuse detection".into(),
        )
    });
    let e = api(&s)
        .post_status("o/r", &sha(1), "factory-ci", "pending", "q", None)
        .unwrap_err();
    assert!(matches!(e, ApiError::RateLimited(_)), "{e:?}");
}

// ---- compose_description (I11) ----

const SUFFIX: &str = " (logs not delivered)";

#[test]
fn short_reasons_pass_through_unchanged() {
    assert_eq!(
        compose_description("all steps passed", true),
        "all steps passed"
    );
    assert_eq!(
        compose_description("all steps passed", false),
        format!("all steps passed{SUFFIX}")
    );
}

#[test]
fn long_reasons_are_shortened_before_the_suffix() {
    let long = "x".repeat(200);
    let d = compose_description(&long, false);
    assert_eq!(d.chars().count(), 140);
    assert!(d.ends_with(SUFFIX));
    assert!(d.contains('…'));
    let d = compose_description(&long, true);
    assert_eq!(d.chars().count(), 140);
    assert!(!d.ends_with(SUFFIX));
}

#[test]
fn description_invariants_hold_for_random_reasons() {
    // I11
    let mut x: u64 = 99;
    for _ in 0..500 {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let len = ((x >> 33) % 300) as usize;
        let ch = ["a", "é", "日", " "][((x >> 20) % 4) as usize];
        let reason = ch.repeat(len);
        for delivered in [true, false] {
            let d = compose_description(&reason, delivered);
            assert!(d.chars().count() <= 140, "{len} {delivered}");
            assert_eq!(d.ends_with(SUFFIX), !delivered, "{len} {delivered}");
        }
    }
}
