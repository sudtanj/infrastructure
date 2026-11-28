//! 012 T011 (test-first, plan step S5a). Targets `hub::ensure_public(base, user, token, namespace, name)
//! -> Result<(), &'static str>` against a `tiny_http` fake Docker Hub (contracts/hub-api.md "Calls", "Mapping",
//! "Rules"). Every Hub answer is scripted per call kind; the fake records each request.
//!
//! Not yet registered: with `pub mod hub;` in `lib.rs`, add `#[cfg(test)] mod hub_tests;` beside it, then remove each
//! `#[ignore = "test-first"]` as its test passes (T012). The login endpoint is T002's call: the fake accepts
//! `POST /v2/auth/token` (`identifier` and `secret`) and the fallback `POST /v2/users/login` (`username` and
//! `password`) alike. Tokens here are obvious fakes.

use crate::hub::ensure_public;
use std::collections::VecDeque;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const USER: &str = "sudtanj";
const TOKEN: &str = "fake-token-AAAA-0123456789";
const JWT: &str = "fake-jwt-BBBB-9876543210";
const CANARY: &str = "canary-Zq81-value-key-0123456789";

const REFUSED_TOKEN: &str = "registry refused the token";
const PRIVATE: &str = "publish refused: repository is private";
const NO_ACCESS: &str = "docker hub api refused repository access";
const NO_CREATE: &str = "docker hub api refused repository creation";
const API: &str = "docker hub api error";
const FIXED: [&str; 5] = [REFUSED_TOKEN, PRIVATE, NO_ACCESS, NO_CREATE, API];

#[derive(Clone, Debug)]
struct Rq {
    method: String,
    url: String,
    auth: String,
    body: String,
}

impl Rq {
    fn kind(&self) -> &'static str {
        let login =
            self.url.starts_with("/v2/auth/token") || self.url.starts_with("/v2/users/login");
        if self.method == "POST" && login {
            "login"
        } else if self.method == "GET" && self.url.starts_with("/v2/repositories/") {
            "probe"
        } else if self.method == "POST" && self.url.trim_end_matches('/') == "/v2/repositories" {
            "create"
        } else {
            "other"
        }
    }
}

type Answer = (u16, String);

#[derive(Clone)]
struct Script {
    login: Answer,
    probes: Vec<Answer>,
    create: Answer,
}

fn a(code: u16, body: &str) -> Answer {
    (code, body.to_string())
}

fn login_ok() -> Answer {
    a(200, &format!("{{\"access_token\":\"{JWT}\"}}"))
}

fn public() -> Answer {
    a(200, "{\"is_private\":false,\"name\":\"hello\"}")
}

fn private() -> Answer {
    a(200, "{\"is_private\":true}")
}

fn missing() -> Answer {
    a(404, "{\"message\":\"object not found\"}")
}

fn script(probes: Vec<Answer>, create: Answer) -> Script {
    Script {
        login: login_ok(),
        probes,
        create,
    }
}

struct Hub {
    base: String,
    reqs: Arc<Mutex<Vec<Rq>>>,
}

impl Hub {
    fn requests(&self) -> Vec<Rq> {
        self.reqs.lock().unwrap().clone()
    }

    fn kinds(&self) -> Vec<&'static str> {
        self.requests().iter().map(Rq::kind).collect()
    }

    fn of(&self, kind: &str) -> Vec<Rq> {
        self.requests()
            .into_iter()
            .filter(|r| r.kind() == kind)
            .collect()
    }
}

/// `delay` holds every answer back (timeout test).
fn serve_slow(s: Script, delay: Duration) -> Hub {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let reqs: Arc<Mutex<Vec<Rq>>> = Arc::default();
    let log = reqs.clone();
    let probes = Arc::new(Mutex::new(VecDeque::from(s.probes.clone())));
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
            let seen = Rq {
                method: rq.method().to_string(),
                url: rq.url().to_string(),
                auth,
                body,
            };
            let (code, text) = match seen.kind() {
                "login" => s.login.clone(),
                "probe" => {
                    let mut q = probes.lock().unwrap();
                    // the last scripted probe repeats; an empty script answers 599
                    if q.len() > 1 {
                        q.pop_front().unwrap()
                    } else {
                        q.front().cloned().unwrap_or_else(|| a(599, ""))
                    }
                }
                "create" => s.create.clone(),
                _ => a(404, "{}"),
            };
            log.lock().unwrap().push(seen);
            std::thread::sleep(delay);
            let json =
                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .unwrap();
            let _ = rq.respond(
                tiny_http::Response::from_string(text)
                    .with_status_code(code)
                    .with_header(json),
            );
        }
    });
    Hub {
        base: format!("http://127.0.0.1:{port}"),
        reqs,
    }
}

