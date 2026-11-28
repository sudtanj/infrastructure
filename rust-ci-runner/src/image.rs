//! Docker image projects under `docker/` (012 data-model.md "Check rules" and "Constants"): the
//! static check of a project folder, run on an untrusted checkout. No regex crate; only
//! `&'static str` ids leave the module, never file content.
use crate::pipeline;
use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

pub const NAMESPACE: &str = "sudtanj";
pub const PUBLISH_BRANCH: &str = "main";
pub const TOKEN_NAME: &str = "DOCKERHUB_TOKEN";
pub const USERNAME_NAME: &str = "DOCKERHUB_USERNAME";
pub const HUB_BASE: &str = "https://hub.docker.com";
pub const REGISTRY: &str = "index.docker.io";
pub const PLATFORMS: [(&str, &str); 2] =
    [("amd64", "image-amd64.tar"), ("arm64", "image-arm64.tar")];
pub const ARCHIVE_MAX: u64 = 2 * 1024 * 1024 * 1024;
pub const TAG_HEX: usize = 12;
/// Index digest of `crane:debug` read from gcr.io on 2026-11-28; T001 records and confirms it.
pub const CRANE_IMAGE: &str = "gcr.io/go-containerregistry/crane:debug@sha256:e78770b31258a3846f878036d9c1f63fbe4c871f9f56990bf77fd95c013e3c1b";
/// Expected paths inside `CRANE_IMAGE`; T002 confirms them.
pub const CRANE_SH: &str = "/busybox/sh";
pub const CRANE_BIN: &str = "/ko-app/crane";
/// The one step image of an image folder (pins.md): `docker:27.5.1-cli` index digest read on 2026-11-28.
pub const STEP_IMAGE: &str =
    "docker:27.5.1-cli@sha256:851f91d241214e7c6db86513b270d58776379aacc5eb9c4a87e5b47115e3065c";
/// One deadline for clone, pulls, pipeline and publish; five minutes of the 45-minute job stay for cleanup.
pub const JOB_DEADLINE: std::time::Duration = std::time::Duration::from_secs(40 * 60);

const DOCKERFILE_MAX: usize = 256 * 1024;
/// `libc::O_NONBLOCK` on Linux (the crate has no libc dependency); other targets open without it.
#[cfg(target_os = "linux")]
const O_NONBLOCK: i32 = 0o4000;
const PLATFORM_VARS: [&str; 5] = [
    "BUILDPLATFORM",
    "TARGETPLATFORM",
    "TARGETARCH",
    "TARGETOS",
    "BUILDARCH",
];
const CHAIN_MAX: usize = 20;
const ALLOWED: [&str; 11] = [
    "FROM",
    "ARG",
    "ENV",
    "WORKDIR",
    "COPY",
    "RUN",
    "USER",
    "ENTRYPOINT",
    "CMD",
    "LABEL",
    "EXPOSE",
];
const SECRET_WORDS: [&str; 6] = ["TOKEN", "PASSWORD", "PASSWD", "SECRET", "CREDENTIAL", "KEY"];
const SHELLS: [&str; 15] = [
    "sh", "bash", "dash", "ash", "busybox", "apk", "apt", "apt-get", "dpkg", "yum", "dnf",
    "microdnf", "zypper", "pip", "npm",
];

/// The only commands an image folder may run (contracts/image-project.md): free shell could build
/// another Dockerfile or save any image.
const BUILD_COMMANDS: [&str; 5] = [
    "docker build --platform linux/amd64 -t image-amd64 .",
    "docker save -o image-amd64.tar image-amd64",
    "docker build --platform linux/arm64 -t image-arm64 .",
    "docker save -o image-arm64.tar image-arm64",
    "chmod 0644 image-amd64.tar image-arm64.tar",
];
/// Limits of the folder walk: entries, bytes of one file and of all files, directory depth.
const MAX_ENTRIES: usize = 500;
const MAX_FILE: u64 = 1 << 20;
const MAX_TOTAL: u64 = 16 << 20;
const MAX_DEPTH: usize = 8;

