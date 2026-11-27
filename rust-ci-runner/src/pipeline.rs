use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

pub const MAX_BYTES: usize = 256 * 1024;
pub const MAX_STEPS: usize = 100;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub secrets: Vec<String>,
    /// Seconds.
    pub timeout: Option<u64>,
    #[serde(default)]
    pub docker: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    steps: BTreeMap<String, Step>,
}

#[derive(Debug, Clone)]
pub struct Pipeline {
    pub steps: BTreeMap<String, Step>,
}

pub fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Secret names become environment variable names of the engine client, so names that
/// the client or the runner use themselves are refused.
pub fn valid_secret_name(s: &str) -> bool {
    let shape = s
        .bytes()
        .enumerate()
        .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()));
    let u = s.to_ascii_uppercase();
    let exact = ["PATH", "HOME", "USER", "HOSTNAME", "TMPDIR", "SHELL", "FACTORY_CI_TOKEN", "ENV", "ALL_SECRETS", "NODE_OPTIONS"];
    let prefixes = [
        "DOCKER_",
        "CONTAINER",
        "XDG_",
        "REGISTRY_",
        "FCI_",
        "FACTORY_CI",
        "GH_",
        "GITHUB_",
        "ACTIONS_",
        "RUNNER_",
        "BWS_",
        "BASH_",
        "ITEM_",
        "LD_",
        "GIT_",
        "SSL_",
    ];
    let reserved = exact.contains(&u.as_str())
        || prefixes.iter().any(|p| u.starts_with(p))
        || u.ends_with("_PROXY");
    !s.is_empty() && shape && !reserved
}

/// Reads `.factory-ci.toml` from the root of a checkout. A symlink is refused: the checkout
/// is untrusted and a link could point at a file on the host.
pub fn read_file(dir: &Path) -> Result<String, String> {
    let path = dir.join(".factory-ci.toml");
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err("no .factory-ci.toml".to_string())
        }
        Err(e) => return Err(format!("cannot read .factory-ci.toml: {e}")),
        Ok(m) if !m.is_file() => return Err(".factory-ci.toml must be a regular file, not a symlink".to_string()),
        Ok(_) => {}
    }
    let mut buf = Vec::new();
    std::fs::File::open(&path)
        .and_then(|f| f.take(MAX_BYTES as u64 + 1).read_to_end(&mut buf))
        .map_err(|e| format!("cannot read .factory-ci.toml: {e}"))?;
    String::from_utf8(buf).map_err(|_| ".factory-ci.toml is not valid UTF-8".to_string())
}

pub fn parse(text: &str) -> Result<Pipeline, Vec<String>> {
    if text.len() > MAX_BYTES {
        return Err(vec![format!(
            "pipeline file is larger than {} KB",
            MAX_BYTES / 1024
        )]);
    }
    let file: File = toml::from_str(text).map_err(|e| vec![format!("invalid TOML: {e}")])?;
    let p = Pipeline { steps: file.steps };
    let bad = p.validate();
    if bad.is_empty() {
        Ok(p)
    } else {
        Err(bad)
    }
}