fn serve(s: Script) -> Hub {
    serve_slow(s, Duration::ZERO)
}

fn call(h: &Hub) -> Result<(), &'static str> {
    ensure_public(&h.base, USER, TOKEN, "sudtanj", "hello")
}

// ---- login ----

#[test]
#[ignore = "test-first"]
fn login_401_is_a_refused_token_and_nothing_else_is_called() {
    // 012 T011; S5a "login 401"; Mapping
    let mut s = script(vec![public()], a(201, "{}"));
    s.login = a(401, "{\"detail\":\"bad credentials\"}");
    let h = serve(s);
    assert_eq!(call(&h), Err(REFUSED_TOKEN));
    assert_eq!(h.kinds(), ["login"]);
}

#[test]
#[ignore = "test-first"]
fn login_other_failures_are_a_hub_api_error() {
    // 012 T011; "login any other status, network error, timeout, body without access_token"
    for login in [
        a(403, "{}"),
        a(404, "{}"),
        a(429, "{}"),
        a(500, "{}"),
        a(503, "{}"),
        a(200, "not json"),
        a(200, ""),
        a(200, "{}"),
        a(200, "{\"token\":\"x\"}"),
        a(200, "{\"access_token\":7}"),
        a(200, "{\"access_token\":null}"),
        a(200, "[]"),
    ] {
        let mut s = script(vec![public()], a(201, "{}"));
        s.login = login.clone();
        let h = serve(s);
        assert_eq!(call(&h), Err(API), "{login:?}");
        assert_eq!(
            h.kinds(),
            ["login"],
            "{login:?}: nothing after a failed login"
        );
    }
}

#[test]
#[ignore = "test-first"]
fn the_login_request_carries_user_and_token_in_a_json_body() {
    // 012 T011; contracts/hub-api.md "login": POST, JSON, identifier + secret (fallback: username + password)
    let h = serve(script(vec![public()], a(201, "{}")));
    call(&h).unwrap();
    let login = &h.of("login")[0];
    let v: serde_json::Value = serde_json::from_str(&login.body).expect("login body is JSON");
    let (u, t) = if v.get("identifier").is_some() {
        (&v["identifier"], &v["secret"])
    } else {
        (&v["username"], &v["password"])
    };
    assert_eq!((u.as_str(), t.as_str()), (Some(USER), Some(TOKEN)));
    assert!(
        login.auth.is_empty(),
        "no Authorization header on login: {:?}",
        login.auth
    );
}

// ---- probe ----

#[test]
#[ignore = "test-first"]
fn a_public_repository_ends_after_login_and_one_probe() {
    // 012 T011; "probe 200 public does nothing more"
    let h = serve(script(vec![public()], a(201, "{}")));
    assert_eq!(call(&h), Ok(()));
    assert_eq!(h.kinds(), ["login", "probe"]);
    let probe = &h.of("probe")[0];
    assert_eq!(probe.url, "/v2/repositories/sudtanj/hello/");
    assert_eq!(probe.auth, format!("Bearer {JWT}"));
}

#[test]
#[ignore = "test-first"]
fn a_repeat_call_on_a_public_repository_is_idempotent() {
    // 012 T011; "A repeat run finds the repository and does nothing here"
    let h = serve(script(vec![public()], a(201, "{}")));
    assert_eq!(call(&h), Ok(()));
    assert_eq!(call(&h), Ok(()));
    assert!(h.of("create").is_empty());
}

#[test]
#[ignore = "test-first"]
fn a_private_repository_is_refused_and_never_created_or_edited() {
    // 012 T011; "probe 200 private gives `publish refused: repository is private` and no create call"
    let h = serve(script(vec![private()], a(201, "{}")));
    assert_eq!(call(&h), Err(PRIVATE));
    assert_eq!(h.kinds(), ["login", "probe"]);
}

