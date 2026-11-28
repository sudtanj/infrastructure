//! T055. `job --names-file F --reason-file R` (prepare mode). FR-039, FR-041, FR-057; AS 12.1,
//! US1 AS 1.5. A stub engine whose clone copies a fixture checkout.

mod common;
use common::*;
use std::path::PathBuf;
use std::process::Output;

struct Prep {
    sb: Sandbox,
    api: Fake,
    names: PathBuf,
    reason: PathBuf,
}

impl Prep {
    fn new() -> Prep {
        let sb = Sandbox::new();
        let (names, reason) = (sb.dir.join("names.txt"), sb.dir.join("reason.txt"));
        Prep {
            sb,
            api: Fake::start(),
            names,
            reason,
        }
    }

    fn args(&self, extra: &[&str]) -> Vec<String> {
        let mut a: Vec<String> = job_args(extra).into_iter().map(String::from).collect();
        a.extend([
            "--names-file".into(),
            self.names.display().to_string(),
            "--reason-file".into(),
            self.reason.display().to_string(),
        ]);
        a
    }

    fn run(&self, files: &[(&str, &str)], extra: &[&str]) -> Output {
        let fixture = self.sb.fixture(files);
        self.run_with(&fixture, extra, |_| {})
    }

    fn run_with(
        &self,
        fixture: &std::path::Path,
        extra: &[&str],
        tweak: impl Fn(&mut std::process::Command),
    ) -> Output {
        let args = self.args(extra);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut c = self.sb.cmd(&refs);
        job_env(&mut c, &self.api.url);
        c.env("STUB_FIXTURE", fixture);
        tweak(&mut c);
        c.output().unwrap()
    }

    fn reason(&self) -> String {
        std::fs::read_to_string(&self.reason).unwrap_or_default()
    }

    /// The refusal contract: exit 1, one reason line, no names file, no step container, silence.
    fn assert_refused(&self, o: &Output, needle: &str) {
        let why = self.reason();
        assert_eq!(o.status.code(), Some(1), "{}", out_text(o));
        assert_eq!(why.trim().lines().count(), 1, "one reason line: {why:?}");
        assert!(why.contains(needle), "{needle} not in {why:?}");
        assert!(!self.names.exists(), "no names file on failure");
        assert!(
            self.sb.step_runs().is_empty(),
            "a container started: {:?}",
            self.sb.step_runs()
        );
        assert!(
            o.stdout.is_empty() && o.stderr.is_empty(),
            "prepare must print nothing: {}",
            out_text(o)
        );
    }
}

fn step(name: &str, extra: &str) -> String {
    pipeline(&[(name, extra)])
}

#[test]
fn a_pipeline_with_secrets_is_refused_with_one_fixed_reason_and_no_name() {
    // The tick's job passes no secrets (feature 010, T017b): the names file stays empty.
    let p = Prep::new();
    let toml = format!(
        "{}{}",
        step("a", "secrets = [\"Z_KEY\", \"A_KEY\"]"),
        step("b", "secrets = [\"M_KEY\"]")
    );
    let o = p.run(&[(".factory-ci.toml", &toml)], &[]);
    p.assert_refused(&o, "steps with secrets are not run by this job");
    let reason = std::fs::read_to_string(&p.reason).unwrap();
    assert!(!reason.contains("_KEY"), "{reason}");
    assert!(p.sb.step_runs().is_empty(), "prepare starts no container");
    assert!(p.api.requests().is_empty(), "prepare posts no status");
}

#[test]
fn a_pipeline_without_secrets_gives_an_empty_file_and_exit_0() {
    let p = Prep::new();
    let o = p.run(&[(".factory-ci.toml", &step("a", ""))], &[]);
    assert_eq!(o.status.code(), Some(0), "{}", out_text(&o));
    assert_eq!(std::fs::read_to_string(&p.names).unwrap(), "");
}

#[test]
fn prepare_ships_nothing_even_when_a_log_store_is_configured() {
    let p = Prep::new();
    let loki = Fake::start();
    let fixture = p.sb.fixture(&[(".factory-ci.toml", &step("a", ""))]);
    let o = p.run_with(&fixture, &[], |c| {
        log_env(c, &loki.url);
    });
    assert_eq!(o.status.code(), Some(0), "{}", out_text(&o));
    assert!(loki.requests().is_empty());
}

