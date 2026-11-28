//! 012 T019, T021, T023, T025 and S6/S7. Binary-level publish tests: the real `rust-ci-runner job` with a stub
//! `docker` on PATH, a fake GitHub (statuses and `branches`) and a fake Docker Hub.
//!
//! The publish step cannot be called directly, because the plan does not pin the signature of
//! `publish::publish` (the contract names `(engine, env, api, target, dir, log) -> Done`, and `Done` and `Target`
//! are private to `job.rs`). So the S6 acceptance list runs here, through `job`, and each test names its S6 line.
//!
//! The stub `docker` below is this file's own: it does not touch `tests/common/mod.rs`. It runs in two modes.
//! Canned (default): the publish container prints `STUB_A_OUT` and exits `STUB_A_EXIT` or `STUB_B_EXIT`. Emulated
//! (`STUB_EMULATE=1`): it runs the real `-c` script of the container under `/bin/sh` with a fake `crane` that keeps a
//! registry in a directory, so script A and script B are exercised end to end (reuse, no overwrite, exit codes).
//! Both modes save the standard input of each publish container to `stdin-<n>-<A|B>`.
//!
//! One assumption the plan leaves open (reported to the lead): `publish_stops_when_the_deadline_is_near` needs a
//! way to shorten `JOB_DEADLINE`; it reads `FACTORY_CI_JOB_DEADLINE_SECS`, a debug-build-only variable like
//! `FACTORY_CI_HUB_URL`. Change that one name if the engineer picks another seam.
//!
//! Secrets here are fakes: the token is the canary of `tasks.md`.

mod common;
use common::*;
use rust_ci_runner::util::base64;
use std::collections::VecDeque;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

const TOKEN: &str = "canary-Zq81-value-key-0123456789";
const USER: &str = "sudtanj";
const JWT: &str = "fake-jwt-BBBB-9876543210";
const OTHER: &str = "fedcba9876543210fedcba9876543210fedcba98";
const TAG: &str = "sha-0123456789ab";
const REPO: &str = "index.docker.io/sudtanj/img";
const CRANE_SH: &str = "/busybox/sh";
const CRANE_BIN: &str = "/ko-app/crane";
const CRANE_IMAGE_PREFIX: &str = "gcr.io/go-containerregistry/crane:debug@sha256:";
const OCI: &str = "application/vnd.oci.image.index.v1+json";
const DOCKER_LIST: &str = "application/vnd.docker.distribution.manifest.list.v2+json";
const DOCKERFILE: &str = include_str!("fixtures/image/pass/Dockerfile");

/// The step of an image folder: the contract template (`image::check` accepts no other commands).
const TOML: &str = "[steps.build]\n\
    image = \"docker:27.5.1-cli@sha256:851f91d241214e7c6db86513b270d58776379aacc5eb9c4a87e5b47115e3065c\"\n\
    docker = true\n\
    commands = [\n\
      \"docker build --platform linux/amd64 -t image-amd64 .\",\n\
      \"docker save -o image-amd64.tar image-amd64\",\n\
      \"docker build --platform linux/arm64 -t image-arm64 .\",\n\
      \"docker save -o image-arm64.tar image-arm64\",\n\
      \"chmod 0644 image-amd64.tar image-arm64.tar\",\n\
    ]\n\
    timeout = 1800\n";

/// The same template with a one-second step timeout.
const SLOW_TOML: &str = "[steps.build]\n\
    image = \"docker:27.5.1-cli@sha256:851f91d241214e7c6db86513b270d58776379aacc5eb9c4a87e5b47115e3065c\"\n\
    docker = true\n\
    commands = [\n\
      \"docker build --platform linux/amd64 -t image-amd64 .\",\n\
      \"docker save -o image-amd64.tar image-amd64\",\n\
      \"docker build --platform linux/arm64 -t image-arm64 .\",\n\
      \"docker save -o image-arm64.tar image-arm64\",\n\
      \"chmod 0644 image-amd64.tar image-arm64.tar\",\n\
    ]\n\
    timeout = 1\n";

/// A plain project (not an image folder) with one step.
const PLAIN_TOML: &str = "[steps.build]\n\
    image = \"alpine@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n\
    commands = [\"echo build\"]\n\
    timeout = 1800\n";

/// The docker stand-in. Settings come from the runner's own environment (the parent), which the runner
/// clears for its children.
const PUB_STUB: &str = r##"#!/bin/sh
get() { tr '\0' '\n' < "/proc/$PPID/environ" | sed -n "s/^$1=//p" | head -n 1; }
d=$(get STUB_DIR)
reg=$(get FAKE_REG)
printf '%s\n' "$@" >> "$d/calls"
echo -- >> "$d/calls"
/usr/bin/env >> "$d/envs"
echo -- >> "$d/envs"
order() { echo "$1" >> "$d/order"; }
name=""; mnt=""; ws=""; prev=""
for a in "$@"; do
  [ "$prev" = "--name" ] && name="$a"
  if [ "$prev" = "-v" ]; then
    case "$a" in
      *:/in:ro) mnt="${a%:/in:ro}";;
      *:/workspace) ws="${a%:/workspace}";;
    esac
  fi
  prev="$a"
done
case "$name" in
  *-clone)
    order run-clone
    echo "stub clone output"
    [ -n "$(get STUB_CLONE_FAIL)" ] && exit 1
    fx=$(get STUB_FIXTURE)
    [ -n "$fx" ] && cp -a "$fx/." "$ws/"
    exit 0;;
  *-publish|*-tag)
    kind=A
    case "$name" in *-tag) kind=B;; esac
    n=$(ls "$d" | grep -c '^stdin-')
    cat > "$d/stdin-$n-$kind"
    printf '%s\0' "$@" > "$d/argv-$n-$kind"
    order "run-$kind"
    [ -n "$mnt" ] && ls -A "$mnt" > "$d/in-$kind"
    if [ -n "$(get STUB_EMULATE)" ]; then
      script=""; prev=""
      for a in "$@"; do
        [ "$prev" = "-c" ] && script="$a"
        prev="$a"
      done
      prev=""
      for a in "$@"; do
        [ "$prev" = "-e" ] && export "$a"
        prev="$a"
      done
      echo "$CRANE" > "$d/crane-path"
      CRANE=$(get FAKE_CRANE)
      FAKE_REG="$reg"
      MNT="$mnt"
      export CRANE FAKE_REG MNT
      for v in FAKE_LOGIN_FAIL FAKE_MANIFEST_ERR FAKE_PUSH_ERR FAKE_DIGEST_FAIL FAKE_TAG_ERR; do
        eval "$v=\$(get $v)"
        export "$v"
      done
      /bin/sh -c "$script" < "$d/stdin-$n-$kind"
      exit $?
    fi
    if [ "$kind" = A ]; then
      out=$(get STUB_A_OUT)
      [ -n "$out" ] && printf '%s\n' "$out"
      code=$(get STUB_A_EXIT)
    else
      code=$(get STUB_B_EXIT)
    fi
    exit "${code:-0}";;
esac
case "$1" in
  ps) order ps; [ -n "$(get STUB_LEFT)" ] && echo deadbeef;;
  network|volume) order "$1";;
  run)
    order run-step
    s=$(get STUB_STEP_SLEEP)
    [ -n "$s" ] && sleep "$s"
    r=$(get STUB_REWRITE)
    [ -n "$r" ] && [ -n "$ws" ] && echo "ADD . /x" >> "$ws/$r"
    big=$(get STUB_BIG)
    b=$(get STUB_BUILT)
    [ -d "$b" ] && [ -n "$ws" ] && cp -a "$b/docker/img/." "$ws/"
    [ -n "$big" ] && [ -n "$ws" ] && truncate -s "$big" "$ws/image-amd64.tar"
    o=$(get STUB_OUT)
    [ -n "$o" ] && echo "$o";;
esac
f=$(get STUB_FAIL)
if [ -n "$f" ]; then
  case "$*" in *"$f"*) c=$(get STUB_FAIL_CODE); exit "${c:-1}";; esac
fi
exit 0
"##;

/// A registry in a directory, behind the `crane` commands the two scripts use (contracts/publish-step.md).
const FAKE_CRANE: &str = r##"#!/bin/sh
R="$FAKE_REG"
echo "$*" >> "$R/crane.log"
key() { echo "$1" | tr '/:@' '___'; }
cmd="$1"
shift
case "$cmd" in
  auth)
    cat > "$R/login-stdin"
    [ -n "$FAKE_LOGIN_FAIL" ] && exit 1
    exit 0;;
  manifest)
    if [ -n "$FAKE_MANIFEST_ERR" ]; then echo "$FAKE_MANIFEST_ERR" >&2; exit 1; fi
    f="$R/m-$(key "$1")"
    if [ -f "$f" ]; then cat "$f"; exit 0; fi
    echo "Error: MANIFEST_UNKNOWN: manifest unknown" >&2
    exit 1;;
  digest)
    [ -n "$FAKE_DIGEST_FAIL" ] && exit 1
    f="${2#/in/}"
    echo "sha256:$(sha256sum "$MNT/$f" | cut -d' ' -f1)"
    exit 0;;
  push)
    if [ -n "$FAKE_PUSH_ERR" ]; then echo "$FAKE_PUSH_ERR" >&2; exit 1; fi
    exit 0;;
  index)
    shift
    tag=""; ms=""
    while [ $# -gt 0 ]; do
      case "$1" in
        -t|--tag) tag="$2"; shift 2;;
        -m|--manifest) ms="$ms $2"; shift 2;;
        *) shift;;
      esac
    done
    set -- $ms
    a="${1#*@}"
    b="${2#*@}"
    printf '{"schemaVersion":2,"mediaType":"application/vnd.oci.image.index.v1+json","manifests":[{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"%s","size":1,"platform":{"architecture":"amd64","os":"linux"}},{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"%s","size":1,"platform":{"architecture":"arm64","os":"linux"}}]}\n' "$a" "$b" > "$R/m-$(key "$tag")"
    exit 0;;
  tag)
    if [ -n "$FAKE_TAG_ERR" ]; then echo "$FAKE_TAG_ERR" >&2; exit 1; fi
    src="$1"
    dst="${src%:*}:$2"
    cp "$R/m-$(key "$src")" "$R/m-$(key "$dst")" || exit 1
    exit 0;;
