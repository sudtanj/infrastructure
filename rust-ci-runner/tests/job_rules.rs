//! T030. What `job` hands to step containers and what it refuses before any container.
//! FR-016, FR-018, FR-020, FR-021, FR-024; AS 3.3, 7.4, 7.6, 7.7, 9.2. Stub `docker` on PATH.

mod common;
use common::*;
use std::process::Output;

struct Job {
    sb: Sandbox,
    api: Fake,
}

impl Job {
    fn new() -> Job {
        Job { sb: Sandbox::new(), api: Fake::start() }
    }

    fn run(&self, files: &[(&str, &str)], extra: &[&str], env: &[(&str, &str)]) -> Output {
        let fixture = self.sb.fixture(files);
        let mut c = self.sb.cmd(&job_args(extra));
        job_env(&mut c, &self.api.url);
        c.env("STUB_FIXTURE", &fixture);
        for (k, v) in env {
            c.env(k, v);
        }
        c.output().unwrap()
    }

    fn last_status(&self) -> serde_json::Value {
        self.api.statuses().last().cloned().unwrap_or_default()
    }
}

fn args_of(runs: &[Vec<String>]) -> String {
    runs.iter().map(|r| r.join("\n")).collect::<Vec<_>>().join("\n--\n")
}

fn dash_e(run: &[String]) -> Vec<&str> {
    run.windows(2).filter(|w| w[0] == "-e").map(|w| w[1].as_str()).collect()
}

#[test]
fn the_runner_environment_never_becomes_a_container_environment() {
    // FR-024, FR-018, AS 7.4: token, log credentials and an unlisted fetched secret
    let j = Job::new();
    let loki = Fake::start();
    let toml = pipeline(&[("build", "")]);
    let mut c = j.sb.cmd(&job_args(&[]));
    job_env(&mut c, &j.api.url);
    log_env(&mut c, &loki.url);
    let fixture = j.sb.fixture(&[(".factory-ci.toml", &toml)]);
    c.env("STUB_FIXTURE", &fixture).env("UNLISTED_SECRET", "unlisted-value-1");
    let o = c.output().unwrap();
    assert!(o.status.success(), "{}", out_text(&o));
    let runs = j.sb.step_runs();
    assert!(!runs.is_empty());
    let all = args_of(&runs);
    for leak in [JOB_TOKEN, "loki-pass-77", "loki-user", "unlisted-value-1", "UNLISTED_SECRET"] {
        assert!(!all.contains(leak), "{leak} reached a step container:\n{all}");
    }
    for r in &runs {
        for name in dash_e(r) {
            assert!(!name.starts_with("FACTORY_CI_LOG") && !name.starts_with("FACTORY_CI_TOKEN"), "{name}");
        }
    }
    assert!(!all.contains("docker.sock") && !all.contains("--privileged"));
}

#[test]
fn a_listed_secret_is_passed_by_name_and_its_value_only_in_the_client_environment() {
    // AS 7.7
    let j = Job::new();
    let toml = pipeline(&[("deploy", "secrets = [\"NPM_TOKEN\"]")]);
    let o = j.run(&[(".factory-ci.toml", &toml)], &[], &[("NPM_TOKEN", "npm-value-4242")]);
    assert!(o.status.success(), "{}", out_text(&o));
    let runs = j.sb.step_runs();
    assert!(runs.iter().any(|r| dash_e(r).contains(&"NPM_TOKEN")), "{}", args_of(&runs));
    assert!(!args_of(&runs).contains("npm-value-4242"));
    assert!(j.sb.client_envs().contains("NPM_TOKEN=npm-value-4242"));
    assert!(!out_text(&o).contains("npm-value-4242"));
    assert!(!j.last_status().to_string().contains("npm-value-4242"));
}

#[test]
fn docker_steps_without_the_opt_in_end_in_error_before_any_step_container() {
    // AS 7.6, FR-021
    let j = Job::new();
    let toml = pipeline(&[("img", "docker = true")]);
    let o = j.run(&[(".factory-ci.toml", &toml)], &[], &[]);
    assert!(o.status.success(), "the final status was delivered: {}", out_text(&o));
    assert!(j.sb.step_runs().is_empty(), "{:?}", j.sb.step_runs());
    let s = j.last_status();
    assert_eq!(s["state"], "error");
    assert!(s["description"].as_str().unwrap().contains("step img needs docker but repo does not allow it"), "{s}");
}

#[test]
fn docker_steps_with_the_opt_in_start() {
    let j = Job::new();
    let toml = pipeline(&[("img", "docker = true")]);
    j.run(&[(".factory-ci.toml", &toml)], &["--allow-docker"], &[]);
    let s = j.last_status().to_string();
    assert!(!s.contains("does not allow it"), "{s}");
}