pub fn image_project(project: &str) -> bool {
    project.starts_with("docker/")
}

pub fn publishes(project: &str, branch: &str) -> bool {
    image_project(project) && branch == PUBLISH_BRANCH
}

/// `^[a-z0-9]+(-[a-z0-9]+)*$`, 2 to 64 bytes.
pub fn name_ok(name: &str) -> bool {
    let b = name.as_bytes();
    (2..=64).contains(&b.len())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
}

/// The ids of the rules the folder breaks, sorted, once each. Never panics, whatever the files hold.
/// `after_build` is true once the build wrote its two archives; before it, those names are refused.
pub fn check(dir: &Path, after_build: bool) -> Vec<&'static str> {
    let mut bad = BTreeSet::new();
    let regular = |n: &str| std::fs::symlink_metadata(dir.join(n)).is_ok_and(|m| m.is_file());
    let (df, readme, toml) = (
        regular("Dockerfile"),
        regular("README.md"),
        regular(".factory-ci.toml"),
    );
    if !(df && readme && toml) {
        bad.insert("layout");
    }
    if df {
        match read_dockerfile(&dir.join("Dockerfile")) {
            Some(text) => dockerfile_rules(&text, &mut bad),
            None => {
                bad.insert("layout");
            }
        }
    }
    if toml {
        toml_rules(dir, &mut bad);
    }
    folder_rules(dir, after_build, &mut bad);
    bad.into_iter().collect()
}

/// `None` when over 256 KiB (exactly 256 KiB passes), not UTF-8, holding a NUL, or unreadable.
fn read_dockerfile(path: &Path) -> Option<String> {
    #[cfg(target_os = "linux")]
    use std::os::unix::fs::OpenOptionsExt;
    let mut buf = Vec::new();
    let mut o = std::fs::OpenOptions::new();
    o.read(true);
    // O_NONBLOCK: a FIFO swapped in after the `symlink_metadata` check must not block the open.
    #[cfg(target_os = "linux")]
    o.custom_flags(O_NONBLOCK);
    let f = o.open(path).ok()?;
    if !f.metadata().ok()?.is_file() {
        return None;
    }
    f.take(DOCKERFILE_MAX as u64 + 1)
        .read_to_end(&mut buf)
        .ok()?;
    if buf.len() > DOCKERFILE_MAX || buf.contains(&0) {
        return None;
    }
    String::from_utf8(buf).ok()
}

/// `@sha256:` plus 64 hex digits after a non-empty name.
fn pinned(reference: &str) -> bool {
    reference.rsplit_once("@sha256:").is_some_and(|(name, d)| {
        !name.is_empty() && d.len() == 64 && d.bytes().all(|c| c.is_ascii_hexdigit())
    })
}

/// A JSON array of strings, the only form BuildKit runs without a shell.
fn exec_form(args: &str) -> bool {
    serde_json::from_str::<Vec<String>>(args).is_ok()
}

fn secret_name(name: &str) -> bool {
    let up = name.to_ascii_uppercase();
    SECRET_WORDS.iter().any(|w| up.contains(w))
}

fn toml_rules(dir: &Path, bad: &mut BTreeSet<&'static str>) {
    let Ok(text) = pipeline::read_file(dir) else {
        bad.insert("pipeline-secrets");
        return;
    };
    secret_value(&text, bad);
    let Ok(p) = pipeline::parse(&text) else {
        bad.insert("pipeline-secrets");
        return;
    };
    let ok = |s: &pipeline::Step| {
        s.docker && s.image == STEP_IMAGE && s.env.is_empty() && s.commands == BUILD_COMMANDS
    };
    if p.steps.len() != 1 || !p.steps.get("build").is_some_and(ok) {
        bad.insert("step-template");
    }
    for s in p.steps.values() {
        if !s.secrets.is_empty() {
            bad.insert("pipeline-secrets");
        }
        if !pinned(&s.image) {
            bad.insert("step-image");
        }
        if s.env.keys().any(|k| secret_name(k)) {
            bad.insert("secret-name");
        }
        if s.commands.iter().any(|c| {
            ["--secret", "--ssh", "--build-arg"]
                .iter()
                .any(|f| c.contains(f))
        }) {
            bad.insert("secret-flag");
        }
    }
}