impl Pipeline {
    fn validate(&self) -> Vec<String> {
        let mut bad = Vec::new();
        if self.steps.is_empty() {
            bad.push("no steps defined".to_string());
        }
        if self.steps.len() > MAX_STEPS {
            bad.push(format!(
                "{} steps, the limit is {MAX_STEPS}",
                self.steps.len()
            ));
        }
        for (name, s) in &self.steps {
            if !valid_name(name) {
                bad.push(format!(
                    "step name '{name}' must be 1 to 64 letters, digits, '-' or '_'"
                ));
            }
            // The runner's own containers are called factory-ci-<run>-clone and -dind; `runner` is the name of its public lines.
            if name == "clone" || name == "dind" || name == "runner" {
                bad.push(format!("step name '{name}' is reserved"));
            }
            if s.image.trim().is_empty() {
                bad.push(format!("step {name} has no image"));
            } else if s.image.starts_with('-') {
                bad.push(format!("step {name}: image must not start with '-'"));
            }
            if s.commands.is_empty() {
                bad.push(format!("step {name} has no commands"));
            }
            for d in &s.depends_on {
                if !self.steps.contains_key(d) {
                    bad.push(format!("step {name} depends on unknown step {d}"));
                }
            }
            for k in s.env.keys() {
                if k.is_empty() || k.contains(['=', '\0']) {
                    bad.push(format!("step {name}: invalid env name '{k}'"));
                }
            }
            for sec in &s.secrets {
                if !valid_secret_name(sec) {
                    bad.push(format!("step {name}: invalid secret name '{sec}'"));
                }
            }
            if s.timeout.is_some_and(|t| t == 0 || t > 86400) {
                bad.push(format!("step {name}: timeout must be 1 to 86400 seconds"));
            }
        }
        let cycle = self.cycle();
        if !cycle.is_empty() {
            bad.push(format!("dependency cycle among steps: {}", cycle.join(", ")));
        }
        bad
    }

    /// Steps on a dependency cycle: peel steps nothing depends on and steps with no
    /// dependencies until no step can be peeled.
    fn cycle(&self) -> Vec<&str> {
        let mut left: BTreeSet<&str> = self.steps.keys().map(String::as_str).collect();
        loop {
            let snap = left.clone();
            left.retain(|n| {
                let has_dep = self.steps[*n]
                    .depends_on
                    .iter()
                    .any(|d| snap.contains(d.as_str()));
                let has_dependent = snap
                    .iter()
                    .any(|m| self.steps[*m].depends_on.iter().any(|d| d.as_str() == *n));
                has_dep && has_dependent
            });
            if left.len() == snap.len() {
                return left.into_iter().collect();
            }
        }
    }

