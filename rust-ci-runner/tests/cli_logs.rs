//! T035. Log-shipping flags on the command line. FR-030, FR-037, FR-049, FR-059; AS 11.8, 11.9.

mod common;
use common::*;

fn project(sb: &Sandbox) -> std::path::PathBuf {
    let d = sb.dir.join("proj");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join(".factory-ci.toml"), pipeline(&[("build", "")])).unwrap();
    d
}

#[test]
fn run_quiet_logs_without_a_log_environment_fails_before_running_anything() {
    // AS 11.8
    let sb = Sandbox::new();
    let d = project(&sb);
    let o = sb
        .cmd(&["run", "--quiet-logs", "--dir", dir_str(&d)])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1), "{}", out_text(&o));
    assert!(
        stderr(&o).contains("FACTORY_CI_LOG_URL"),
        "the reason names the variables: {}",
        stderr(&o)
    );
    assert!(sb.calls().is_empty(), "{:?}", sb.calls());
}

#[test]
fn job_quiet_logs_without_a_log_environment_fails_before_cloning() {
    // AS 11.8
    let sb = Sandbox::new();
    let api = Fake::start();
    let mut c = sb.cmd(&job_args(&["--quiet-logs"]));
    job_env(&mut c, &api.url);
    let o = c.output().unwrap();
    assert_eq!(o.status.code(), Some(1), "{}", out_text(&o));
    assert!(!stderr(&o).is_empty());
    assert!(sb.calls().is_empty(), "{:?}", sb.calls());
}

#[test]
fn job_private_without_a_log_store_exits_1_before_cloning_and_prints_nothing() {
    // FR-049, FR-059
    let sb = Sandbox::new();
    let api = Fake::start();
    let mut c = sb.cmd(&job_args(&["--private"]));
    job_env(&mut c, &api.url);
    let o = c.output().unwrap();
    assert_eq!(o.status.code(), Some(1), "{}", out_text(&o));
    assert!(o.stdout.is_empty(), "{}", stdout(&o));
    assert!(sb.calls().is_empty(), "{:?}", sb.calls());
    assert!(stderr(&o).contains("log store required"), "{}", stderr(&o));
}

#[test]
fn a_partial_log_environment_is_refused_by_job_and_run() {
    // FR-029
    let sb = Sandbox::new();
    let d = project(&sb);
    let mut c = sb.cmd(&["run", "--dir", dir_str(&d)]);
    c.env("FACTORY_CI_LOG_URL", "http://127.0.0.1:1");
    let o = c.output().unwrap();
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("FACTORY_CI_LOG_USER"));
    assert!(sb.calls().is_empty());
}

#[test]
fn run_without_configuration_prints_step_output() {
    // AS 11.9
    let sb = Sandbox::new();
    let d = project(&sb);
    let o = sb
        .cmd(&["run", "--dir", dir_str(&d)])
        .env("STUB_OUT", "STEP_OUT_1")
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", out_text(&o));
    assert!(stdout(&o).contains("[build] STEP_OUT_1"), "{}", stdout(&o));
}

#[test]
fn run_with_a_log_store_ships_and_prints() {
    // AS 11.9
    let sb = Sandbox::new();
    let d = project(&sb);
    let loki = Fake::start();
    let mut c = sb.cmd(&["run", "--dir", dir_str(&d)]);
    log_env(&mut c, &loki.url);
    c.env("STUB_OUT", "STEP_OUT_2");
    let o = c.output().unwrap();
    assert!(o.status.success(), "{}", out_text(&o));
    assert!(stdout(&o).contains("STEP_OUT_2"));
    assert!(
        loki.loki_lines()
            .iter()
            .any(|(_, t)| t.contains("STEP_OUT_2")),
        "{:?}",
        loki.loki_lines()
    );
}

#[test]
fn run_quiet_logs_with_a_log_store_ships_and_does_not_print() {
    // AS 11.9, FR-030
    let sb = Sandbox::new();
    let d = project(&sb);
    let loki = Fake::start();
    let mut c = sb.cmd(&["run", "--quiet-logs", "--dir", dir_str(&d)]);
    log_env(&mut c, &loki.url);
    c.env("STUB_OUT", "STEP_OUT_3");
    let o = c.output().unwrap();
    assert!(o.status.success(), "{}", out_text(&o));
    assert!(!out_text(&o).contains("STEP_OUT_3"), "{}", out_text(&o));
    assert!(loki
        .loki_lines()
        .iter()
        .any(|(_, t)| t.contains("STEP_OUT_3")));
}