/// Counts the bytes of `class` right after each `prefix`; true when one run reaches `min`.
fn token(text: &str, prefix: &str, min: usize, class: fn(u8) -> bool) -> bool {
    text.match_indices(prefix).any(|(i, _)| {
        text.as_bytes()[i + prefix.len()..]
            .iter()
            .take_while(|b| class(**b))
            .take(min)
            .count()
            >= min
    })
}

fn secret_value(text: &str, bad: &mut BTreeSet<&'static str>) {
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
    let alnum = |b: u8| b.is_ascii_alphanumeric();
    let hit = token(text, "dckr_pat_", 20, word)
        || ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"]
            .iter()
            .any(|p| token(text, p, 30, alnum))
        || token(text, "github_pat_", 30, |b| {
            b.is_ascii_alphanumeric() || b == b'_'
        })
        || token(text, "AKIA", 16, |b| {
            b.is_ascii_digit() || b.is_ascii_uppercase()
        })
        || token(text, "AIza", 30, word)
        || token(text, "sk_live_", 20, alnum)
        || token(text, "npm_", 30, alnum)
        || token(text, "glpat-", 20, word)
        || ["xoxb-", "xoxa-", "xoxp-", "xoxr-", "xoxs-"]
            .iter()
            .any(|p| token(text, p, 10, word))
        || jwt(text)
        || text.lines().any(|l| {
            l.find("-----BEGIN ")
                .is_some_and(|i| l[i..].contains("PRIVATE KEY-----"))
        });
    if hit {
        bad.insert("secret-value");
    }
}

/// `eyJ` and then three base64url segments of at least 10 characters, joined by dots: a JSON web token.
fn jwt(text: &str) -> bool {
    let b64 = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
    let run = |s: &[u8]| s.iter().take_while(|b| b64(**b)).count();
    text.match_indices("eyJ").any(|(i, _)| {
        let mut rest = &text.as_bytes()[i..];
        (0..3).all(|n| {
            let len = run(rest);
            let ok = len >= 10 && (n == 2 || rest.get(len) == Some(&b'.'));
            rest = rest.get(len + 1..).unwrap_or_default();
            ok
        })
    })
}

/// File names that hold credentials.
fn secret_file(name: &str) -> bool {
    let named = [
        ".env",
        ".npmrc",
        ".netrc",
        "id_rsa",
        "id_dsa",
        "id_ed25519",
        "id_ecdsa",
    ];
    named.contains(&name)
        || name.starts_with(".env.")
        || [".pem", ".key", ".p12", ".pfx"]
            .iter()
            .any(|e| name.ends_with(e))
}

/// Every file of the folder, not only the three the check names: capped in entries, bytes and depth, no
/// entry that is not a regular file or a directory, no credential file name, no credential in a file.
/// After the build, the two archives at the top are skipped when they are regular files with those exact
/// lowercase names; `publish` reads them with `archive`. Before it, any entry with those names, in any
/// case, is a `layout` failure.
fn folder_rules(dir: &Path, after_build: bool, bad: &mut BTreeSet<&'static str>) {
    let (mut seen, mut total) = (0, 0);
    let mut todo = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            bad.insert("layout");
            continue;
        };
        for e in entries {
            seen += 1;
            if seen > MAX_ENTRIES {
                bad.insert("layout");
                return;
            }
            let Ok(e) = e else {
                bad.insert("layout");
                continue;
            };
            let name = e.file_name().to_string_lossy().to_lowercase();
            let meta = std::fs::symlink_metadata(e.path());
            let Ok(m) = meta else {
                bad.insert("layout");
                continue;
            };
            if depth == 0 && PLATFORMS.iter().any(|(_, f)| *f == name) {
                let exact = e.file_name().to_str().is_some_and(|n| n == name);
                if !(after_build && exact && m.is_file()) {
                    bad.insert("layout");
                }
                continue;
            }
            if m.is_dir() && depth < MAX_DEPTH {
                todo.push((e.path(), depth + 1));
                continue;
            }
            total += m.len();
            if !m.is_file() || m.len() > MAX_FILE || total > MAX_TOTAL {
                bad.insert("layout");
                continue;
            }
            if secret_file(&name) {
                bad.insert("secret-file");
            }
            match std::fs::read(e.path()) {
                Ok(bytes) => secret_value(&String::from_utf8_lossy(&bytes), bad),
                Err(_) => {
                    bad.insert("layout");
                }
            }
        }
    }
}