    /// The named step and everything it depends on.
    pub fn only(&self, name: &str) -> Result<Pipeline, String> {
        if !self.steps.contains_key(name) {
            return Err(format!("unknown step {name}"));
        }
        let mut keep = BTreeSet::new();
        let mut todo = vec![name.to_string()];
        while let Some(n) = todo.pop() {
            if keep.insert(n.clone()) {
                todo.extend(self.steps[&n].depends_on.iter().cloned());
            }
        }
        let steps = self
            .steps
            .iter()
            .filter(|(k, _)| keep.contains(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(Pipeline { steps })
    }

    pub fn needs_docker(&self) -> bool {
        self.steps.values().any(|s| s.docker)
    }

    /// Looks up every listed secret. Reports every problem.
    pub fn resolve_secrets(
        &self,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<BTreeMap<String, String>, Vec<String>> {
        let mut out = BTreeMap::new();
        let mut bad = Vec::new();
        for (name, s) in &self.steps {
            for sec in &s.secrets {
                if let Some(v) = lookup(sec) {
                    out.insert(sec.clone(), v);
                } else {
                    bad.push(format!("step {name} requests secret {sec}, which is not defined"));
                }
            }
        }
        if bad.is_empty() {
            Ok(out)
        } else {
            Err(bad)
        }
    }

    /// Docker access needs the repository's opt-in.
    pub fn docker_problems(&self, allowed: bool) -> Vec<String> {
        self.steps
            .iter()
            .filter(|(_, s)| s.docker && !allowed)
            .map(|(name, _)| format!("step {name} needs docker but repo does not allow it"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bad(text: &str) -> Vec<String> {
        parse(text).unwrap_err()
    }

    fn has(v: &[String], needle: &str) -> bool {
        v.iter().any(|m| m.contains(needle))
    }

    const GOOD: &str = r#"
[steps.lint]
image = "alpine"
commands = ["echo lint"]

[steps.test]
image = "alpine"
commands = ["echo test"]
timeout = 60

[steps.build]
image = "alpine"
commands = ["echo build"]
depends_on = ["lint", "test"]
env = { A = "1" }
secrets = ["TOKEN"]
"#;

    #[test]
    fn parses_a_valid_file() {
        let p = parse(GOOD).unwrap();
        assert_eq!(p.steps.len(), 3);
        assert_eq!(p.steps["build"].depends_on, ["lint", "test"]);
        assert_eq!(p.steps["test"].timeout, Some(60));
        assert!(!p.needs_docker());
    }

    #[test]
    fn rejects_unknown_keys() {
        let v = bad("[steps.a]\nimage = \"x\"\ncommands = [\"y\"]\nbogus = 1\n");
        assert!(has(&v, "bogus"));
        let v = bad("extra = 1\n[steps.a]\nimage = \"x\"\ncommands = [\"y\"]\n");
        assert!(has(&v, "extra"));
    }

    #[test]
    fn rejects_duplicate_step_names() {
        let t = "[steps.a]\nimage = \"x\"\ncommands = [\"y\"]\n[steps.a]\nimage = \"x\"\ncommands = [\"y\"]\n";
        assert!(!bad(t).is_empty());
    }

    #[test]
    fn reports_every_problem() {
        let t = r#"
[steps.a]
commands = ["x"]
depends_on = ["ghost"]

[steps.b]
image = "alpine"

[steps.c]
image = "-evil"
commands = ["x"]
timeout = 0
secrets = ["PATH"]
"#;
        let v = bad(t);
        assert!(has(&v, "step a has no image"));
        assert!(has(&v, "step a depends on unknown step ghost"));
        assert!(has(&v, "step b has no commands"));
        assert!(has(&v, "step c: image must not start"));
        assert!(has(&v, "step c: timeout"));
        assert!(has(&v, "step c: invalid secret name 'PATH'"));
    }

    #[test]
    fn reserved_step_and_secret_names_are_refused() {
        let t = "[steps.clone]\nimage = \"x\"\ncommands = [\"y\"]\n[steps.dind]\nimage = \"x\"\ncommands = [\"y\"]\n";
        let v = bad(t);
        assert!(has(&v, "step name 'clone' is reserved"));
        assert!(has(&v, "step name 'dind' is reserved"));
        let refused = [
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "ACTIONS_X",
            "RUNNER_X",
            "FACTORY_CI_TOKEN",
            "path",
        ];
        for name in refused {
            assert!(!valid_secret_name(name), "{name}");
        }
        assert!(valid_secret_name("NPM_TOKEN"));
    }

    #[test]
    fn detects_cycles_and_names_only_cycle_members() {
        let t = r#"
[steps.a]
image = "x"
commands = ["x"]
depends_on = ["b"]
[steps.b]
image = "x"
commands = ["x"]
depends_on = ["a"]
[steps.c]
image = "x"
commands = ["x"]
depends_on = ["a"]
[steps.d]
image = "x"
commands = ["x"]
depends_on = ["d"]
[steps.e]
image = "x"
commands = ["x"]
"#;
        let v = bad(t);
        assert!(has(&v, "dependency cycle among steps: a, b, d"));
    }

    #[test]
    fn enforces_limits() {
        assert!(has(&bad(&" ".repeat(MAX_BYTES + 1)), "larger than"));
        let mut t = String::new();
        for i in 0..=MAX_STEPS {
            t.push_str(&format!(
                "[steps.s{i}]\nimage = \"x\"\ncommands = [\"y\"]\n"
            ));
        }
        assert!(has(&bad(&t), "the limit is 100"));
        assert!(has(&bad(""), "no steps"));
    }

    #[test]
    fn read_file_refuses_missing_links_and_big_files() {
        let dir = std::env::temp_dir().join(format!("fci-pipe-{}", crate::util::random_id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_file(&dir).unwrap_err(), "no .factory-ci.toml");
        let file = dir.join(".factory-ci.toml");
        std::fs::write(&file, GOOD).unwrap();
        assert_eq!(read_file(&dir).unwrap(), GOOD);
        std::fs::write(&file, " ".repeat(MAX_BYTES * 2)).unwrap();
        assert!(has(&bad(&read_file(&dir).unwrap()), "larger than"));
        std::fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", &file).unwrap();
        assert!(read_file(&dir).unwrap_err().contains("regular file"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_selects_dependencies() {
        let p = parse(GOOD).unwrap();
        let q = p.only("build").unwrap();
        assert_eq!(q.steps.len(), 3);
        let q = p.only("lint").unwrap();
        assert_eq!(q.steps.keys().collect::<Vec<_>>(), ["lint"]);
        assert!(p.only("nope").is_err());
    }

    // T029 (FR-018, FR-019; AS 7.7, story 12): the API without the `fork` parameter.
    #[test]
    fn reserved_secret_names_are_refused_whatever_the_case() {
        for name in [
            "BWS_ACCESS_TOKEN",
            "BWS_X",
            "LD_PRELOAD",
            "GIT_DIR",
            "SSL_CERT_FILE",
            "BASH_ENV",
            "ENV",
            "HTTPS_PROXY",
            "http_proxy",
            "FACTORY_CI_LOG_URL",
            "FACTORY_CI_LOG_USER",
            "FACTORY_CI_LOG_TOKEN",
            "bws_x",
            "Github_Token",
        ] {
            assert!(!valid_secret_name(name), "{name} must be reserved");
        }
        for name in ["NPM_TOKEN", "DEPLOY_KEY", "_X", "a1"] {
            assert!(valid_secret_name(name), "{name}");
        }
    }

    #[test]
    fn a_bad_or_reserved_step_secret_name_fails_parse_naming_every_one() {
        let t = "[steps.a]\nimage = \"x\"\ncommands = [\"y\"]\nsecrets = [\"1BAD\", \"A-B\"]\n\
                 [steps.b]\nimage = \"x\"\ncommands = [\"y\"]\nsecrets = [\"BWS_ACCESS_TOKEN\", \"GOOD\"]\n";
        let v = bad(t);
        for n in ["1BAD", "A-B", "BWS_ACCESS_TOKEN"] {
            assert!(has(&v, n), "{n} not named in {v:?}");
        }
        assert!(!has(&v, "GOOD"), "{v:?}");
    }

    #[test]
    fn resolve_secrets_without_fork_resolves_or_reports_not_defined() {
        let p = parse(GOOD).unwrap();
        let ok = p.resolve_secrets(|n| (n == "TOKEN").then(|| "v".to_string())).unwrap();
        assert_eq!(ok["TOKEN"], "v");
        let e = p.resolve_secrets(|_| None).unwrap_err();
        assert_eq!(e, ["step build requests secret TOKEN, which is not defined"]);
    }

    #[test]
    fn docker_needs_the_repo_opt_in_and_nothing_else() {
        let p = parse("[steps.img]\nimage = \"x\"\ncommands = [\"y\"]\ndocker = true\n").unwrap();
        assert!(p.needs_docker());
        assert!(p.docker_problems(true).is_empty());
        assert_eq!(p.docker_problems(false), ["step img needs docker but repo does not allow it"]);
        assert!(parse(GOOD).unwrap().docker_problems(false).is_empty());
    }

    // 008 T004 (test-first): `runner` is reserved so a step cannot forge `[runner] pipeline passed`. FR-018.
    #[test]
    fn step_named_runner_is_refused() {
        let v = bad("[steps.runner]\nimage = \"x\"\ncommands = [\"y\"]\n");
        assert!(has(&v, "runner"), "{v:?}");
        assert!(parse("[steps.runner2]\nimage = \"x\"\ncommands = [\"y\"]\n").is_ok());
        assert!(parse("[steps.Runner]\nimage = \"x\"\ncommands = [\"y\"]\n").is_ok());
    }
}
