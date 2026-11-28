//! The built-in publish step of a passing `docker/<image>` row on `main` (contracts/publish-step.md).
//! The token comes from the environment, goes to `crane` on standard input and nowhere else, and no
//! error text holds it: every way out is one fixed line.

use crate::archive::{self, ImageConfig};
use crate::engine::Engine;
use crate::github::Api;
use crate::hub;
use crate::image::{self, NAMESPACE, PLATFORMS, REGISTRY};
use crate::job::{done, temp_base, var, Done, Env, Target};
use crate::util::random_id;
use serde_json::Value;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A call that would start with less time left than this is not made.
const MIN_LEFT: Duration = Duration::from_secs(10);
const CONTAINER_LIMIT: Duration = Duration::from_secs(10 * 60);

const OCI_INDEX: &str = "application/vnd.oci.image.index.v1+json";
const DOCKER_LIST: &str = "application/vnd.docker.distribution.manifest.list.v2+json";
const SHELLS: [&str; 6] = ["sh", "bash", "dash", "ash", "busybox", "zsh"];

const NO_TOKEN: &str = "publish refused: no token";
const NO_LOGIN: &str = "publish refused: no login name";
const NOT_NAMESPACE: &str = "publish refused: login is not the namespace";
const LEFT_OVER: &str = "publish refused: sidecar still running";
const TIME: &str = "publish refused: time ran out";
const MISSING: &str = "publish refused: archive missing";
const BIG: &str = "publish refused: archive too large";
const UNREADABLE: &str = "publish refused: archive unreadable";
const ROOT: &str = "publish refused: image runs as root";
const SHELL: &str = "publish refused: image has a shell entrypoint";
const PLATFORM: &str = "publish refused: image platform mismatch";
const TIP: &str = "tip of main unreadable";
const REFUSED_TOKEN: &str = "registry refused the token";
const INCOMPLETE: &str = "image index incomplete";
const PUSH_FAILED: &str = "push failed";

/// The login, then `body`. Exit 41 is a refused credential, 42 any other registry failure, 43 a digest failure.
fn script(body: &str) -> String {
    format!(
        "fail() {{ case \"$1\" in *UNAUTHORIZED*|*DENIED*) exit 41;; esac; exit 42; }}\n\
         \"$CRANE\" auth login {REGISTRY} -u {NAMESPACE} --password-stdin >/dev/null 2>&1 || exit 41\n\
         {body}"
    )
}

/// Reuses a list that exists for the tag and pushes nothing; else pushes each archive by digest and
/// appends one list. Prints the list.
const SCRIPT_A: &str = r#"if ! m=$("$CRANE" manifest "$REPO:$TAG" 2>&1); then
  case "$m" in
    *MANIFEST_UNKNOWN*|*NAME_UNKNOWN*) ;;
    *UNAUTHORIZED*|*DENIED*) exit 41;;
    *) exit 42;;
  esac
  for a in amd64 arm64; do
    d=$("$CRANE" digest --tarball "/in/image-$a.tar" 2>/dev/null) || exit 43
    o=$("$CRANE" push "/in/image-$a.tar" "$REPO@$d" 2>&1) || fail "$o"
    eval "d_$a=\$d"
  done
  o=$("$CRANE" index append -t "$REPO:$TAG" -m "$REPO@$d_amd64" -m "$REPO@$d_arm64" 2>&1) || fail "$o"
fi
m=$("$CRANE" manifest "$REPO:$TAG") || exit 42
echo "INDEX $(printf %s "$m" | tr -d '\n')"
"#;

const SCRIPT_B: &str = r#"o=$("$CRANE" tag "$REPO:$TAG" latest 2>&1) || fail "$o"
"#;

/// `check failed: <ids>` (at most three) when the project folder breaks a rule.
pub(crate) fn check_failed(dir: &Path) -> Option<Done> {
    let bad = image::check(dir);
    if bad.is_empty() {
        return None;
    }
    let shown = bad[..bad.len().min(3)].join(", ");
    Some(done("failure", format!("check failed: {shown}")))
}