#[test]
#[ignore = "test-first"]
fn probe_401_and_403_are_an_access_error_never_a_bad_token() {
    // 012 T011; "probe 401 or 403 with a valid login gives `docker hub api refused repository access`"
    for code in [401, 403] {
        let h = serve(script(
            vec![a(code, "{\"detail\":\"scope\"}")],
            a(201, "{}"),
        ));
        let r = call(&h);
        assert_eq!(r, Err(NO_ACCESS), "{code}");
        assert_ne!(r, Err(REFUSED_TOKEN));
        assert!(
            h.of("create").is_empty(),
            "{code}: no create after a refused probe"
        );
    }
}

#[test]
#[ignore = "test-first"]
fn other_probe_statuses_and_bodies_are_a_hub_api_error() {
    // 012 T011; "probe any other status", a non-JSON body, and (fail closed) a body without `is_private`
    for probe in [
        a(500, "{}"),
        a(502, "{}"),
        a(429, "{}"),
        a(400, "{}"),
        a(200, "not json"),
        a(200, ""),
        a(200, "{\"name\":\"hello\"}"),
        a(200, "{\"is_private\":\"false\"}"),
        a(200, "{\"is_private\":null}"),
    ] {
        let h = serve(script(vec![probe.clone()], a(201, "{}")));
        let r = call(&h);
        assert_eq!(r, Err(API), "{probe:?}");
        assert!(
            h.of("create").is_empty(),
            "{probe:?}: never create on an unclear probe"
        );
    }
}

// ---- create ----

#[test]
#[ignore = "test-first"]
fn a_missing_repository_is_created_public_then_probed() {
    // 012 T011; "probe 404 then create 201 then a probe that shows public succeeds"
    let h = serve(script(vec![missing(), public()], a(201, "{}")));
    assert_eq!(call(&h), Ok(()));
    assert_eq!(h.kinds(), ["login", "probe", "create", "probe"]);
    let create = &h.of("create")[0];
    assert_eq!(create.auth, format!("Bearer {JWT}"));
    let v: serde_json::Value = serde_json::from_str(&create.body).unwrap();
    assert_eq!(v["namespace"], "sudtanj");
    assert_eq!(v["name"], "hello");
    assert_eq!(v["is_private"], serde_json::Value::Bool(false));
}

#[test]
#[ignore = "test-first"]
fn a_create_race_400_or_409_is_followed_by_a_probe() {
    // 012 T011; "create answered 400 or 409 (a race) is followed by a probe and succeeds when public"
    for code in [400, 409] {
        let h = serve(script(
            vec![missing(), public()],
            a(code, "{\"detail\":\"exists\"}"),
        ));
        assert_eq!(call(&h), Ok(()), "{code}");
        assert_eq!(h.kinds(), ["login", "probe", "create", "probe"], "{code}");
    }
}

#[test]
#[ignore = "test-first"]
fn a_create_race_that_ends_private_or_missing_fails() {
    // 012 T011: the second probe decides; mapping of "create 201 / 400 / 409"
    for code in [201, 400, 409] {
        let h = serve(script(vec![missing(), private()], a(code, "{}")));
        assert_eq!(call(&h), Err(PRIVATE), "{code} then private");
        let h = serve(script(vec![missing(), missing()], a(code, "{}")));
        assert_eq!(call(&h), Err(API), "{code} then still missing");
        let h = serve(script(vec![missing(), a(500, "{}")], a(code, "{}")));
        assert_eq!(call(&h), Err(API), "{code} then 500");
    }
}

#[test]
#[ignore = "test-first"]
fn create_401_and_403_are_a_creation_error() {
    // 012 T011; "create 401 or 403 gives `docker hub api refused repository creation`"
    for code in [401, 403] {
        let h = serve(script(
            vec![missing(), public()],
            a(code, "{\"detail\":\"scope\"}"),
        ));
        let r = call(&h);
        assert_eq!(r, Err(NO_CREATE), "{code}");
        assert_ne!(r, Err(REFUSED_TOKEN));
        assert_eq!(
            h.kinds(),
            ["login", "probe", "create"],
            "{code}: no probe after a refused create"
        );
    }
}