/// One Dockerfile instruction after continuations are joined.
struct Ins {
    kw: String,
    args: String,
    /// Continuations glued with no separator, so a token split across lines shows up.
    raw: String,
}

struct Stage {
    name: Option<String>,
    parent: Option<usize>,
    buildplatform: bool,
    run: bool,
    user: Option<String>,
    shell: bool,
}

/// Heredoc openers of a `RUN` or `COPY`: the terminator and whether tabs are stripped. `None` when a
/// `<<` is not part of a whole word `\d*<<-?WORD` or `\d*<<-?"WORD"` (BuildKit reads only those as
/// openers; fail closed otherwise), so the body skip can never hide an instruction BuildKit runs.
fn heredocs(args: &str) -> Option<Vec<(String, bool)>> {
    let mut out = Vec::new();
    for w in args.split_whitespace().filter(|w| w.contains("<<")) {
        let r = w
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .strip_prefix("<<")?;
        let dash = r.starts_with('-');
        let r = r.strip_prefix('-').unwrap_or(r);
        let q = r.chars().next().filter(|c| matches!(c, '"' | '\''));
        let word = match q {
            Some(q) => r.strip_prefix(q)?.strip_suffix(q)?,
            None => r,
        };
        if word.is_empty() || !word.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return None;
        }
        out.push((word.to_string(), dash));
    }
    Some(out)
}

/// Instructions in order; comments, blank lines and heredoc bodies (`RUN`, `COPY`) are skipped.
/// The bool is true when a heredoc opener is malformed or never closed.
fn instructions(text: &str) -> (Vec<Ins>, bool) {
    let mut lines = text.split('\n').map(|l| l.trim_end_matches('\r'));
    let mut out = Vec::new();
    let mut open: Vec<(String, bool)> = Vec::new();
    let mut bad_open = false;
    while let Some(line) = lines.next() {
        if !open.is_empty() {
            let (word, dash) = &open[0];
            let l = if *dash {
                line.trim_start_matches('\t')
            } else {
                line
            };
            if l == word.as_str() {
                open.remove(0);
            }
            continue;
        }
        let mut cur = line.trim().to_string();
        if cur.is_empty() || cur.starts_with('#') {
            continue;
        }
        let mut raw = cur.clone();
        while cur.ends_with('\\') {
            cur.pop();
            raw.pop();
            let Some(next) = lines
                .by_ref()
                .map(str::trim)
                .find(|l| !l.is_empty() && !l.starts_with('#'))
            else {
                break;
            };
            cur.push(' ');
            cur.push_str(next);
            raw.push_str(next);
        }
        let cur = cur.trim();
        let (kw, args) = cur.split_once(char::is_whitespace).unwrap_or((cur, ""));
        if kw.is_empty() {
            continue;
        }
        let kw = kw.to_ascii_uppercase();
        if kw == "RUN" || kw == "COPY" {
            match heredocs(args) {
                Some(h) => open = h,
                None => bad_open = true,
            }
        }
        out.push(Ins {
            kw,
            args: args.trim().to_string(),
            raw,
        });
    }
    (out, bad_open || !open.is_empty())
}