#[test]
fn a_missing_secret_ends_in_error_before_any_container() {
    // AS 7.7, FR-041
    let j = Job::new();
    let toml = pipeline(&[("deploy", "secrets = [\"NPM_TOKEN\"]")]);
    j.run(&[(".factory-ci.toml", &toml)], &[], &[]);
    assert!(j.sb.step_runs().is_empty());
    let s = j.last_status();
    assert_eq!(s["state"], "error");
    assert!(s["description"].as_str().unwrap().contains("requests secret NPM_TOKEN, which is not defined"), "{s}");
}

#[test]
fn a_folder_project_mounts_its_own_directory_never_the_repo_root() {
    // AS 3.3, FR-016
    let j = Job::new();
    let toml = pipeline(&[("build", "")]);
    j.run(&[("svc/.factory-ci.toml", &toml), ("other/secret.txt", "x")], &["--project", "svc"], &[]);
    let runs = j.sb.step_runs();
    assert!(!runs.is_empty());
    for r in &runs {
        let mount = r.windows(2).find(|w| w[0] == "-v").map(|w| w[1].clone()).unwrap();
        assert!(mount.ends_with("/svc:/workspace"), "{mount}");
        assert!(mount.starts_with(j.sb.tmp.to_str().unwrap()), "{mount}");
    }
}

#[test]
fn the_root_project_mounts_the_workspace_itself() {
    let j = Job::new();
    let toml = pipeline(&[("build", "")]);
    j.run(&[(".factory-ci.toml", &toml)], &[], &[]);
    for r in j.sb.step_runs() {
        let mount = r.windows(2).find(|w| w[0] == "-v").map(|w| w[1].clone()).unwrap();
        assert!(mount.ends_with(":/workspace") && !mount.contains("/./"), "{mount}");
    }
}

#[test]
fn hostile_project_paths_are_refused_before_any_container() {
    // FR-016, AS 9.2
    for project in ["../x", "/etc", "a/b", "a b", "-x", ".."] {
        let j = Job::new();
        let toml = pipeline(&[("build", "")]);
        let o = j.run(&[(".factory-ci.toml", &toml), ("x/.factory-ci.toml", &toml)], &["--project", project], &[]);
        assert!(j.sb.step_runs().is_empty(), "{project}: {:?}", j.sb.step_runs());
        let refused = !o.status.success() || j.last_status()["state"] == "error";
        assert!(refused, "{project}: {}", out_text(&o));
    }
}

#[test]
fn a_symlinked_project_directory_is_refused() {
    let j = Job::new();
    let elsewhere = j.sb.dir.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(elsewhere.join(".factory-ci.toml"), pipeline(&[("build", "")])).unwrap();
    let fixture = j.sb.fixture(&[("keep", "x")]);
    std::os::unix::fs::symlink(&elsewhere, fixture.join("link")).unwrap();
    let mut c = j.sb.cmd(&job_args(&["--project", "link"]));
    job_env(&mut c, &j.api.url);
    c.env("STUB_FIXTURE", &fixture);
    let o = c.output().unwrap();
    assert!(j.sb.step_runs().is_empty(), "{}", out_text(&o));
    assert!(!o.status.success() || j.last_status()["state"] == "error");
}

#[test]
fn every_step_container_drops_the_three_capabilities() {
    // FR-054
    let j = Job::new();
    let toml = pipeline(&[("a", ""), ("b", "depends_on = [\"a\"]")]);
    j.run(&[(".factory-ci.toml", &toml)], &[], &[]);
    let runs = j.sb.step_runs();
    assert!(runs.len() >= 2);
    for r in &runs {
        for cap in ["NET_RAW", "MKNOD", "SETFCAP"] {
            assert!(r.windows(2).any(|w| w[0] == "--cap-drop" && w[1] == cap), "{cap} missing in {r:?}");
        }
    }
}

#[test]
fn a_docker_step_with_an_unpinned_dind_image_ends_in_error_before_any_step_container() {
    // FR-057
    let j = Job::new();
    let toml = pipeline(&[("img", "docker = true")]);
    let o = j.run(&[(".factory-ci.toml", &toml)], &["--allow-docker"], &[("FACTORY_CI_DIND_IMAGE", "docker:27.5.1-dind")]);
    assert!(o.status.success(), "the final status was delivered: {}", out_text(&o));
    assert!(j.sb.step_runs().is_empty(), "{:?}", j.sb.step_runs());
    let s = j.last_status();
    assert_eq!(s["state"], "error");
    assert!(s["description"].as_str().unwrap().contains("image is not pinned"), "{s}");
}

#[test]
fn a_failed_final_status_prints_the_code_and_never_the_request_path() {
    // FR-024, R4
    for (code, shown) in [(500u16, "HTTP 500"), (403, "HTTP 403")] {
        let j = Job::new();
        j.api.code.store(code, std::sync::atomic::Ordering::Relaxed);
        let o = j.run(&[(".factory-ci.toml", &pipeline(&[("a", "")]))], &[], &[]);
        assert!(!o.status.success());
        let err = stderr(&o);
        assert!(err.contains("final status not delivered"), "{err}");
        assert!(err.contains(shown), "{err}");
        assert!(!err.contains("o/r") && !err.contains(SHA), "{err}");
    }
}