#[test]
#[ignore = "test-first"]
fn create_with_any_other_status_is_a_hub_api_error() {
    // 012 T011; "create any other status"
    for code in [500, 502, 503, 404, 422, 429] {
        let h = serve(script(vec![missing(), public()], a(code, "{}")));
        assert_eq!(call(&h), Err(API), "{code}");
        assert_eq!(h.kinds(), ["login", "probe", "create"], "{code}");
    }
}

// ---- network, timeout, base ----

#[test]
#[ignore = "test-first"]
fn a_refused_connection_is_a_hub_api_error() {
    // 012 T011; "network error"
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let r = ensure_public(
        &format!("http://127.0.0.1:{port}"),
        USER,
        TOKEN,
        "sudtanj",
        "hello",
    );
    assert_eq!(r, Err(API));
}

#[test]
#[ignore = "test-first"]
fn a_server_that_drops_the_connection_is_a_hub_api_error() {
    // 012 T011
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for c in l.incoming() {
            drop(c);
        }
    });
    let r = ensure_public(
        &format!("http://127.0.0.1:{port}"),
        USER,
        TOKEN,
        "sudtanj",
        "hello",
    );
    assert_eq!(r, Err(API));
}

#[test]
#[ignore = "test-first"]
fn a_call_that_never_answers_times_out_as_a_hub_api_error() {
    // 012 T011; "a timeout"; 30-second limit per call (this test takes about 30 seconds)
    let h = serve_slow(
        script(vec![public()], a(201, "{}")),
        Duration::from_secs(40),
    );
    let t = Instant::now();
    assert_eq!(call(&h), Err(API));
    let took = t.elapsed();
    assert!(
        took >= Duration::from_secs(20),
        "gave up too early: {took:?}"
    );
    assert!(took < Duration::from_secs(38), "no timeout: {took:?}");
}

#[test]
#[ignore = "test-first"]
fn a_base_that_is_not_https_or_loopback_http_is_refused() {
    // 012 T011; "`base` other than `https://` or `http://127.0.0.1` is refused"
    for base in [
        "http://hub.docker.com",
        "http://example.com",
        "http://localhost:1",
        "http://127.0.0.2:1",
        "http://[::1]:1",
        "ftp://127.0.0.1",
        "file:///etc/passwd",
        "hub.docker.com",
        "127.0.0.1:1",
        "",
        "HTTP://example.com",
    ] {
        let r = ensure_public(base, USER, TOKEN, "sudtanj", "hello");
        assert!(r.is_err(), "{base:?} accepted: {r:?}");
    }
}

#[test]
#[ignore = "test-first"]
fn a_loopback_prefix_trick_is_refused_without_a_request() {
    // 012 T011: `starts_with("http://127.0.0.1")` alone would let these through; the fake would see the call
    let h = serve(script(vec![public()], a(201, "{}")));
    let port = h.base.rsplit(':').next().unwrap().to_string();
    for base in [
        format!("http://127.0.0.1@localhost:{port}"),
        format!("http://127.0.0.1.localhost:{port}"),
        format!("http://127.0.0.1:{port}@localhost:{port}"),
        format!("http://127.0.0.1\\@localhost:{port}"),
    ] {
        let r = ensure_public(&base, USER, TOKEN, "sudtanj", "hello");
        assert!(r.is_err(), "{base:?} accepted: {r:?}");
    }
    assert!(h.requests().is_empty(), "{:?}", h.requests());
}

#[test]
#[ignore = "test-first"]
fn loopback_http_with_any_port_is_accepted() {
    // 012 T011: the override exists for tests only, and these tests rely on it
    let h = serve(script(vec![public()], a(201, "{}")));
    assert!(h.base.starts_with("http://127.0.0.1:"));
    assert_eq!(call(&h), Ok(()));
}

// ---- secrets and bodies ----