/// Names declared by `ARG`/`ENV` and whether each is given a value: `A=1 B="x y"` pairs, or the old
/// `ENV KEY value` form.
fn var_names(args: &str, env: bool) -> Vec<(&str, bool)> {
    let b = args.as_bytes();
    let (mut i, mut out) = (0, Vec::new());
    while i < b.len() {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let s = i;
        while i < b.len() && b[i] != b'=' && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        let name = &args[s..i];
        if i < b.len() && b[i] == b'=' {
            if !name.is_empty() {
                out.push((name, true));
            }
            i += 1;
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                i += 1;
                while i < b.len() && b[i] != q {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
            }
            while i < b.len() && !b[i].is_ascii_whitespace() {
                i += 1;
            }
        } else {
            if !name.is_empty() {
                out.push((name, env && !args[i..].trim().is_empty()));
            }
            if env {
                break;
            }
        }
    }
    out
}

/// The leading `--flag` words of a `RUN` or `COPY`, lower-cased, and the words after them.
fn flags(args: &str) -> (Vec<String>, Vec<&str>) {
    let mut words = args.split_whitespace().peekable();
    let mut fl = Vec::new();
    while let Some(w) = words.next_if(|w| w.starts_with("--")) {
        fl.push(w.to_ascii_lowercase());
    }
    (fl, words.collect())
}

/// `--from=` or a mount's `from=`: an earlier stage by name or by number.
fn earlier(stages: &[Stage], from: &str) -> bool {
    let cur = stages.len().saturating_sub(1);
    let from = from.to_ascii_lowercase();
    stages[..cur]
        .iter()
        .any(|s| s.name.as_deref() == Some(from.as_str()))
        || from
            .parse::<usize>()
            .is_ok_and(|n| n < cur && n.to_string() == from)
}

fn dockerfile_rules(text: &str, bad: &mut BTreeSet<&'static str>) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    secret_value(text, bad);
    let directive = text.lines().any(|l| {
        let l = l.trim_start().strip_prefix('#').unwrap_or("").trim_start();
        let l = l.to_ascii_lowercase();
        ["syntax", "escape"].iter().any(|d| {
            l.strip_prefix(d)
                .is_some_and(|r| r.trim_start().starts_with('='))
        })
    });
    if directive {
        bad.insert("directive");
    }
    let (ins, unclosed) = instructions(text);
    if unclosed {
        bad.insert("heredoc");
    }
    let mut stages: Vec<Stage> = Vec::new();
    let mut base: Vec<String> = Vec::new();
    for i in &ins {
        let kw = i.kw.as_str();
        if !ALLOWED.contains(&kw) {
            bad.insert("instruction");
        }
        if kw != "RUN" && kw != "COPY" && i.args.contains("<<") {
            bad.insert("heredoc");
        }
        secret_value(&i.raw, bad);
        match kw {
            "FROM" => add_stage(i, &mut stages, &mut base, bad),
            "ARG" | "ENV" => {
                for (n, assigned) in var_names(&i.args, kw == "ENV") {
                    if secret_name(n) {
                        bad.insert("secret-name");
                    }
                    // A forced platform value would turn the cross build into an emulated one.
                    if assigned && PLATFORM_VARS.iter().any(|p| p.eq_ignore_ascii_case(n)) {
                        bad.insert("emulation");
                    }
                }
            }
            "USER" => {
                if let Some(s) = stages.last_mut() {
                    s.user = Some(i.args.clone());
                }
            }
            "CMD" | "ENTRYPOINT" => {
                if let Some(s) = stages.last_mut() {
                    s.shell |= !exec_form(&i.args);
                }
            }
            "RUN" | "COPY" => run_copy(i, &mut stages, bad),
            _ => {}
        }
    }
    if stages.iter().any(|s| s.run && !s.buildplatform) {
        bad.insert("emulation");
    }
    final_rules(&stages, &base, bad);
}

