//! Makes sure the Docker Hub repository `<namespace>/<name>` exists and is public, with `ureq`
//! (contracts/hub-api.md). The token is only in the login body; the access token it returns is only in
//! the `Authorization` header of the next calls. No response body is returned or logged: every
//! error is one fixed text.

use serde_json::{json, Value};
use std::io::Read;
use std::time::Duration;

const CALL_LIMIT: Duration = Duration::from_secs(30);
const BODY_MAX: u64 = 1 << 20;

const REFUSED_TOKEN: &str = "registry refused the token";
const PRIVATE: &str = "publish refused: repository is private";
const NO_ACCESS: &str = "docker hub api refused repository access";
const NO_CREATE: &str = "docker hub api refused repository creation";
const API: &str = "docker hub api error";

enum Repo {
    Public,
    Private,
    Missing,
}

/// Only `https://` or `http://127.0.0.1` (a port may follow, nothing else: no userinfo, no longer host).
fn base_ok(base: &str) -> bool {
    match base.strip_prefix("http://127.0.0.1") {
        Some(rest) => rest.is_empty() || rest.strip_prefix(':').is_some_and(port_ok),
        None => base.starts_with("https://"),
    }
}

fn port_ok(p: &str) -> bool {
    !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())
}

/// Status and body of one call; `None` on a network error, a timeout or a refused connection.
/// Redirects are not followed, so no credential moves to another host.
fn send(req: ureq::Request, body: Option<Value>) -> Option<(u16, String)> {
    let r = match body {
        Some(b) => req
            .set("Content-Type", "application/json")
            .send_string(&b.to_string()),
        None => req.call(),
    };
    let resp = match r {
        Ok(r) | Err(ureq::Error::Status(_, r)) => r,
        Err(_) => return None,
    };
    let code = resp.status();
    let mut text = String::new();
    let _ = resp.into_reader().take(BODY_MAX).read_to_string(&mut text);
    Some((code, text))
}

fn probe(
    agent: &ureq::Agent,
    base: &str,
    jwt: &str,
    namespace: &str,
    name: &str,
) -> Result<Repo, &'static str> {
    let req = agent
        .get(&format!("{base}/v2/repositories/{namespace}/{name}/"))
        .set("Authorization", &format!("Bearer {jwt}"));
    match send(req, None).ok_or(API)? {
        (200, text) => {
            match serde_json::from_str::<Value>(&text).map(|v| v["is_private"].as_bool()) {
                Ok(Some(false)) => Ok(Repo::Public),
                Ok(Some(true)) => Ok(Repo::Private),
                _ => Err(API),
            }
        }
        (404, _) => Ok(Repo::Missing),
        (401 | 403, _) => Err(NO_ACCESS),
        _ => Err(API),
    }
}

/// Public repository: `Ok`. Private: refused. Missing: created public, then probed again.
pub fn ensure_public(
    base: &str,
    user: &str,
    token: &str,
    namespace: &str,
    name: &str,
) -> Result<(), &'static str> {
    if !base_ok(base) {
        return Err(API);
    }
    let base = base.trim_end_matches('/');
    let agent = ureq::AgentBuilder::new()
        .timeout(CALL_LIMIT)
        .redirects(0)
        .build();
    let login = agent.post(&format!("{base}/v2/auth/token"));
    let body = json!({"identifier": user, "secret": token});
    let jwt = match send(login, Some(body)).ok_or(API)? {
        (200, text) => serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v["access_token"].as_str().map(str::to_string))
            .ok_or(API)?,
        (401, _) => return Err(REFUSED_TOKEN),
        _ => return Err(API),
    };
    let check = |repo: Repo| match repo {
        Repo::Public => Ok(()),
        Repo::Private => Err(PRIVATE),
        Repo::Missing => Err(API),
    };
    match probe(&agent, base, &jwt, namespace, name)? {
        Repo::Missing => {}
        repo => return check(repo),
    }
    let create = agent
        .post(&format!("{base}/v2/repositories/"))
        .set("Authorization", &format!("Bearer {jwt}"));
    let body = json!({"namespace": namespace, "name": name, "is_private": false});
    match send(create, Some(body)).ok_or(API)?.0 {
        201 | 400 | 409 => check(probe(&agent, base, &jwt, namespace, name)?),
        401 | 403 => Err(NO_CREATE),
        _ => Err(API),
    }
}