/// Exactly one `linux/amd64` and one `linux/arm64` entry, each with a `sha256:` digest, in an OCI index
/// or a Docker manifest list. Anything else, such as a single image manifest, fails.
pub fn verify_index(json: &str) -> Result<(), &'static str> {
    let v: Value = serde_json::from_str(json).map_err(|_| INCOMPLETE)?;
    let media_ok = matches!(v["mediaType"].as_str(), Some(OCI_INDEX | DOCKER_LIST));
    let Some([a, b]) = v["manifests"].as_array().map(Vec::as_slice) else {
        return Err(INCOMPLETE);
    };
    let hex = |c: u8| matches!(c, b'0'..=b'9' | b'a'..=b'f');
    let digest_ok = |e: &Value| {
        let d = e["digest"].as_str().unwrap_or_default();
        let h = d.strip_prefix("sha256:").unwrap_or_default();
        h.len() == 64 && h.bytes().all(hex)
    };
    let is = |e: &Value, arch: &str| {
        let p = &e["platform"];
        p["os"] == "linux" && p["architecture"] == arch
    };
    // Two entries: exactly one of them has the architecture.
    let one = |arch: &str| is(a, arch) != is(b, arch);
    if media_ok && digest_ok(a) && digest_ok(b) && one("amd64") && one("arm64") {
        Ok(())
    } else {
        Err(INCOMPLETE)
    }
}

fn err(text: &str) -> Done {
    done("error", text)
}

/// Remaining time of the job, or the fixed refusal when it is below `MIN_LEFT`.
fn left(deadline: Instant) -> Result<Duration, Done> {
    let left = deadline.saturating_duration_since(Instant::now());
    (left >= MIN_LEFT).then_some(left).ok_or_else(|| err(TIME))
}

/// A numeric `uid` or `uid:gid` with a uid other than 0.
fn user_ok(user: &str) -> bool {
    let (uid, gid) = user
        .split_once(':')
        .map_or((user, None), |(u, g)| (u, Some(g)));
    let num = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    num(uid) && gid.is_none_or(num) && uid.bytes().any(|b| b != b'0')
}

/// data-model.md "Archive config": the policy `read_config` leaves to us.
fn policy(arch: &str, c: &ImageConfig) -> Result<(), Done> {
    if c.os != "linux" || c.architecture != arch {
        return Err(err(PLATFORM));
    }
    if !user_ok(&c.user) {
        return Err(err(ROOT));
    }
    let first = c.entrypoint.first().or(c.cmd.first());
    let base = first.and_then(|f| f.rsplit('/').next()).unwrap_or_default();
    if SHELLS.contains(&base) {
        return Err(err(SHELL));
    }
    Ok(())
}

/// A new directory under the temp base, mode 0700, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(env: Env, run_id: &str) -> Result<Scratch, Done> {
        let base = temp_base(env);
        let p = base.join(format!("fci-pub-{run_id}-{}", random_id()));
        // The path goes into a `-v` argument, where `:` separates fields.
        if p.to_str().is_none_or(|s| s.contains(':')) {
            return Err(err(UNREADABLE));
        }
        let made = std::fs::DirBuilder::new().mode(0o700).create(&p);
        made.map(|()| Scratch(p)).map_err(|_| err(UNREADABLE))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Checks that each archive in `dir` is a regular file of a fitting size, and renames it into `to` at once.
/// The files stay in `to`: nothing can swap them after the check.
fn move_archives(dir: &Path, to: &Path) -> Result<(), Done> {
    for (_, file) in PLATFORMS {
        let from = dir.join(file);
        let m = match std::fs::symlink_metadata(&from) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(err(MISSING)),
            Err(_) => return Err(err(UNREADABLE)),
        };
        if !m.is_file() || m.len() == 0 {
            return Err(err(UNREADABLE));
        }
        if m.len() > image::ARCHIVE_MAX {
            return Err(err(BIG));
        }
        std::fs::rename(&from, to.join(file)).map_err(|_| err(UNREADABLE))?;
    }
    Ok(())
}

/// The commit at the tip of `main`.
fn tip(api: &dyn Api, repo: &str) -> Result<String, Done> {
    let branches = api.branches(repo).map_err(|_| err(TIP))?;
    let main = branches
        .into_iter()
        .find(|b| b.name == image::PUBLISH_BRANCH);
    main.map(|b| b.sha).ok_or_else(|| err(TIP))
}

/// Flags of every publish container: no capability, no new privilege, nothing writable but `/tmp`.
const HARDEN: [&str; 11] = [
    "--read-only",
    "--tmpfs",
    "/tmp:rw,size=64m",
    "--cap-drop",
    "ALL",
    "--security-opt",
    "no-new-privileges",
    "--pids-limit",
    "256",
    "--memory",
    "1g",
];