fn add_stage(
    i: &Ins,
    stages: &mut Vec<Stage>,
    base: &mut Vec<String>,
    bad: &mut BTreeSet<&'static str>,
) {
    let mut words = i.args.split_whitespace();
    let mut buildplatform = false;
    let mut reference = "";
    for w in words.by_ref() {
        match w.strip_prefix("--platform=") {
            Some(v) => buildplatform = v == "$BUILDPLATFORM" || v == "${BUILDPLATFORM}",
            None if w.starts_with("--") => {}
            None => {
                reference = w;
                break;
            }
        }
    }
    let name = match (words.next(), words.next()) {
        (Some(a), Some(n)) if a.eq_ignore_ascii_case("as") => Some(n.to_ascii_lowercase()),
        _ => None,
    };
    let lower = reference.to_ascii_lowercase();
    let parent = stages
        .iter()
        .rposition(|s| s.name.as_deref() == Some(lower.as_str()));
    if reference.contains('$') {
        bad.insert("from-arg");
    }
    if parent.is_none() && reference != "scratch" && !pinned(reference) {
        bad.insert("from-unpinned");
    }
    stages.push(Stage {
        name,
        parent,
        buildplatform,
        run: false,
        user: None,
        shell: false,
    });
    base.push(reference.to_string());
}

fn run_copy(i: &Ins, stages: &mut [Stage], bad: &mut BTreeSet<&'static str>) {
    let (fl, rest) = flags(&i.args);
    let run = i.kw == "RUN";
    for f in &fl {
        // Host network and extra privileges are not part of the allowlist.
        if f.starts_with("--network") || f.starts_with("--security") {
            bad.insert("instruction");
        }
        // A quoted CSV field hides `from=` or `type=` from the split below: refuse, fail closed.
        if (f.starts_with("--mount=") || f.starts_with("--from=")) && f.contains(['"', '\'']) {
            bad.insert("external-source");
            bad.insert("secret-flag");
        }
        if let Some(v) = f.strip_prefix("--from=") {
            if !earlier(stages, v) {
                bad.insert("external-source");
            }
        }
        for field in f.strip_prefix("--mount=").unwrap_or("").split(',') {
            match field.split_once('=') {
                Some(("from", v)) if !earlier(stages, v) => {
                    bad.insert("external-source");
                }
                Some(("type", "secret" | "ssh")) => {
                    bad.insert("secret-flag");
                }
                _ => {}
            }
        }
    }
    let Some(s) = stages.last_mut() else { return };
    if run {
        s.run = true;
    } else if rest.iter().any(|w| {
        let w = w.trim_matches(['[', ']', '"', ',']).trim_end_matches('/');
        SHELLS.contains(
            &w.rsplit('/')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
        )
    }) {
        s.shell = true;
    }
}

/// The only distroless repositories a final stage may end at.
const DISTROLESS: [&str; 3] = [
    "gcr.io/distroless/static",
    "gcr.io/distroless/static-debian12",
    "gcr.io/distroless/base-nossl",
];

fn final_rules(stages: &[Stage], base: &[String], bad: &mut BTreeSet<&'static str>) {
    let Some(last) = stages.len().checked_sub(1) else {
        bad.insert("final-base");
        return;
    };
    let (mut chain, mut cur) = (vec![last], last);
    while let Some(p) = stages[cur].parent {
        cur = p;
        chain.push(p);
        if chain.len() > CHAIN_MAX + 1 {
            bad.insert("final-base");
            break;
        }
    }
    let end = &base[cur];
    let name = end.split('@').next().unwrap_or("");
    let (repo, tag) = name.split_once(':').unwrap_or((name, ""));
    let safe = end == "scratch" || (DISTROLESS.contains(&repo) && !tag.contains("debug"));
    if !safe {
        bad.insert("final-base");
    }
    if chain.iter().any(|&n| stages[n].run) {
        bad.insert("final-run");
    }
    if stages[last].shell {
        bad.insert("final-shell");
    }
    let user = chain.iter().find_map(|&n| stages[n].user.as_deref());
    let numeric = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let ok = user.is_some_and(|u| {
        let (uid, gid) = u.split_once(':').map_or((u, "0"), |(a, b)| (a, b));
        numeric(uid) && numeric(gid) && !uid.bytes().all(|b| b == b'0')
    });
    if !ok {
        bad.insert("final-user");
    }
}