#[test]
#[ignore = "test-first"]
fn the_token_is_only_in_the_login_body() {
    // 012 T011; "the request bodies carry the token only in the login call"; no URL, no header
    let h = serve(script(vec![missing(), public()], a(201, "{}")));
    call(&h).unwrap();
    let all = h.requests();
    assert_eq!(all.len(), 4);
    for r in &all {
        assert!(!r.url.contains(TOKEN), "{} {}", r.method, r.url);
        assert!(!r.auth.contains(TOKEN), "{} Authorization", r.kind());
        if r.kind() == "login" {
            assert!(r.body.contains(TOKEN));
        } else {
            assert!(!r.body.contains(TOKEN), "{} body", r.kind());
        }
    }
    let with_token = all.iter().filter(|r| r.body.contains(TOKEN)).count();
    assert_eq!(with_token, 1);
}

#[test]
#[ignore = "test-first"]
fn only_the_access_token_field_is_used_and_only_is_private_decides() {
    // 012 T011; "only the `access_token` and `is_private` fields are read"
    let mut s = script(
        vec![a(
            200,
            "{\"is_private\":false,\"private\":true,\"name\":\"is_private\"}",
        )],
        a(201, "{}"),
    );
    s.login = a(
        200,
        &format!(
            "{{\"token\":\"decoy-jwt\",\"access_token\":\"{JWT}\",\"refresh_token\":\"decoy-2\"}}"
        ),
    );
    let h = serve(s);
    assert_eq!(call(&h), Ok(()));
    assert_eq!(h.of("probe")[0].auth, format!("Bearer {JWT}"));
}

#[test]
#[ignore = "test-first"]
fn no_response_body_reaches_a_returned_string() {
    // 012 T011; "response bodies never reach a returned string"; every error is one of the fixed texts
    let body =
        format!("{{\"message\":\"{CANARY}\",\"detail\":\"{TOKEN}\",\"access_token\":\"{JWT}\"}}");
    let junk = a(500, &body);
    let mut scenarios: Vec<Script> = vec![];
    for login in [
        a(401, &body),
        a(403, &body),
        a(500, &body),
        a(200, &body.replace("\"access_token\":\"", "\"x\":\"")),
    ] {
        let mut s = script(vec![public()], junk.clone());
        s.login = login;
        scenarios.push(s);
    }
    for probe in [
        a(401, &body),
        a(403, &body),
        a(500, &body),
        a(200, &body),
        a(200, &format!("{CANARY} not json")),
    ] {
        scenarios.push(script(vec![probe], junk.clone()));
    }
    for create in [
        a(401, &body),
        a(403, &body),
        a(500, &body),
        a(400, &body),
        a(409, &body),
    ] {
        scenarios.push(script(vec![missing(), a(404, &body)], create));
    }
    scenarios.push(script(vec![missing(), a(200, &body)], a(201, &body)));
    for (i, s) in scenarios.into_iter().enumerate() {
        let h = serve(s);
        if let Err(e) = call(&h) {
            assert!(FIXED.contains(&e), "scenario {i}: {e:?}");
            for secret in [CANARY, TOKEN, JWT, "message", "detail"] {
                assert!(!e.contains(secret), "scenario {i}: {e:?}");
            }
        }
    }
}

// ---- redirects ----

#[test]
#[ignore = "test-first"]
fn a_redirect_never_carries_the_token_or_the_access_token_elsewhere() {
    // 012 T011 (FR-015): a hostile or broken Hub answer must not move credentials to another server
    let decoy = serve(script(vec![public()], a(201, "{}")));
    for code in [301, 302, 303, 307, 308] {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let target = format!("{}/v2/auth/token", decoy.base);
        std::thread::spawn(move || {
            for mut rq in server.incoming_requests() {
                let mut sink = String::new();
                let _ = rq.as_reader().read_to_string(&mut sink);
                let loc =
                    tiny_http::Header::from_bytes(&b"Location"[..], target.as_bytes()).unwrap();
                let _ = rq.respond(tiny_http::Response::empty(code as u16).with_header(loc));
            }
        });
        let r = ensure_public(
            &format!("http://127.0.0.1:{port}"),
            USER,
            TOKEN,
            "sudtanj",
            "hello",
        );
        assert!(r.is_err(), "{code}: a redirect is not a login");
    }
    for r in decoy.requests() {
        assert!(
            !r.body.contains(TOKEN) && !r.auth.contains(TOKEN) && !r.auth.contains(JWT),
            "{r:?}"
        );
    }
}