/// One publish container running `body`. The token is its standard input; the arguments and the
/// environment hold none. It runs as the owner of `scratch`, which `mount` gives it (container A).
fn crane(
    engine: &Engine,
    run_id: &str,
    (kind, body): (&str, &str),
    (scratch, mount): (&Path, bool),
    envs: &[String],
    token: &str,
    limit: Duration,
) -> Result<String, Done> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<String>>();
    let name = format!("factory-ci-{run_id}-{kind}");
    let label = format!("{}={run_id}", crate::engine::LABEL);
    let owner = std::fs::metadata(scratch).map_err(|_| err(UNREADABLE))?;
    let user = format!("{}:{}", owner.uid(), owner.gid());
    let mut a = s(&["run", "-i", "--rm", "--name", &name, "--label", &label]);
    a.extend(HARDEN.map(String::from));
    a.extend(s(&["--user", &user]));
    if mount {
        a.extend(s(&["-v", &format!("{}:/in:ro", scratch.display())]));
    }
    for e in envs {
        a.extend(s(&["-e", e]));
    }
    a.extend(s(&["--entrypoint", image::CRANE_SH, image::CRANE_IMAGE]));
    a.extend(s(&["-c", &script(body)]));
    let out = engine.capture_in(&a, &[], token.as_bytes(), limit, Some(&name));
    // `capture_in` words a failed exit as `exit status: <n>: <stderr>`; the stderr stays in this match.
    out.map_err(|e| {
        let refused = e.starts_with("exit status: 41:");
        err(if refused { REFUSED_TOKEN } else { PUSH_FAILED })
    })
}

fn go(
    engine: &Engine,
    env: Env,
    api: &dyn Api,
    t: &Target,
    dir: &Path,
    deadline: Instant,
    hub_base: &str,
) -> Result<String, Done> {
    if let Some(d) = check_failed(dir) {
        return Err(d);
    }
    let token = var(env, image::TOKEN_NAME).ok_or_else(|| err(NO_TOKEN))?;
    let login = var(env, image::USERNAME_NAME).ok_or_else(|| err(NO_LOGIN))?;
    if login != NAMESPACE {
        return Err(err(NOT_NAMESPACE));
    }
    // The name goes into URLs and a registry path.
    let name = t.project.strip_prefix("docker/").unwrap_or_default();
    if !image::name_ok(name) {
        return Err(done("failure", "check failed: layout"));
    }

    left(deadline)?;
    engine.cleanup_run(&t.run_id);
    if engine.run_left(&t.run_id) {
        return Err(err(LEFT_OVER));
    }
    // The sidecar is gone, so nothing else can touch the archives: check each, move it, then read it.
    let scratch = Scratch::new(env, &t.run_id)?;
    move_archives(dir, &scratch.0)?;
    for (arch, file) in PLATFORMS {
        let read = archive::read_config(&scratch.0.join(file));
        policy(arch, &read.map_err(|_| err(UNREADABLE))?)?;
    }

    left(deadline)?;
    let first = tip(api, &t.repo)?;
    let remaining = left(deadline)?;
    let ensured = hub::ensure_public(hub_base, &login, &token, NAMESPACE, name, remaining);
    ensured.map_err(err)?;

    let tag = format!("sha-{}", t.sha.get(..image::TAG_HEX).unwrap_or(&t.sha));
    let envs = [
        "HOME=/tmp".to_string(),
        "DOCKER_CONFIG=/tmp/.docker".to_string(),
        format!("CRANE={}", image::CRANE_BIN),
        format!("REPO={REGISTRY}/{NAMESPACE}/{name}"),
        format!("TAG={tag}"),
    ];
    let run = |kb, dirs, limit| crane(engine, &t.run_id, kb, dirs, &envs, &token, limit);
    let limit = left(deadline)?.min(CONTAINER_LIMIT);
    let out = run(("publish", SCRIPT_A), (scratch.0.as_path(), true), limit)?;
    let list = out.lines().rev().find_map(|l| l.strip_prefix("INDEX "));
    list.map(verify_index)
        .unwrap_or(Err(INCOMPLETE))
        .map_err(err)?;

    let not_latest = format!("published {tag}, not latest");
    if first != t.sha {
        return Ok(not_latest);
    }
    left(deadline)?;
    if tip(api, &t.repo)? != t.sha {
        return Ok(not_latest);
    }
    let limit = left(deadline)?.min(CONTAINER_LIMIT);
    run(("tag", SCRIPT_B), (scratch.0.as_path(), false), limit)?;
    Ok(format!("published latest and {tag}"))
}

/// Pushes the two archives of `dir` as one list tagged `sha-<12 hex>` and, when `main` still points at the
/// commit, tags it `latest`. Called only for a passing `docker/<image>` row on `main`, with the sidecar still up.
pub(crate) fn publish(
    engine: &Engine,
    env: Env,
    api: &dyn Api,
    t: &Target,
    dir: &Path,
    deadline: Instant,
    hub_base: &str,
) -> Done {
    match go(engine, env, api, t, dir, deadline, hub_base) {
        Ok(text) => done("success", text),
        Err(d) => d,
    }
}