#[test]
fn invalid_pipelines_are_refused_with_one_reason_line() {
    // FR-039, FR-041, US1 AS 1.5
    let big: String = (0..101).map(|i| step(&format!("s{i}"), "")).collect();
    let cases: Vec<(&str, String, &str)> = vec![
        ("invalid toml", "[steps.a\nimage =".to_string(), "invalid"),
        (
            "cycle",
            format!(
                "{}{}",
                step("a", "depends_on = [\"b\"]"),
                step("b", "depends_on = [\"a\"]")
            ),
            "cycl",
        ),
        ("101 steps", big, "101"),
        ("bad name 1BAD", step("a", "secrets = [\"1BAD\"]"), "1BAD"),
        ("bad name A-B", step("a", "secrets = [\"A-B\"]"), "A-B"),
        (
            "reserved BWS",
            step("a", "secrets = [\"BWS_ACCESS_TOKEN\"]"),
            "BWS_ACCESS_TOKEN",
        ),
        (
            "reserved token",
            step("a", "secrets = [\"FACTORY_CI_TOKEN\"]"),
            "FACTORY_CI_TOKEN",
        ),
        (
            "reserved log",
            step("a", "secrets = [\"FACTORY_CI_LOG_URL\"]"),
            "FACTORY_CI_LOG_URL",
        ),
        (
            "docker without opt-in",
            step("img", "docker = true"),
            "img needs docker",
        ),
    ];
    for (_label, toml, needle) in cases {
        let p = Prep::new();
        let o = p.run(&[(".factory-ci.toml", &toml)], &[]);
        p.assert_refused(&o, needle);
    }
}

#[test]
fn docker_with_the_opt_in_is_accepted() {
    let p = Prep::new();
    let o = p.run(
        &[(".factory-ci.toml", &step("img", "docker = true"))],
        &["--allow-docker"],
    );
    assert_eq!(o.status.code(), Some(0), "{}", out_text(&o));
}

#[test]
fn a_symlinked_pipeline_file_is_refused() {
    let p = Prep::new();
    let fixture = p.sb.fixture(&[("real.toml", &step("a", ""))]);
    std::os::unix::fs::symlink(fixture.join("real.toml"), fixture.join(".factory-ci.toml"))
        .unwrap();
    let o = p.run_with(&fixture, &[], |_| {});
    p.assert_refused(&o, "symlink");
}

#[test]
fn a_failed_clone_gives_clone_failed() {
    let p = Prep::new();
    let fixture = p.sb.fixture(&[(".factory-ci.toml", &step("a", ""))]);
    let o = p.run_with(&fixture, &[], |c| {
        c.env("STUB_CLONE_FAIL", "1");
    });
    assert_eq!(o.status.code(), Some(1));
    assert!(p.reason().contains("clone failed"), "{:?}", p.reason());
    assert!(!p.names.exists());
}

#[test]
fn an_unpinned_clone_image_exits_before_cloning() {
    // FR-057
    let unpinned = [
        None,
        Some("alpine/git"),
        Some("alpine/git@sha256:abc"),
        Some("alpine/git@sha256:"),
    ];
    for image in unpinned {
        let p = Prep::new();
        let fixture = p.sb.fixture(&[(".factory-ci.toml", &step("a", ""))]);
        let o = p.run_with(&fixture, &[], |c| match image {
            Some(i) => {
                c.env("FACTORY_CI_CLONE_IMAGE", i);
            }
            None => {
                c.env_remove("FACTORY_CI_CLONE_IMAGE");
            }
        });
        assert_eq!(o.status.code(), Some(1), "{image:?}: {}", out_text(&o));
        let said = format!("{}{}", p.reason(), out_text(&o));
        assert!(said.contains("image is not pinned"), "{image:?}: {said}");
        assert!(p.sb.calls().is_empty(), "{image:?}: cloned anyway");
        assert!(!p.names.exists());
    }
}

#[test]
fn the_clone_token_travels_in_the_environment_only() {
    let p = Prep::new();
    p.run(&[(".factory-ci.toml", &step("a", ""))], &[]);
    let calls = p.sb.calls();
    assert!(!calls.is_empty());
    assert!(
        calls.iter().all(|c| !c.join("\n").contains(JOB_TOKEN)),
        "token in argv: {calls:?}"
    );
    assert!(
        p.sb.client_envs()
            .contains(&format!("FCI_TOKEN={JOB_TOKEN}")),
        "token not in the client environment"
    );
}

#[test]
fn a_docker_pipeline_with_an_unpinned_dind_image_is_refused() {
    // FR-057: the sidecar image is pinned by digest like the clone image
    for image in [None, Some("docker:27.5.1-dind"), Some("docker@sha256:abc")] {
        let p = Prep::new();
        let fixture =
            p.sb.fixture(&[(".factory-ci.toml", &step("img", "docker = true"))]);
        let o = p.run_with(&fixture, &["--allow-docker"], |c| match image {
            Some(i) => {
                c.env("FACTORY_CI_DIND_IMAGE", i);
            }
            None => {
                c.env_remove("FACTORY_CI_DIND_IMAGE");
            }
        });
        p.assert_refused(&o, "FACTORY_CI_DIND_IMAGE");
    }
    // a pipeline without docker steps does not need the variable
    let p = Prep::new();
    let fixture = p.sb.fixture(&[(".factory-ci.toml", &step("a", ""))]);
    let o = p.run_with(&fixture, &[], |c| {
        c.env_remove("FACTORY_CI_DIND_IMAGE");
    });
    assert_eq!(o.status.code(), Some(0), "{}", out_text(&o));
}