esac
exit 2
"##;

// ---- tars and lists ----

fn octal(buf: &mut [u8], v: u64) {
    let n = buf.len();
    let s = format!("{v:0w$o}", w = n - 1);
    buf[..n - 1].copy_from_slice(s.as_bytes());
    buf[n - 1] = 0;
}

fn member(name: &str, data: &[u8]) -> Vec<u8> {
    let mut h = [0u8; 512];
    h[..name.len()].copy_from_slice(name.as_bytes());
    octal(&mut h[100..108], 0o644);
    octal(&mut h[108..116], 0);
    octal(&mut h[116..124], 0);
    octal(&mut h[124..136], data.len() as u64);
    octal(&mut h[136..148], 0);
    h[156] = b'0';
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    h[148..156].copy_from_slice(b"        ");
    let sum: u32 = h.iter().map(|b| *b as u32).sum();
    let s = format!("{sum:06o}\0 ");
    h[148..156].copy_from_slice(s.as_bytes());
    let mut out = h.to_vec();
    out.extend_from_slice(data);
    out.resize(out.len().div_ceil(512) * 512, 0);
    out
}

/// A `docker save` tar: layer, config, manifest. `entry` and `cmd` are JSON (`[...]` or `null`).
fn archive(arch: &str, os: &str, user: &str, entry: &str, cmd: &str) -> Vec<u8> {
    let hex = "0123456789abcdef".repeat(4);
    let blob = format!("blobs/sha256/{hex}");
    let config = format!(
        "{{\"architecture\":\"{arch}\",\"os\":\"{os}\",\"config\":{{\"User\":\"{user}\",\"Entrypoint\":{entry},\"Cmd\":{cmd}}},\"rootfs\":{{\"type\":\"layers\",\"diff_ids\":[]}}}}"
    );
    let manifest =
        format!("[{{\"Config\":\"{blob}\",\"RepoTags\":[\"image-{arch}:latest\"],\"Layers\":[\"layer.tar\"]}}]");
    let mut v = member("layer.tar", &[7u8; 700]);
    v.extend(member(&blob, config.as_bytes()));
    v.extend(member("manifest.json", manifest.as_bytes()));
    v.extend_from_slice(&[0u8; 1024]);
    v
}

fn good_archive(arch: &str) -> Vec<u8> {
    archive(arch, "linux", "65532:65532", "[\"/hello\"]", "null")
}

/// A manifest list with one entry per `(os, architecture)`, each with its own digest.
fn list(media: &str, plats: &[(&str, &str)]) -> String {
    let ms: Vec<String> = plats
        .iter()
        .enumerate()
        .map(|(i, (os, arch))| {
            let c = (b'a' + i as u8) as char;
            format!(
                "{{\"mediaType\":\"application/vnd.oci.image.manifest.v1+json\",\"digest\":\"sha256:{}\",\"size\":{},\"platform\":{{\"architecture\":\"{arch}\",\"os\":\"{os}\"}}}}",
                c.to_string().repeat(64),
                100 + i
            )
        })
        .collect();
    format!(
        "{{\"schemaVersion\":2,\"mediaType\":\"{media}\",\"manifests\":[{}]}}",
        ms.join(",")
    )
}

fn good_list() -> String {
    list(OCI, &[("linux", "amd64"), ("linux", "arm64")])
}

fn tip(sha: &str) -> String {
    format!("[{{\"name\":\"main\",\"commit\":{{\"sha\":\"{sha}\"}}}}]")
}

const NO_TIP: &str = "[]";

// ---- fake GitHub ----

struct Gh {
    url: String,
    reqs: Arc<Mutex<Vec<Req>>>,
    tips: Arc<Mutex<VecDeque<String>>>,
}

/// `branches` answers the scripted bodies in order; the last one repeats. Status posts answer 201.
fn gh(tips: &[String]) -> Gh {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let reqs: Arc<Mutex<Vec<Req>>> = Arc::default();
    let queue: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(tips.iter().cloned().collect()));
    let tips_handle = queue.clone();
    let log = reqs.clone();
    std::thread::spawn(move || {
        for mut rq in server.incoming_requests() {
            let mut body = String::new();
            let _ = rq.as_reader().read_to_string(&mut body);
            let url = rq.url().to_string();
            let method = rq.method().to_string();
            log.lock().unwrap().push(Req {
                method: method.clone(),
                url: url.clone(),
                auth: String::new(),
                body,
            });
            let (code, text): (u16, String) = if method == "GET" && url.contains("/branches") {
                let mut q = queue.lock().unwrap();
                let b = if q.len() > 1 {
                    q.pop_front().unwrap()
                } else {
                    q.front().cloned().unwrap_or_else(|| NO_TIP.to_string())
                };
                (200, b)
            } else if method == "POST" {
                (201, "{}".to_string())
            } else {
                (200, "{}".to_string())
            };
            let _ = rq.respond(tiny_http::Response::from_string(text).with_status_code(code));
        }
    });
    Gh {
        url: format!("http://127.0.0.1:{port}"),
        reqs,
        tips: tips_handle,
    }
}

impl Gh {
    fn reqs(&self) -> Vec<Req> {
        self.reqs.lock().unwrap().clone()
    }

    /// `(state, description)` of every status post, in order.
    fn statuses(&self) -> Vec<(String, String)> {
        self.reqs()
            .iter()
            .filter(|r| r.method == "POST" && r.url.contains("/statuses/"))
            .map(|r| {
                let v: serde_json::Value = serde_json::from_str(&r.body).unwrap_or_default();
                (
                    v["state"].as_str().unwrap_or("").to_string(),
                    v["description"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect()
    }

    /// Replaces the scripted `branches` answers (between two runs of one test).
    fn set_tips(&self, tips: &[String]) {
        *self.tips.lock().unwrap() = tips.iter().cloned().collect();
    }

    fn branch_reads(&self) -> usize {
        self.reqs()
            .iter()
            .filter(|r| r.method == "GET" && r.url.contains("/branches"))
            .count()
    }
}

// ---- fake Docker Hub ----

#[derive(Clone, Copy)]
struct HubCfg {
    login: u16,
    /// `missing`, `public` or `private`
    repo: &'static str,
    /// Overrides every probe answer.
    probe: Option<u16>,
    create: u16,
}

impl HubCfg {
    fn ok() -> HubCfg {
        HubCfg {
            login: 200,
            repo: "public",
            probe: None,
            create: 201,
        }
    }
}

struct Hub {
    url: String,
    reqs: Arc<Mutex<Vec<Req>>>,
}

fn append(path: &Path, line: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{line}");
    }
}

fn hub(cfg: HubCfg, order: PathBuf) -> Hub {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let reqs: Arc<Mutex<Vec<Req>>> = Arc::default();
    let log = reqs.clone();
    std::thread::spawn(move || {
        let mut repo = cfg.repo;
        for mut rq in server.incoming_requests() {
            let mut body = String::new();
            let _ = rq.as_reader().read_to_string(&mut body);
            let auth = rq
                .headers()
                .iter()
                .find(|h| h.field.equiv("Authorization"))
                .map(|h| h.value.to_string())
                .unwrap_or_default();
            let url = rq.url().to_string();
            let method = rq.method().to_string();
            let login = url.starts_with("/v2/auth/token") || url.starts_with("/v2/users/login");
            let kind = if method == "POST" && login {
                "login"
            } else if method == "GET" && url.starts_with("/v2/repositories/") {
                "probe"
            } else if method == "POST" && url.trim_end_matches('/') == "/v2/repositories" {
                "create"
            } else {
                "other"
            };
            append(&order, &format!("hub-{kind}"));
            log.lock().unwrap().push(Req {
                method,
                url,
                auth,
                body,
            });
            let (code, text): (u16, String) = match kind {
                "login" if cfg.login == 200 => (200, format!("{{\"access_token\":\"{JWT}\"}}")),
                "login" => (cfg.login, "{}".to_string()),
                "probe" => match (cfg.probe, repo) {
                    (Some(c), _) => (c, "{}".to_string()),
                    (None, "missing") => (404, "{\"message\":\"object not found\"}".to_string()),
                    (None, "private") => (200, "{\"is_private\":true}".to_string()),
                    (None, _) => (200, "{\"is_private\":false}".to_string()),
                },
                "create" => {
                    if cfg.create == 201 {
                        repo = "public";
                    }
                    (cfg.create, "{}".to_string())
                }
                _ => (404, "{}".to_string()),
            };
            let _ = rq.respond(tiny_http::Response::from_string(text).with_status_code(code));
        }
    });
    Hub {
        url: format!("http://127.0.0.1:{port}"),
        reqs,
    }
}

impl Hub {
    fn reqs(&self) -> Vec<Req> {
        self.reqs.lock().unwrap().clone()
    }
}

// ---- the world of one test ----

/// The names file (None when absent), the reason and the exit code of a prepare run.
type Prepared = (Option<String>, String, Option<i32>);

struct W {
    sb: Sandbox,
    gh: Gh,
    hub: Hub,
    fixture: PathBuf,
    /// What the build step writes into the workspace: the two archives, absent from the clone.
    built: PathBuf,
    reg: PathBuf,
}

impl W {
    /// A passing image project `docker/img` with both archives, the Hub answering `cfg`, and `tips` as the
    /// answers of `branches`.
    fn with(cfg: HubCfg, tips: &[String]) -> W {
        let sb = Sandbox::new();
        std::fs::write(sb.bin.join("docker"), PUB_STUB).unwrap();
        let reg = sb.dir.join("reg");
        std::fs::create_dir_all(&reg).unwrap();
        let fake = sb.dir.join("fake-crane");
        std::fs::write(&fake, FAKE_CRANE).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let fixture = sb.fixture(&[
            ("docker/img/Dockerfile", DOCKERFILE),
            ("docker/img/README.md", "# img\n"),
            ("docker/img/.factory-ci.toml", TOML),
        ]);
        let built = sb.dir.join("built");
        let order = sb.stub.join("order");
        let w = W {
            hub: hub(cfg, order),
            gh: gh(tips),
            sb,
            fixture,
            built,
            reg,
        };
        w.put("docker/img/image-amd64.tar", &good_archive("amd64"));
        w.put("docker/img/image-arm64.tar", &good_archive("arm64"));
        w
    }

    /// The main case: the tip is the commit on both reads.
    fn new() -> W {
        W::with(HubCfg::ok(), &[tip(SHA)])
    }

    fn put(&self, rel: &str, bytes: &[u8]) {
        let p = self.base(rel).join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let _ = std::fs::remove_file(&p);
        std::fs::write(p, bytes).unwrap();
    }

    fn base(&self, rel: &str) -> &PathBuf {
        if rel.ends_with(".tar") {
            &self.built
        } else {
            &self.fixture
        }
    }

    fn rm(&self, rel: &str) {
        std::fs::remove_file(self.base(rel).join(rel)).unwrap();
    }

    fn command(&self, project: &str, branch: &str) -> Command {
        self.command_flag(project, branch, true)
    }

    /// `allow` false leaves `--allow-docker` out.
    fn command_flag(&self, project: &str, branch: &str, allow: bool) -> Command {
        let mut args = vec![
            "job",
            "--repo",
            "o/r",
            "--sha",
            SHA,
            "--project",
            project,
            "--branch",
            branch,
            "--run-id",
            RUN_ID,
        ];
        if allow {
            args.push("--allow-docker");
        }
        let mut c = self.sb.cmd(&args);
        job_env(&mut c, &self.gh.url);
        c.env("STUB_FIXTURE", &self.fixture)
            .env("STUB_BUILT", &self.built)
            .env("FAKE_REG", &self.reg)
            .env("FAKE_CRANE", self.sb.dir.join("fake-crane"))
            .env("FACTORY_CI_HUB_URL", &self.hub.url)
            .env("DOCKERHUB_TOKEN", TOKEN)
            .env("DOCKERHUB_USERNAME", USER)
            .env("STUB_A_OUT", format!("INDEX {}", good_list()));
        c
    }

    /// `docker/img` on `main`, then `tweak`.
    fn run(&self, tweak: impl Fn(&mut Command)) -> Output {
        self.run_for("docker/img", "main", tweak)
    }

    fn run_for(&self, project: &str, branch: &str, tweak: impl Fn(&mut Command)) -> Output {
        let mut c = self.command(project, branch);
        tweak(&mut c);
        c.output().unwrap()
    }

    fn emulate(c: &mut Command) {
        c.env("STUB_EMULATE", "1").env_remove("STUB_A_OUT");
    }

    fn last(&self) -> (String, String) {
        self.gh.statuses().last().cloned().unwrap_or_default()
    }

    /// The run ended with exactly this state and text, and the runner exited 0.
    fn ended(&self, o: &Output, state: &str, text: &str) {
        assert_eq!(o.status.code(), Some(0), "{}", out_text(o));
        let (s, d) = self.last();
        assert_eq!((s.as_str(), d.as_str()), (state, text), "{}", out_text(o));
    }

    /// Calls of the publish container `A` (`publish`) or `B` (`tag`).
    fn container(&self, kind: char) -> Vec<Vec<String>> {
        let suffix = if kind == 'A' { "publish" } else { "tag" };
        let name = format!("factory-ci-{RUN_ID}-{suffix}");
        self.sb
            .calls()
            .into_iter()
            .filter(|c| c.windows(2).any(|w| w[0] == "--name" && w[1] == name))
            .collect()
    }

    fn order(&self) -> Vec<String> {
        std::fs::read_to_string(self.sb.stub.join("order"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    fn hub_kinds(&self) -> Vec<String> {
        self.order()
            .iter()
            .filter_map(|l| l.strip_prefix("hub-").map(String::from))
            .collect()
    }

    /// The standard input saved for each container of `kind`, in start order.
    fn stdin(&self, kind: char) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.sb.stub)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("stdin-") && n.ends_with(&format!("-{kind}")))
            .collect();
        names.sort();
        names
            .iter()
            .map(|n| std::fs::read_to_string(self.sb.stub.join(n)).unwrap())
            .collect()
    }

    /// The exact arguments of each container of `kind`, in start order (the script keeps its newlines).
    fn argv(&self, kind: char) -> Vec<Vec<String>> {
        let mut names: Vec<String> = std::fs::read_dir(&self.sb.stub)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("argv-") && n.ends_with(&format!("-{kind}")))
            .collect();
        names.sort();
        names
            .iter()
            .map(|n| {
                let bytes = std::fs::read(self.sb.stub.join(n)).unwrap();
                let mut parts: Vec<String> = bytes
                    .split(|b| *b == 0)
                    .map(|p| String::from_utf8_lossy(p).to_string())
                    .collect();
                parts.pop();
                parts
            })
            .collect()
    }

    /// The `-c` script of the first container of `kind`: the last argument.
    fn script(&self, kind: char) -> String {
        self.argv(kind).remove(0).pop().unwrap_or_default()
    }

    /// Index in the order log of the first `ps` after the last step: the teardown before the publish.
    fn teardown_at(&self) -> usize {
        let order = self.order();
        let last_step = order
            .iter()
            .rposition(|l| l == "run-step")
            .expect("the pipeline ran");
        order
            .iter()
            .enumerate()
            .find(|(i, l)| *i > last_step && l.as_str() == "ps")
            .map(|(i, _)| i)
            .unwrap_or_else(|| panic!("no teardown after the last step: {order:?}"))
    }

    /// No publish container started; the Hub may have been called.
    fn nothing_published_but_hub(&self) {
        assert!(self.container('A').is_empty(), "container A started");
        assert!(self.container('B').is_empty(), "container B started");
    }

    fn crane_log(&self) -> Vec<String> {
        std::fs::read_to_string(self.reg.join("crane.log"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    /// Crane calls whose first word is `cmd`.
    fn crane(&self, cmd: &str) -> Vec<String> {
        self.crane_log()
            .into_iter()
            .filter(|l| l.split_whitespace().next() == Some(cmd))
            .collect()
    }

    fn seed(&self, tag: &str, json: &str) {
        let key = format!("{REPO}:{tag}").replace(['/', ':', '@'], "_");
        std::fs::write(self.reg.join(format!("m-{key}")), json).unwrap();
    }

    fn registry_list(&self, tag: &str) -> Option<String> {
        let key = format!("{REPO}:{tag}").replace(['/', ':', '@'], "_");
        std::fs::read_to_string(self.reg.join(format!("m-{key}"))).ok()
    }

    /// No publish container, no Hub call and no tip read happened.
    fn nothing_published(&self) {
        assert!(self.container('A').is_empty(), "container A started");
        assert!(self.container('B').is_empty(), "container B started");
        assert!(
            self.hub.reqs().is_empty(),
            "Hub called: {:?}",
            self.hub_kinds()
        );
    }

    /// Where any of `secrets` shows up in what the runner prints, posts, passes on argv or in a client env.
    fn leaks(&self, o: &Output, secrets: &[&str]) -> Vec<String> {
        let posts: Vec<String> = self
            .gh
            .reqs()
            .iter()
            .map(|r| format!("{}{}", r.url, r.body))
            .collect();
        let argv: Vec<String> = self.sb.calls().iter().map(|c| c.join("\n")).collect();
        let places = [
            ("stdout", stdout(o)),
            ("stderr", stderr(o)),
            ("statuses", posts.join("\n")),
            ("argv", argv.join("\n")),
            ("client env", self.sb.client_envs()),
        ];
        let mut out = vec![];
        for (place, text) in &places {
            for s in secrets {
                if text.contains(s) {
                    out.push(format!("{place}: {s}"));
                }
            }
        }
        out
    }

    /// Prepare mode: the names file (None when absent), the reason and the exit code.
    fn prepare(&self, project: &str, branch: &str) -> Prepared {
        let names = self.sb.dir.join("names.txt");
        let reason = self.sb.dir.join("reason.txt");
        let _ = std::fs::remove_file(&names);
        let _ = std::fs::remove_file(&reason);
        let (n, r) = (names.display().to_string(), reason.display().to_string());
        let args = [
            "job",
            "--repo",
            "o/r",
            "--sha",
            SHA,
            "--project",
            project,
            "--branch",
            branch,
            "--run-id",
            RUN_ID,
            "--allow-docker",
            "--names-file",
            &n,
            "--reason-file",
            &r,
        ];
        let mut c = self.sb.cmd(&args);
        job_env(&mut c, &self.gh.url);
        c.env("STUB_FIXTURE", &self.fixture);
        let o = c.output().unwrap();
        (
            std::fs::read_to_string(&names).ok(),
            std::fs::read_to_string(&reason).unwrap_or_default(),
            o.status.code(),
        )
    }
}

fn user_token_b64() -> String {
    base64(format!("{USER}:{TOKEN}").as_bytes())
}

fn latest_text() -> String {
    format!("published latest and {TAG}")
}

const A64: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// The digest of the one step image the template allows (`image::STEP_IMAGE`).
const STEP_DIGEST: &str = "851f91d241214e7c6db86513b270d58776379aacc5eb9c4a87e5b47115e3065c";

/// `DOCKERFILE` with `from` replaced by `to`; fails when `from` is not there, so a stale edit cannot pass.
fn df_edit(from: &str, to: &str) -> String {
    assert!(DOCKERFILE.contains(from), "{from:?} is not in the fixture");
    DOCKERFILE.replace(from, to)
}

fn toml_edit(from: &str, to: &str) -> String {
    assert!(TOML.contains(from), "{from:?} is not in the toml");
    TOML.replace(from, to)
}

const ARCHIVE_LINES: [&str; 3] = [
    "publish refused: archive missing",
    "publish refused: archive too large",
    "publish refused: archive unreadable",
];

// ---- the main path (S6, FR-014, FR-017, FR-026) ----

#[test]
fn main_image_publishes_both_tags() {
    // 012 T019; S6; FR-014, FR-017
    let w = W::new();
    let o = w.run(|_| {});
    w.ended(&o, "success", &latest_text());
    assert_eq!(w.container('A').len(), 1);
    assert_eq!(w.container('B').len(), 1);
    assert_eq!(
        w.gh.branch_reads(),
        2,
        "the tip is read before A and before B"
    );
    assert_eq!(w.hub_kinds(), ["login", "probe"]);
    for kind in ['A', 'B'] {
        let argv = w.argv(kind).remove(0).join("\n");
        assert!(argv.contains(&format!("REPO={REPO}")), "{argv}");
        assert!(argv.contains(&format!("TAG={TAG}")), "{argv}");
    }
}

#[test]
fn list_has_both_platforms() {
    // 012 T019; FR-026; either media type and either order pass
    let amd_arm = [("linux", "amd64"), ("linux", "arm64")];
    let arm_amd = [("linux", "arm64"), ("linux", "amd64")];
    for l in [
        list(OCI, &arm_amd),
        list(DOCKER_LIST, &amd_arm),
        list(DOCKER_LIST, &arm_amd),
    ] {
        let w = W::new();
        let o = w.run(|c| {
            c.env("STUB_A_OUT", format!("INDEX {l}"));
        });
        w.ended(&o, "success", &latest_text());
        assert_eq!(w.container('B').len(), 1, "{l}");
    }
}

#[test]
fn single_arch_or_bad_index_never_tags() {
    // 012 T019; FR-026; I10, I11
    let single = "{\"schemaVersion\":2,\"mediaType\":\"application/vnd.oci.image.manifest.v1+json\",\"config\":{}}";
    let bad = [
        format!("INDEX {}", list(OCI, &[("linux", "amd64")])),
        format!("INDEX {}", list(OCI, &[("linux", "arm64")])),
        format!(
            "INDEX {}",
            list(
                OCI,
                &[
                    ("linux", "amd64"),
                    ("linux", "arm64"),
                    ("unknown", "unknown")
                ]
            )
        ),
        format!(
            "INDEX {}",
            list(OCI, &[("linux", "amd64"), ("linux", "amd64")])
        ),
        format!("INDEX {single}"),
        "INDEX not json".to_string(),
        "no index line here".to_string(),
        String::new(),
    ];
    for out in bad {
        let w = W::new();
        let o = w.run(|c| {
            c.env("STUB_A_OUT", &out);
        });
        w.ended(&o, "error", "image index incomplete");
        assert_eq!(w.container('A').len(), 1, "{out}");
        assert!(w.container('B').is_empty(), "latest was tagged for {out}");
    }
}

#[test]
fn failed_build_pushes_nothing() {
    // 012 T019; FR-026, FR-007: a failing step, and one archive absent
    let w = W::new();
    let o = w.run(|c| {
        c.env("STUB_FAIL", "-build");
    });
    assert_eq!(w.last().0, "error", "{}", out_text(&o));
    w.nothing_published();
    assert_eq!(w.gh.branch_reads(), 0);

    let w = W::new();
    w.rm("docker/img/image-arm64.tar");
    let o = w.run(|_| {});
    w.ended(&o, "error", "publish refused: archive missing");
    w.nothing_published();
}

#[test]
fn image_step_failure_is_error_and_retried() {
    // 012 T019; FR-007, MAJOR-2; I15: a failed step with any exit code, and a timeout, end `error` for an image row
    for code in ["1", "2", "125", "126", "127"] {
        let w = W::new();
        let o = w.run(|c| {
            c.env("STUB_FAIL", "-build").env("STUB_FAIL_CODE", code);
        });
        let (state, text) = w.last();
        assert_eq!(state, "error", "exit {code}: {}", out_text(&o));
        assert!(text.starts_with("step build"), "exit {code}: {text}");
        w.nothing_published();
    }
    let w = W::new();
    w.put("docker/img/.factory-ci.toml", SLOW_TOML.as_bytes());
    let o = w.run(|c| {
        c.env("STUB_STEP_SLEEP", "3");
    });
    let (state, text) = w.last();
    assert_eq!(state, "error", "{}", out_text(&o));
    assert!(text.contains("timed out"), "{text}");
    w.nothing_published();
}

#[test]
fn a_step_of_a_plain_project_still_ends_failure() {
    // 012 T019; FR-007 last sentence: only image rows change
    let w = W::new();
    w.put(".factory-ci.toml", PLAIN_TOML.as_bytes());
    let o = w.run_for(".", "main", |c| {
        c.env("STUB_FAIL", "-build");
    });
    let (state, text) = w.last();
    assert_eq!(state, "failure", "{}", out_text(&o));
    assert!(text.starts_with("step build failed"), "{text}");
}

// ---- the check, in the job and again in publish (FR-008, FR-011, FR-027, FR-029, SC-002) ----

/// The Dockerfile or pipeline `setup` writes fails the check with `id`: status `failure`, no step, no publish
/// container, no Hub call, no tip read, and the status names ids only.
fn refuses(id: &str, setup: impl Fn(&W)) {
    let w = W::new();
    setup(&w);
    let o = w.run(|_| {});
    assert_eq!(o.status.code(), Some(0), "{}", out_text(&o));
    let (state, text) = w.last();
    assert_eq!(state, "failure", "{id}: {text}");
    assert!(text.starts_with("check failed: "), "{id}: {text}");
    assert!(text.contains(id), "{id} not in {text:?}");
    assert!(w.sb.step_runs().is_empty(), "{id}: {:?}", w.sb.step_runs());
    w.nothing_published();
    assert_eq!(w.gh.branch_reads(), 0, "{id}");
    assert!(w.leaks(&o, &[TOKEN]).is_empty(), "{id}");
}

#[test]
fn failed_check_pushes_nothing() {
    // 012 T021; FR-008, FR-029, SC-002: a sample of ids and bypasses
    refuses("instruction", |w| {
        w.put(
            "docker/img/Dockerfile",
            format!("{DOCKERFILE}\nADD . /x\n").as_bytes(),
        )
    });
    refuses("directive", |w| {
        w.put(
            "docker/img/Dockerfile",
            format!("# syntax=docker/dockerfile:1\n{DOCKERFILE}").as_bytes(),
        )
    });
    refuses("external-source", |w| {
        w.put(
            "docker/img/Dockerfile",
            format!("{DOCKERFILE}\nCOPY --from=alpine /a /b\n").as_bytes(),
        )
    });
    refuses("heredoc", |w| {
        w.put(
            "docker/img/Dockerfile",
            format!("{DOCKERFILE}\nENV A=<<X\n").as_bytes(),
        )
    });
    refuses("from-unpinned", |w| {
        w.put(
            "docker/img/Dockerfile",
            df_edit(&format!("@sha256:{A64}"), "").as_bytes(),
        )
    });
    refuses("final-user", |w| {
        w.put(
            "docker/img/Dockerfile",
            df_edit("USER 65532:65532", "USER 0").as_bytes(),
        )
    });
    refuses("secret-name", |w| {
        w.put(
            "docker/img/Dockerfile",
            format!("{DOCKERFILE}\nENV API_TOKEN=1\n").as_bytes(),
        )
    });
    // The template pins every command, so a flag cannot trip `secret-flag` alone: `step-template` fires too.
    for id in ["secret-flag", "step-template"] {
        refuses(id, |w| {
            w.put(
                "docker/img/.factory-ci.toml",
                toml_edit("-t image-amd64 .", "-t image-amd64 . --build-arg X=1").as_bytes(),
            )
        });
    }
    refuses("layout", |w| w.rm("docker/img/README.md"));
}

#[test]
fn emulation_refused() {
    // 012 T021; FR-027: a stage with `RUN` and no `--platform=$BUILDPLATFORM`
    refuses("emulation", |w| {
        w.put(
            "docker/img/Dockerfile",
            df_edit("FROM --platform=$BUILDPLATFORM ", "FROM ").as_bytes(),
        )
    });
}

#[test]
fn pipeline_with_secrets_fails_check() {
    // 012 T021; FR-011: the check runs before `prepare()`, so the id is named, not the fixed secrets line
    refuses("pipeline-secrets", |w| {
        w.put(
            "docker/img/.factory-ci.toml",
            format!("{TOML}secrets = [\"X_KEY\"]\n").as_bytes(),
        )
    });
}

#[test]
fn step_image_unpinned_fails_check() {
    // 012 T021; FR-029: no digest, and a tag
    for image in ["alpine", "alpine:3.23"] {
        refuses("step-image", |w| {
            w.put(
                "docker/img/.factory-ci.toml",
                toml_edit(&format!("docker:27.5.1-cli@sha256:{STEP_DIGEST}"), image).as_bytes(),
            )
        });
    }
}

#[test]
fn omitted_check_step_still_refused_in_publish() {
    // 012 T021; FR-008: a build step rewrites the Dockerfile after the first check; publish checks again
    let w = W::new();
    let o = w.run(|c| {
        c.env("STUB_REWRITE", "Dockerfile");
    });
    assert!(
        w.order().iter().any(|l| l == "run-step"),
        "the step must have run: {:?}",
        w.order()
    );
    let (state, text) = w.last();
    assert_eq!(state, "failure", "{}", out_text(&o));
    assert!(text.starts_with("check failed: instruction"), "{text}");
    w.nothing_published();
    assert_eq!(w.gh.branch_reads(), 0, "publish checks before any API call");
}

// ---- the archives (S6 lines 4 and 5; FR-030) ----

#[test]
fn archive_with_root_user_shell_entrypoint_or_wrong_platform_refused() {
    // 012 T021; FR-030, QA-1: the policy is `publish`'s, each fixed line starts no container
    let root = "publish refused: image runs as root";
    let shell = "publish refused: image has a shell entrypoint";
    let plat = "publish refused: image platform mismatch";
    let mut cases: Vec<(&str, Vec<u8>, &str)> = vec![];
    for user in ["", "0", "0:0", "root", "nobody", "1000x"] {
        cases.push((
            "amd64",
            archive("amd64", "linux", user, "[\"/hello\"]", "null"),
            root,
        ));
    }
    cases.push((
        "arm64",
        archive("arm64", "linux", "0", "[\"/hello\"]", "null"),
        root,
    ));
    for entry in [
        "[\"/bin/sh\",\"-c\",\"x\"]",
        "[\"sh\"]",
        "[\"/usr/bin/dash\"]",
        "[\"/bin/bash\"]",
        "[\"/bin/ash\"]",
        "[\"/bin/busybox\"]",
        "[\"/usr/bin/zsh\"]",
    ] {
        cases.push((
            "amd64",
            archive("amd64", "linux", "65532:65532", entry, "null"),
            shell,
        ));
    }
    cases.push((
        "amd64",
        archive("amd64", "linux", "65532:65532", "null", "[\"/bin/sh\"]"),
        shell,
    ));
    cases.push((
        "amd64",
        archive("arm64", "linux", "65532:65532", "[\"/hello\"]", "null"),
        plat,
    ));
    cases.push((
        "arm64",
        archive("amd64", "linux", "65532:65532", "[\"/hello\"]", "null"),
        plat,
    ));
    cases.push((
        "amd64",
        archive("amd64", "windows", "65532:65532", "[\"/hello\"]", "null"),
        plat,
    ));
    for (arch, bytes, line) in cases {
        let w = W::new();
        w.put(&format!("docker/img/image-{arch}.tar"), &bytes);
        let o = w.run(|_| {});
        w.ended(&o, "error", line);
        w.nothing_published();
        assert_eq!(
            w.gh.branch_reads(),
            0,
            "{line}: the policy runs before the tip read"
        );
    }
}

#[test]
fn archive_policy_lets_the_allowed_shapes_through() {
    // 012 T021; data-model.md "Archive config": uid 0 is the only numeric refusal, both empty is allowed, and
    // `cmd` counts only when `entrypoint` is empty
    for (user, entry, cmd) in [
        ("1000:0", "[\"/hello\"]", "null"),
        ("65532", "[\"/hello\"]", "null"),
        ("65532", "null", "null"),
        ("65532", "[]", "[]"),
        ("65532", "[\"/hello\"]", "[\"sh\"]"),
        ("65532", "null", "[\"/hello\"]"),
    ] {
        let w = W::new();
        w.put(
            "docker/img/image-amd64.tar",
            &archive("amd64", "linux", user, entry, cmd),
        );
        let o = w.run(|_| {});
        w.ended(&o, "success", &latest_text());
    }
}

#[test]
fn archive_lines_for_missing_empty_symlinked_garbage_and_oversized() {
    // 012 T015; S6 line 4; FR-030: the size limit is inclusive (2 GiB passes, one byte more is refused)
    let w = W::new();
    w.rm("docker/img/image-amd64.tar");
    let o = w.run(|_| {});
    w.ended(&o, "error", "publish refused: archive missing");
    w.nothing_published();

    let w = W::new();
    w.put("docker/img/image-arm64.tar", b"");
    let o = w.run(|_| {});
    let (state, text) = w.last();
    assert_eq!(state, "error", "{}", out_text(&o));
    assert!(ARCHIVE_LINES.contains(&text.as_str()), "{text}");
    w.nothing_published();

    let w = W::new();
    w.put("docker/img/real.tar", &good_archive("amd64"));
    w.rm("docker/img/image-amd64.tar");
    std::os::unix::fs::symlink("real.tar", w.built.join("docker/img/image-amd64.tar")).unwrap();
    let o = w.run(|_| {});
    // T046: the after-build check skips regular files only, so the link fails `layout`.
    w.ended(&o, "failure", "check failed: layout");
    w.nothing_published();

    let w = W::new();
    w.put(
        "docker/img/image-amd64.tar",
        b"this is not a tar file at all",
    );
    let o = w.run(|_| {});
    w.ended(&o, "error", "publish refused: archive unreadable");
    w.nothing_published();

    let w = W::new();
    let o = w.run(|c| {
        c.env("STUB_BIG", "2147483649");
    });
    w.ended(&o, "error", "publish refused: archive too large");
    w.nothing_published();

    let w = W::new();
    let o = w.run(|c| {
        c.env("STUB_BIG", "2147483648");
    });
    w.ended(&o, "success", &latest_text());
}

// ---- the tip (FR-018) ----

#[test]
fn unreadable_tip_pushes_nothing() {
    // 012 T025; FR-018: no `main` entry, or an empty list, before any Hub call or container
    for body in [
        NO_TIP.to_string(),
        "[{\"name\":\"dev\",\"commit\":{\"sha\":\"0123456789abcdef0123456789abcdef01234567\"}}]"
            .to_string(),
    ] {
        let w = W::with(HubCfg::ok(), &[body]);
        let o = w.run(|_| {});
        w.ended(&o, "error", "tip of main unreadable");
        w.nothing_published();
    }
}

#[test]
fn second_tip_read_fails_leaves_sha_tag() {
    // 012 T025; FR-018: the `sha-` list is written, `latest` is not
    let w = W::with(HubCfg::ok(), &[tip(SHA), NO_TIP.to_string()]);
    let o = w.run(|_| {});
    w.ended(&o, "error", "tip of main unreadable");
    assert_eq!(w.container('A').len(), 1);
    assert!(w.container('B').is_empty());
    assert_eq!(w.gh.branch_reads(), 2);
}

#[test]
fn older_commit_pushes_sha_only() {
    // 012 T025; FR-018: another tip on the first read, or on the second
    for tips in [vec![tip(OTHER)], vec![tip(SHA), tip(OTHER)]] {
        let w = W::with(HubCfg::ok(), &tips);
        let o = w.run(|_| {});
        w.ended(&o, "success", &format!("published {TAG} not latest"));
        assert_eq!(w.container('A').len(), 1);
        assert!(w.container('B').is_empty(), "latest moved");
    }
}

// ---- reuse of an existing list (FR-020, MAJOR-3) ----

#[test]
fn existing_valid_list_is_reused_without_digest_compare() {
    // 012 T025; FR-020, I14: digests differ from the archives', nothing is pushed or overwritten
    let w = W::new();
    let seeded = good_list();
    w.seed(TAG, &seeded);
    let o = w.run(W::emulate);
    w.ended(&o, "success", &latest_text());
    assert!(w.crane("push").is_empty(), "{:?}", w.crane_log());
    assert!(w.crane("index").is_empty(), "{:?}", w.crane_log());
    assert!(
        w.crane("digest").is_empty(),
        "no digest comparison: {:?}",
        w.crane_log()
    );
    let tags = w.crane("tag");
    assert_eq!(tags.len(), 1, "{tags:?}");
    assert!(tags[0].ends_with(" latest"), "{tags:?}");
    assert_eq!(w.registry_list(TAG).unwrap().trim(), seeded);
}

#[test]
fn existing_invalid_list_errors() {
    // 012 T025; FR-020: never overwritten, never tagged, container B never runs
    for bad in [
        list(OCI, &[("linux", "amd64")]),
        list(OCI, &[("linux", "amd64"), ("linux", "amd64")]),
        list(
            OCI,
            &[
                ("linux", "amd64"),
                ("linux", "arm64"),
                ("unknown", "unknown"),
            ],
        ),
    ] {
        let w = W::new();
        w.seed(TAG, &bad);
        let o = w.run(W::emulate);
        w.ended(&o, "error", "image index incomplete");
        for cmd in ["push", "index", "tag"] {
            assert!(w.crane(cmd).is_empty(), "{cmd}: {:?}", w.crane_log());
        }
        assert!(w.container('B').is_empty());
        assert_eq!(w.registry_list(TAG).unwrap().trim(), bad);
    }
}

#[test]
fn repeat_publish_only_tags_latest() {
    // 012 T025; FR-020: a first run for an older tip leaves the `sha-` list; the retry only tags `latest`
    let w = W::with(HubCfg::ok(), &[tip(OTHER)]);
    let o = w.run(W::emulate);
    w.ended(&o, "success", &format!("published {TAG} not latest"));
    assert_eq!(w.crane("push").len(), 2);
    assert_eq!(w.crane("index").len(), 1);
    assert!(w.crane("tag").is_empty());
    let first_list = w.registry_list(TAG).unwrap();
    let before = w.crane_log().len();

    w.gh.set_tips(&[tip(SHA)]);
    let o = w.run(W::emulate);
    w.ended(&o, "success", &latest_text());
    let again: Vec<String> = w.crane_log().into_iter().skip(before).collect();
    let first_word = |l: &String| l.split_whitespace().next().unwrap_or("").to_string();
    let words: Vec<String> = again.iter().map(first_word).collect();
    for banned in ["push", "index", "digest"] {
        assert!(!words.iter().any(|x| x == banned), "{banned} in {again:?}");
    }
    assert_eq!(words.iter().filter(|x| *x == "tag").count(), 1, "{again:?}");
    assert_eq!(
        w.registry_list(TAG).unwrap(),
        first_list,
        "the list was rewritten"
    );
}

#[test]
fn repeat_publish_ends_in_the_same_text_as_the_first_run() {
    // 012 T015; S6 last line: a commit whose list exists
    let w = W::new();
    let o = w.run(W::emulate);
    w.ended(&o, "success", &latest_text());
    let before = w.crane_log().len();
    let o = w.run(W::emulate);
    w.ended(&o, "success", &latest_text());
    let again: Vec<String> = w.crane_log().into_iter().skip(before).collect();
    assert!(
        !again
            .iter()
            .any(|l| l.starts_with("push ") || l.starts_with("index ")),
        "{again:?}"
    );
}

#[test]
fn first_publish_pushes_by_digest_then_appends_once() {
    // 012 T019; FR-026, contracts/publish-step.md script A and B
    let w = W::new();
    let o = w.run(W::emulate);
    w.ended(&o, "success", &latest_text());
    let log = w.crane_log();
    assert!(log[0].starts_with("auth login index.docker.io"), "{log:?}");
    assert!(
        log[0].contains("--password-stdin") && !log[0].contains(TOKEN),
        "{log:?}"
    );
    let pushes = w.crane("push");
    assert_eq!(pushes.len(), 2, "{log:?}");
    let digests: Vec<String> = pushes
        .iter()
        .map(|l| {
            let dest = l.split_whitespace().last().unwrap().to_string();
            let (repo, digest) = dest.split_once('@').expect("pushed by digest, no tag");
            assert_eq!(repo, REPO, "{l}");
            assert!(digest.starts_with("sha256:") && digest.len() == 71, "{l}");
            digest.to_string()
        })
        .collect();
    assert_ne!(
        digests[0], digests[1],
        "two different archives, two digests"
    );
    let index = w.crane("index");
    assert_eq!(index.len(), 1, "{log:?}");
    assert_eq!(
        index[0],
        format!(
            "index append -t {REPO}:{TAG} -m {REPO}@{} -m {REPO}@{}",
            digests[0], digests[1]
        )
    );
    let tags = w.crane("tag");
    assert_eq!(tags, [format!("tag {REPO}:{TAG} latest")]);
    let pass = std::fs::read_to_string(w.reg.join("login-stdin")).unwrap();
    assert_eq!(
        pass, TOKEN,
        "the token arrives on standard input, whole and without a newline"
    );
    let path = std::fs::read_to_string(w.sb.stub.join("crane-path")).unwrap();
    assert_eq!(path.trim(), CRANE_BIN, "the scripts get crane by CRANE_BIN");
}

#[test]
fn no_tag_other_than_sha12_and_latest_is_ever_named() {
    // 012 T019; FR-017, FR-026; I6: every crane argument, every `-e TAG`, and the two scripts
    let w = W::new();
    let o = w.run(W::emulate);
    w.ended(&o, "success", &latest_text());
    for line in w.crane_log() {
        for word in line.split_whitespace() {
            if let Some(rest) = word.strip_prefix(REPO) {
                match rest.split_once('@') {
                    Some((_, d)) => assert!(d.starts_with("sha256:"), "{line}"),
                    None => assert!(rest.is_empty() || rest == format!(":{TAG}"), "{line}"),
                }
            }
        }
    }
    for kind in ['A', 'B'] {
        let argv = w.argv(kind).remove(0);
        let tags: Vec<&String> = argv.iter().filter(|a| a.starts_with("TAG=")).collect();
        assert_eq!(tags, [&format!("TAG={TAG}")], "{kind}");
    }
    let script_a = w.script('A');
    assert!(
        !script_a.contains("latest"),
        "script A must not write `latest`:\n{script_a}"
    );
    assert!(w.script('B').contains("latest"));
}

// ---- exit codes (contracts/publish-step.md "Exit mapping") ----

#[test]
fn container_exit_codes_map_to_fixed_lines() {
    // 012 T015; S6: exit 41, 42, 43 and any other code
    let refused = "registry refused the token";
    for (code, text) in [
        ("41", refused),
        ("42", "push failed"),
        ("43", "push failed"),
        ("7", "push failed"),
        ("125", "push failed"),
    ] {
        let w = W::new();
        let o = w.run(|c| {
            c.env("STUB_A_EXIT", code);
        });
        w.ended(&o, "error", text);
        assert!(
            w.container('B').is_empty(),
            "exit {code}: B ran after A failed"
        );
    }
    for (code, text) in [("41", refused), ("42", "push failed"), ("9", "push failed")] {
        let w = W::new();
        let o = w.run(|c| {
            c.env("STUB_B_EXIT", code);
        });
        w.ended(&o, "error", text);
        assert_eq!(w.container('B').len(), 1);
    }
}

#[test]
fn the_scripts_map_crane_errors_to_the_exit_codes() {
    // 012 T015; contracts/publish-step.md scripts A and B: 41 for a refused login or UNAUTHORIZED or DENIED, 43 for
    // a digest failure, 42 for anything else
    let refused = "registry refused the token";
    let table = [
        ("FAKE_LOGIN_FAIL", "1", refused),
        (
            "FAKE_MANIFEST_ERR",
            "UNAUTHORIZED: authentication required",
            refused,
        ),
        (
            "FAKE_MANIFEST_ERR",
            "DENIED: requested access is denied",
            refused,
        ),
        (
            "FAKE_MANIFEST_ERR",
            "Error: connection reset by peer",
            "push failed",
        ),
        ("FAKE_DIGEST_FAIL", "1", "push failed"),
        ("FAKE_PUSH_ERR", "UNAUTHORIZED: bad credentials", refused),
        (
            "FAKE_PUSH_ERR",
            "DENIED: requested access is denied",
            refused,
        ),
        (
            "FAKE_PUSH_ERR",
            "Error: 503 service unavailable",
            "push failed",
        ),
        ("FAKE_TAG_ERR", "UNAUTHORIZED: bad credentials", refused),
        (
            "FAKE_TAG_ERR",
            "Error: 503 service unavailable",
            "push failed",
        ),
    ];
    for (var, value, text) in table {
        let w = W::new();
        let o = w.run(|c| {
            W::emulate(c);
            c.env(var, value);
        });
        w.ended(&o, "error", text);
        assert!(
            w.crane("tag").len() <= 1 && w.registry_list("latest").is_none(),
            "{var}: latest exists after a failure"
        );
    }
}

// ---- containers (FR-014, FR-015) ----

#[test]
fn publish_container_names_the_entrypoint() {
    // 012 T019; FR-014, MAJOR-4: `--entrypoint <CRANE_SH> <CRANE_IMAGE> -c <script>`, crane called by CRANE_BIN
    let w = W::new();
    let o = w.run(|_| {});
    w.ended(&o, "success", &latest_text());
    for kind in ['A', 'B'] {
        let argv = w.argv(kind).remove(0);
        let i = argv
            .iter()
            .position(|a| a == "--entrypoint")
            .unwrap_or_else(|| panic!("{kind}: no --entrypoint in {argv:?}"));
        assert_eq!(argv[i + 1], CRANE_SH, "{kind}");
        let image = &argv[i + 2];
        assert!(
            image.starts_with(CRANE_IMAGE_PREFIX) && image.len() == CRANE_IMAGE_PREFIX.len() + 64,
            "{kind}: {image}"
        );
        assert_eq!(argv[i + 3], "-c", "{kind}");
        assert_eq!(argv.len(), i + 5, "{kind}: the script is the last argument");
        assert!(
            argv.windows(2)
                .any(|p| p[0] == "-e" && p[1] == format!("CRANE={CRANE_BIN}")),
            "{kind}: {argv:?}"
        );
        assert!(
            argv[i + 4].contains("\"$CRANE\""),
            "{kind}: the script calls crane by $CRANE"
        );
    }
}

#[test]
fn publish_container_hardening_flags() {
    // 012 T019; contracts/publish-step.md "Containers A and B"; FR-015
    let w = W::new();
    let o = w.run(|_| {});
    w.ended(&o, "success", &latest_text());
    let meta = std::fs::metadata(&w.sb.tmp).unwrap();
    let user = format!("{}:{}", meta.uid(), meta.gid());
    for (kind, suffix) in [('A', "publish"), ('B', "tag")] {
        let argv = w.argv(kind).remove(0);
        let has = |a: &str, b: &str| argv.windows(2).any(|p| p[0] == a && p[1] == b);
        for flag in ["run", "-i", "--rm", "--read-only"] {
            assert!(argv.iter().any(|a| a == flag), "{kind}: {flag}");
        }
        assert!(
            has("--name", &format!("factory-ci-{RUN_ID}-{suffix}")),
            "{kind}"
        );
        assert!(
            has("--label", &format!("factory-ci.run={RUN_ID}")),
            "{kind}"
        );
        assert!(has("--cap-drop", "ALL"), "{kind}");
        assert!(has("--security-opt", "no-new-privileges"), "{kind}");
        assert!(has("--pids-limit", "256"), "{kind}");
        assert!(has("--memory", "1g"), "{kind}");
        assert!(has("--user", &user), "{kind}: {user}");
        assert!(has("-e", "HOME=/tmp"), "{kind}");
        assert!(has("-e", "DOCKER_CONFIG=/tmp/.docker"), "{kind}");
        assert!(
            !argv
                .iter()
                .any(|a| a == "--privileged" || a == "--network=host"),
            "{kind}"
        );
    }
    assert!(
        !w.argv('B').remove(0).iter().any(|a| a == "-v"),
        "B mounts nothing"
    );
}

#[test]
fn publish_dir_holds_only_the_archives_read_only() {
    // 012 T019; FR-014, FR-015; I12: a fresh directory, two files, mounted `:ro`, gone afterwards
    let w = W::new();
    let o = w.run(|_| {});
    w.ended(&o, "success", &latest_text());
    let listing = std::fs::read_to_string(w.sb.stub.join("in-A")).unwrap();
    let mut files: Vec<&str> = listing.lines().collect();
    files.sort();
    assert_eq!(files, ["image-amd64.tar", "image-arm64.tar"]);
    let argv = w.argv('A').remove(0);
    let mount = argv
        .windows(2)
        .find(|p| p[0] == "-v")
        .map(|p| p[1].clone())
        .expect("A mounts the fresh directory");
    let dir = mount
        .strip_suffix(":/in:ro")
        .unwrap_or_else(|| panic!("not read-only: {mount}"));
    assert!(!dir.contains(':'), "{dir}");
    assert!(
        Path::new(dir).starts_with(&w.sb.tmp),
        "{dir} is not under RUNNER_TEMP"
    );
    assert!(
        !dir.contains("docker/img"),
        "the project directory itself is mounted: {dir}"
    );
    assert!(
        !Path::new(dir).exists(),
        "the fresh directory outlived the run"
    );
}

#[test]
fn sidecar_removed_before_publish_containers_start() {
    // 012 T023; FR-015; I12: the stub log shows the teardown after the last step and before any publish call
    let w = W::new();
    let o = w.run(|_| {});
    w.ended(&o, "success", &latest_text());
    let order = w.order();
    let at = |what: &str| order.iter().position(|l| l == what);
    let last_step = order
        .iter()
        .rposition(|l| l == "run-step")
        .expect("the pipeline ran");
    let after = |what: &str| {
        order
            .iter()
            .enumerate()
            .find(|(i, l)| *i > last_step && l.as_str() == what)
            .map(|(i, _)| i)
            .unwrap_or_else(|| panic!("no `{what}` after the last step: {order:?}"))
    };
    let (ps, net, vol) = (w.teardown_at(), after("network"), after("volume"));
    let a = at("run-A").expect("container A");
    let b = at("run-B").expect("container B");
    let hub_first = at("hub-login").expect("the Hub login");
    assert!(ps < a && net < a && vol < a, "{order:?}");
    assert!(
        ps < hub_first && net < hub_first && vol < hub_first,
        "teardown before the first Hub call: {order:?}"
    );
    assert!(a < b);
}

#[test]
fn hub_ensure_runs_after_teardown_and_before_container_a() {
    // 012 T015; S6, contracts/publish-step.md step 8: even when the repository has to be created
    let w = W::with(
        HubCfg {
            repo: "missing",
            ..HubCfg::ok()
        },
        &[tip(SHA)],
    );
    let o = w.run(|_| {});
    w.ended(&o, "success", &latest_text());
    let order = w.order();
    let last_hub = order.iter().rposition(|l| l.starts_with("hub-")).unwrap();
    let first_hub = order.iter().position(|l| l == "hub-login").unwrap();
    let a = order.iter().position(|l| l == "run-A").unwrap();
    assert!(w.teardown_at() < first_hub, "{order:?}");
    assert!(last_hub < a, "{order:?}");
}

#[test]
fn a_left_over_resource_stops_the_publish() {
    // 012 T015; S6, QA-4: `run_left` true gives the fixed line, no Hub call and no container
    let w = W::new();
    let o = w.run(|c| {
        c.env("STUB_LEFT", "1");
    });
    w.ended(&o, "error", "publish refused: sidecar still running");
    w.nothing_published();
}

#[test]
fn run_left_fails_closed() {
    // 012 T015; QA-4: a failed `ps`, `network ls` or `volume ls` counts as "left"
    for cmd in ["ps -aq", "network ls", "volume ls"] {
        let w = W::new();
        let o = w.run(|c| {
            c.env("STUB_FAIL", cmd);
        });
        w.ended(&o, "error", "publish refused: sidecar still running");
        w.nothing_published();
    }
}

#[test]
fn publish_stops_when_the_deadline_is_near() {
    // 012 T019; FR-033, I16. ASSUMED SEAM: `FACTORY_CI_JOB_DEADLINE_SECS`, read in debug builds only. With one
    // second left, no publish call has its minimum
    let w = W::new();
    let o = w.run(|c| {
        c.env("FACTORY_CI_JOB_DEADLINE_SECS", "1");
    });
    w.ended(&o, "error", "publish refused: time ran out");
    w.nothing_published();
}

// ---- the Docker Hub (FR-016) ----

#[test]
fn missing_repository_is_created_public_before_the_push() {
    // 012 T019; FR-016: login, probe 404, create, probe, then container A
    let w = W::with(
        HubCfg {
            repo: "missing",
            ..HubCfg::ok()
        },
        &[tip(SHA)],
    );
    let o = w.run(|_| {});
    w.ended(&o, "success", &latest_text());
    assert_eq!(w.hub_kinds(), ["login", "probe", "create", "probe"]);
    let create = w
        .hub
        .reqs()
        .into_iter()
        .find(|r| {
            r.method == "POST" && !r.url.starts_with("/v2/auth") && !r.url.starts_with("/v2/users")
        })
        .unwrap();
    let body: serde_json::Value = serde_json::from_str(&create.body).unwrap();
    assert_eq!(body["is_private"], false, "{}", create.body);
    assert_eq!(body["namespace"], "sudtanj");
    assert_eq!(body["name"], "img");
}

#[test]
fn existing_private_repository_stops_the_item() {
    // 012 T019; FR-016: no create, no container
    let w = W::with(
        HubCfg {
            repo: "private",
            ..HubCfg::ok()
        },
        &[tip(SHA)],
    );
    let o = w.run(|_| {});
    w.ended(&o, "error", "publish refused: repository is private");
    assert_eq!(w.hub_kinds(), ["login", "probe"]);
    assert!(w.container('A').is_empty() && w.container('B').is_empty());
}

#[test]
fn probe_401_is_not_a_bad_token() {
    // 012 T019; FR-016, R13: 401 and 403 on the probe are a scope line
    for code in [401, 403] {
        let w = W::with(
            HubCfg {
                probe: Some(code),
                ..HubCfg::ok()
            },
            &[tip(SHA)],
        );
        let o = w.run(|_| {});
        let (state, text) = w.last();
        assert_eq!(state, "error", "{}", out_text(&o));
        assert_eq!(
            text, "docker hub api refused repository access",
            "probe {code}"
        );
        assert_ne!(text, "registry refused the token");
        w.nothing_published_but_hub();
    }
}

#[test]
fn hub_failures_end_with_their_fixed_lines() {
    // 012 T019; FR-016; contracts/hub-api.md "Mapping": nothing is pushed on any of them
    let refused = "registry refused the token";
    let api = "docker hub api error";
    let missing = HubCfg {
        repo: "missing",
        ..HubCfg::ok()
    };
    let cases = [
        (
            HubCfg {
                login: 401,
                ..HubCfg::ok()
            },
            refused,
        ),
        (
            HubCfg {
                login: 500,
                ..HubCfg::ok()
            },
            api,
        ),
        (
            HubCfg {
                probe: Some(500),
                ..HubCfg::ok()
            },
            api,
        ),
        (
            HubCfg {
                create: 403,
                ..missing
            },
            "docker hub api refused repository creation",
        ),
        (
            HubCfg {
                create: 401,
                ..missing
            },
            "docker hub api refused repository creation",
        ),
        (
            HubCfg {
                create: 500,
                ..missing
            },
            api,
        ),
    ];
    for (cfg, text) in cases {
        let w = W::with(cfg, &[tip(SHA)]);
        let o = w.run(|_| {});
        w.ended(&o, "error", text);
        w.nothing_published_but_hub();
    }
}

#[test]
fn hub_requests_carry_the_token_only_in_the_login_body() {
    // 012 T019, T023; FR-015, FR-016; I4
    let w = W::with(
        HubCfg {
            repo: "missing",
            ..HubCfg::ok()
        },
        &[tip(SHA)],
    );
    let o = w.run(|_| {});
    w.ended(&o, "success", &latest_text());
    let b64 = user_token_b64();
    let reqs = w.hub.reqs();
    assert_eq!(reqs.len(), 4, "{:?}", w.hub_kinds());
    assert!(
        reqs[0].url.starts_with("/v2/auth/token") || reqs[0].url.starts_with("/v2/users/login")
    );
    let login: serde_json::Value = serde_json::from_str(&reqs[0].body).unwrap();
    let pair = |u: &str, p: &str| login[u] == USER && login[p] == TOKEN;
    assert!(
        pair("identifier", "secret") || pair("username", "password"),
        "{}",
        reqs[0].body.replace(TOKEN, "<token>")
    );
    for r in &reqs[1..] {
        let all = format!("{} {} {}", r.url, r.body, r.auth);
        assert!(
            !all.contains(TOKEN) && !all.contains(&b64),
            "{} {}",
            r.method,
            r.url
        );
        assert_eq!(r.auth, format!("Bearer {JWT}"), "{} {}", r.method, r.url);
    }
    let leaks = w.leaks(&o, &[TOKEN, &b64, JWT]);
    assert!(leaks.is_empty(), "{leaks:?}");
}

// ---- credentials: who gets them, who sees them (FR-013, FR-015, FR-031, SC-003) ----

#[test]
fn token_only_on_stdin_of_publish_containers() {
    // 012 T023; FR-015; I4: stdin of A and B is the token, whole; nothing else carries it
    let w = W::new();
    let o = w.run(|_| {});
    w.ended(&o, "success", &latest_text());
    assert_eq!(w.stdin('A'), [TOKEN]);
    assert_eq!(w.stdin('B'), [TOKEN]);
    let b64 = user_token_b64();
    let leaks = w.leaks(&o, &[TOKEN, &b64]);
    assert!(leaks.is_empty(), "{leaks:?}");
    for kind in ['A', 'B'] {
        let argv = w.argv(kind).remove(0);
        for e in argv.windows(2).filter(|p| p[0] == "-e").map(|p| &p[1]) {
            assert!(
                e.contains('='),
                "{kind}: bare -e {e} would pass the runner's environment"
            );
            assert!(!e.starts_with("DOCKERHUB_"), "{kind}: {e}");
        }
        assert!(!argv.iter().any(|a| a.contains("DOCKERHUB")), "{kind}");
    }
}

#[test]
fn canary_token_absent_from_public_lines_argv_envs_status() {
    // 012 T023; FR-015, SC-003: with a log store too, and a step that prints the token and its docker-auth form
    let w = W::new();
    let loki = Fake::start();
    let b64 = user_token_b64();
    let out = format!("saw {TOKEN} and {b64}");
    let o = w.run(|c| {
        log_env(c, &loki.url);
        c.env("STUB_OUT", &out);
    });
    assert_eq!(o.status.code(), Some(0), "{}", out_text(&o));
    assert_eq!(w.last().0, "success");
    let partial = &TOKEN[..8];
    let leaks = w.leaks(&o, &[TOKEN, &b64, JWT, partial]);
    assert!(leaks.is_empty(), "{leaks:?}");
    let bodies: String = loki.requests().iter().map(|r| r.body.clone()).collect();
    for s in [TOKEN, b64.as_str(), JWT, partial] {
        assert!(!bodies.contains(s), "{s} reached the log store");
    }
    let lines = loki.loki_lines();
    assert!(
        lines.iter().any(|(_, l)| l.contains("saw ***")),
        "the step output must ship, masked: {lines:?}"
    );
}

#[test]
fn user_step_env_never_holds_token() {
    // 012 T023; FR-015, FR-024: the step container and every client environment
    let w = W::new();
    let o = w.run(|_| {});
    w.ended(&o, "success", &latest_text());
    let steps: Vec<Vec<String>> =
        w.sb.step_runs()
            .into_iter()
            .filter(|c| {
                !c.iter()
                    .any(|a| a.ends_with("-publish") || a.ends_with("-tag"))
            })
            .collect();
    assert!(!steps.is_empty());
    for s in &steps {
        let all = s.join("\n");
        assert!(!all.contains(TOKEN) && !all.contains("DOCKERHUB"), "{all}");
    }
    let envs = w.sb.client_envs();
    assert!(
        !envs.contains(TOKEN) && !envs.contains("DOCKERHUB_"),
        "a client environment holds a credential"
    );
}

#[test]
fn login_name_must_equal_the_namespace_and_is_never_redacted_or_printed() {
    // 012 T023; FR-031: `sudtanj` passes and stays visible, the token and its docker-auth form are masked
    let w = W::new();
    let b64 = user_token_b64();
    let out = format!("built by {USER} leak {TOKEN} {b64}");
    let o = w.run(|c| {
        c.env("STUB_OUT", &out);
    });
    w.ended(&o, "success", &latest_text());
    let so = stdout(&o);
    assert!(so.contains(&format!("built by {USER} leak ")), "{so}");
    assert!(so.contains("***"), "{so}");
    assert!(!so.contains(TOKEN) && !so.contains(&b64), "{so}");

    for name in ["other-user", "Sudtanj", "sudtanj2"] {
        let w = W::new();
        let o = w.run(|c| {
            c.env("DOCKERHUB_USERNAME", name);
        });
        w.ended(&o, "error", "publish refused: login is not the namespace");
        w.nothing_published();
        if name == "other-user" {
            assert!(w.leaks(&o, &[name]).is_empty(), "the value was printed");
        }
    }
    for set in [Some(""), None] {
        let w = W::new();
        let o = w.run(|c| match set {
            Some(v) => {
                c.env("DOCKERHUB_USERNAME", v);
            }
            None => {
                c.env_remove("DOCKERHUB_USERNAME");
            }
        });
        w.ended(&o, "error", "publish refused: no login name");
        w.nothing_published();
    }
}

#[test]
fn a_missing_or_empty_token_is_refused() {
    // 012 T015; S6 line 3
    for set in [Some(""), None] {
        let w = W::new();
        let o = w.run(|c| match set {
            Some(v) => {
                c.env("DOCKERHUB_TOKEN", v);
            }
            None => {
                c.env_remove("DOCKERHUB_TOKEN");
            }
        });
        w.ended(&o, "error", "publish refused: no token");
        w.nothing_published();
    }
}

#[test]
fn non_image_project_exports_no_credential() {
    // 012 T023; FR-013, SC-003; I1: a plain project and an image project on another branch use no credential
    let w = W::new();
    w.put(".factory-ci.toml", PLAIN_TOML.as_bytes());
    let o = w.run_for(".", "main", |_| {});
    w.ended(&o, "success", "all steps passed");
    w.nothing_published();
    assert_eq!(w.gh.branch_reads(), 0);
    assert!(w.crane_log().is_empty());
    assert!(w.leaks(&o, &[TOKEN]).is_empty());
    // without the variables the run is the same
    let o = w.run_for(".", "main", |c| {
        c.env_remove("DOCKERHUB_TOKEN")
            .env_remove("DOCKERHUB_USERNAME");
    });
    w.ended(&o, "success", "all steps passed");

    let w = W::new();
    let o = w.run_for("docker/img", "dev", |_| {});
    assert_eq!(w.last().0, "success", "{}", out_text(&o));
    w.nothing_published();
    assert_eq!(w.gh.branch_reads(), 0);
    assert!(w.leaks(&o, &[TOKEN]).is_empty());
}

// ---- prepare mode: the names file (S7, contracts/publish-step.md 1, I8) ----

#[test]
fn prepare_lists_both_names_for_a_passing_image_row_on_main() {
    // 012 T017, T024; S7
    let w = W::new();
    let (names, reason, code) = w.prepare("docker/img", "main");
    assert_eq!(code, Some(0), "{reason}");
    assert_eq!(
        names.as_deref(),
        Some("DOCKERHUB_USERNAME\nDOCKERHUB_TOKEN\n")
    );
    assert!(w.sb.step_runs().is_empty(), "prepare starts no container");
    assert!(w.gh.reqs().is_empty(), "prepare posts nothing");
}

#[test]
fn prepare_names_stay_empty_for_every_other_case() {
    // 012 T017, T024; S7, I8: another branch, a failing check, a plain project; the run still exits 0
    let w = W::new();
    let (names, reason, code) = w.prepare("docker/img", "dev");
    assert_eq!((names.as_deref(), code), (Some(""), Some(0)), "{reason}");

    let w = W::new();
    w.put(
        "docker/img/Dockerfile",
        format!("{DOCKERFILE}\nADD . /x\n").as_bytes(),
    );
    let (names, reason, code) = w.prepare("docker/img", "main");
    assert_eq!(
        (names.as_deref(), code),
        (Some(""), Some(0)),
        "a failing check: {reason}"
    );

    let w = W::new();
    w.put(".factory-ci.toml", PLAIN_TOML.as_bytes());
    let (names, reason, code) = w.prepare(".", "main");
    assert_eq!((names.as_deref(), code), (Some(""), Some(0)), "{reason}");
}

#[test]
fn prepare_refuses_a_pipeline_that_lists_a_secret_before_any_name() {
    // 012 T017; S7 "`NO_JOB_SECRETS` still first"
    let w = W::new();
    w.put(
        "docker/img/.factory-ci.toml",
        format!("{TOML}secrets = [\"X_KEY\"]\n").as_bytes(),
    );
    let (names, reason, code) = w.prepare("docker/img", "main");
    assert_eq!(code, Some(1));
    assert!(names.is_none(), "{names:?}");
    assert!(
        reason.contains("steps with secrets are not run by this job"),
        "{reason}"
    );
    assert!(!reason.contains("X_KEY"));
}

// ---- project paths at the job boundary (S2, T014) ----

#[test]
fn project_path_shapes_at_the_job_boundary() {
    // 012 T014; S2: `docker/<name>` is accepted, the rest is refused before any container or status
    let w = W::new();
    let o = w.run(|_| {});
    assert_eq!(o.status.code(), Some(0), "{}", out_text(&o));
    assert!(!w.gh.statuses().is_empty());
    for bad in [
        "a/b",
        "docker/Hello",
        "../x",
        "docker/a",
        "docker/x/y",
        "docker/",
        "/docker/x",
        "docker//x",
    ] {
        let w = W::new();
        let o = w.run_for(bad, "main", |_| {});
        assert_eq!(o.status.code(), Some(1), "{bad}: {}", out_text(&o));
        assert!(w.gh.statuses().is_empty(), "{bad}");
        assert!(w.sb.calls().is_empty(), "{bad}: a container started");
    }
}

// ---- names that pass today and must keep passing (T024) ----

#[test]
fn dockerhub_names_are_valid_secret_names() {
    // 012 T024; I7, I8: both names pass the pipeline rule and a watch entry that lists them parses
    for n in ["DOCKERHUB_TOKEN", "DOCKERHUB_USERNAME"] {
        assert!(rust_ci_runner::pipeline::valid_secret_name(n), "{n}");
    }
    let entry = "[[watch]]\nrepo = \"sudtanj/infrastructure\"\nid = 1474045808\nbranches = [\"main\"]\nallow_docker = true\nsecrets = [\"DOCKERHUB_USERNAME\", \"DOCKERHUB_TOKEN\"]\n";
    let list = rust_ci_runner::watch::parse(entry).unwrap_or_else(|p| panic!("{p:?}"));
    assert_eq!(list[0].secrets, ["DOCKERHUB_USERNAME", "DOCKERHUB_TOKEN"]);
    assert!(list[0].allow_docker && list[0].secret_branches.is_none());
    // the manual `ci` path unlocks nothing without `secret_branches`
    assert!(!list[0].secrets_allowed_on("main"));
}

// ---- review and security fixes (S6b) ----

#[test]
fn a_failed_check_joins_its_ids_with_a_space() {
    // 012 T032, R1: the status alphabet has no comma
    let w = W::new();
    w.put(
        "docker/img/Dockerfile",
        format!("# syntax=docker/dockerfile:1\n{DOCKERFILE}\nADD . /x\n").as_bytes(),
    );
    let o = w.run(|_| {});
    w.ended(&o, "failure", "check failed: directive instruction");
}

#[test]
fn the_deadline_seam_cuts_only_the_publish_step() {
    // 012 T033, R2: a slow pipeline still finishes; only publish sees the one-second deadline
    let w = W::new();
    let o = w.run(|c| {
        c.env("FACTORY_CI_JOB_DEADLINE_SECS", "1")
            .env("STUB_STEP_SLEEP", "2");
    });
    w.ended(&o, "error", "publish refused: time ran out");
    w.nothing_published();
}

#[test]
fn publish_checks_the_folder_after_the_teardown() {
    // 012 T035, R4: a rewritten Dockerfile and a sidecar that is still there end in the sidecar line,
    // because the check reads files only once nothing can change them
    let w = W::new();
    let o = w.run(|c| {
        c.env("STUB_REWRITE", "Dockerfile").env("STUB_LEFT", "1");
    });
    w.ended(&o, "error", "publish refused: sidecar still running");
    w.nothing_published();
}

#[test]
fn an_image_row_without_allow_docker_is_a_plain_row() {
    // 012 T036, R5 and security 5: no check, no names, no publish without the opt-in
    let w = W::new();
    w.put("docker/img/.factory-ci.toml", PLAIN_TOML.as_bytes());
    let o = w
        .command_flag("docker/img", "main", false)
        .output()
        .unwrap();
    w.ended(&o, "success", "all steps passed");
    w.nothing_published();
    assert_eq!(w.gh.branch_reads(), 0);
}
