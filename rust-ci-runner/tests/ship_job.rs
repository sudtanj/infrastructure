//! T034. `job` with log shipping, a stub engine that prints markers, a fake Loki and a fake
//! status server. FR-030, FR-031, FR-035, FR-059; AS 11.1 to 11.3, 11.6; SC-009, SC-011;
//! I11, I16, I17.

mod common;
use common::*;
use std::process::Output;
use std::time::{Duration, Instant};

const OUT: &str = "OUT_MARKER_1";
const ERR: &str = "ERR_MARKER_2";

struct Case {
    sb: Sandbox,
    gh: Fake,
    loki: Fake,
}

impl Case {
    fn new() -> Case {
        Case {
            sb: Sandbox::new(),
            gh: Fake::start(),
            loki: Fake::start(),
        }
    }

    fn run(&self, toml: &str, extra: &[&str], env: &[(&str, &str)], with_loki: bool) -> Output {
        let fixture = self.sb.fixture(&[(".factory-ci.toml", toml)]);
        let mut c = self.sb.cmd(&job_args(extra));
        job_env(&mut c, &self.gh.url);
        if with_loki {
            log_env(&mut c, &self.loki.url);
        }
        c.env("STUB_FIXTURE", &fixture)
            .env("STUB_OUT", format!("{OUT} npm-val-123"))
            .env("STUB_ERR", ERR);
        for (k, v) in env {
            c.env(k, v);
        }
        c.output().unwrap()
    }

    fn last(&self) -> serde_json::Value {
        self.gh.statuses().last().cloned().unwrap_or_default()
    }
}

#[test]
fn with_shipping_on_stdout_holds_only_events_and_the_markers_reach_loki_redacted() {
    // AS 11.1, FR-030, FR-032, I17
    let c = Case::new();
    let toml = pipeline(&[("build", "secrets = [\"NPM_TOKEN\"]")]);
    let o = c.run(&toml, &[], &[("NPM_TOKEN", "npm-val-123")], true);
    assert!(o.status.success(), "{}", out_text(&o));
    let all = out_text(&o);
    assert!(
        !all.contains(OUT) && !all.contains(ERR),
        "step output on the job's streams:\n{all}"
    );
    assert!(!all.contains("npm-val-123"));
    for l in stdout(&o).lines() {
        let event = l.starts_with("[build] ")
            && ["started", "success in", "failed in", "skipped", "timed out"]
                .iter()
                .any(|e| l.contains(e));
        assert!(event || l.contains(RUN_ID), "unexpected stdout line: {l:?}");
    }
    let lines = c.loki.loki_lines();
    let out = lines
        .iter()
        .find(|(_, t)| t.contains(OUT))
        .expect("stdout marker in Loki");
    assert_eq!(out.0["stream"], "stdout");
    assert!(
        out.1.contains("***") && !out.1.contains("npm-val-123"),
        "{}",
        out.1
    );
    let err = lines
        .iter()
        .find(|(_, t)| t.contains(ERR))
        .expect("stderr marker in Loki");
    assert_eq!(err.0["stream"], "stderr");
    assert_eq!(err.0["step"], "build");
    assert!(lines.iter().all(|(_, t)| !t.contains("npm-val-123")));
}

#[test]
fn clone_output_reaches_loki_under_its_own_step_label() {
    // FR-032
    let c = Case::new();
    c.run(&pipeline(&[("build", "")]), &[], &[], true);
    let lines = c.loki.loki_lines();
    assert!(
        lines
            .iter()
            .any(|(l, t)| l["step"] == "clone" && t.contains("stub clone output")),
        "{lines:?}"
    );
}

#[test]
fn the_final_status_link_has_the_run_id_filled_in_from_the_template() {
    // AS 11.2, FR-031
    let c = Case::new();
    c.run(
        &pipeline(&[("build", "")]),
        &[],
        &[(
            "FACTORY_CI_LOG_LINK",
            "https://g.example/explore?run={run_id}",
        )],
        true,
    );
    assert_eq!(
        c.last()["target_url"],
        format!("https://g.example/explore?run={RUN_ID}")
    );
}

#[test]
fn without_a_template_there_is_no_link() {
    let c = Case::new();
    c.run(&pipeline(&[("build", "")]), &[], &[], true);
    assert!(
        c.last().get("target_url").is_none_or(|v| v.is_null()),
        "{}",
        c.last()
    );
}

#[test]
fn with_shipping_off_the_link_is_the_actions_run_url() {
    // AS 11.3
    let c = Case::new();
    let env = [
        ("GITHUB_SERVER_URL", "https://github.com"),
        ("GITHUB_REPOSITORY", "b/b"),
        ("GITHUB_RUN_ID", "555"),
    ];
    let o = c.run(&pipeline(&[("build", "")]), &[], &env, false);
    assert!(o.status.success(), "{}", out_text(&o));
    assert_eq!(
        c.last()["target_url"],
        "https://github.com/b/b/actions/runs/555"
    );
    assert!(
        stdout(&o).contains(OUT),
        "shipping off prints step output as before"
    );
}

#[test]
fn a_shipper_failure_never_changes_the_result_state() {
    // I16: 401 from Loki versus no shipping at all
    let ok = Case::new();
    ok.run(&pipeline(&[("build", ""), ("test", "")]), &[], &[], false);
    let bad = Case::new();
    bad.loki
        .code
        .store(401, std::sync::atomic::Ordering::Relaxed);
    bad.run(&pipeline(&[("build", ""), ("test", "")]), &[], &[], true);
    assert_eq!(ok.last()["state"], "success");
    assert_eq!(bad.last()["state"], ok.last()["state"]);

    let ok = Case::new();
    let mut c = ok.sb.cmd(&job_args(&[]));
    job_env(&mut c, &ok.gh.url);
    let fixture = ok
        .sb
        .fixture(&[(".factory-ci.toml", &pipeline(&[("bad", "")]))]);
    c.env("STUB_FIXTURE", &fixture).env("STUB_FAIL", "-bad");
    c.output().unwrap();
    assert_eq!(ok.last()["state"], "failure");
}

#[test]
fn a_dead_loki_keeps_the_state_and_appends_the_suffix_to_a_long_reason() {
    // AS 11.6, SC-011, I11. Takes about 60 s: the shipper's retry window.
    let c = Case::new();
    c.loki.code.store(503, std::sync::atomic::Ordering::Relaxed);
    let names = [
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "cccccccccccccccccccccccccccccccc",
    ];
    let toml: String = names
        .iter()
        .map(|n| pipeline(&[(*n, "secrets = [\"MISSING_ONE\"]")]))
        .collect();
    let started = Instant::now();
    let o = c.run(&toml, &[], &[], true);
    assert!(o.status.success(), "{}", out_text(&o));
    assert!(
        started.elapsed() < Duration::from_secs(60 + 45),
        "{:?}",
        started.elapsed()
    );
    let s = c.last();
    assert_eq!(s["state"], "error");
    let d = s["description"].as_str().unwrap();
    assert!(d.ends_with(" (logs not delivered)"), "{d}");
    assert!(d.chars().count() <= 140, "{d}");
}

#[test]
fn a_private_row_prints_nothing_and_its_events_go_only_to_loki() {
    // FR-059, I17
    let c = Case::new();
    let o = c.run(&pipeline(&[("build", "")]), &["--private"], &[], true);
    assert!(o.status.success(), "{}", out_text(&o));
    assert!(o.stdout.is_empty(), "stdout: {}", stdout(&o));
    let lines = c.loki.loki_lines();
    assert!(
        lines
            .iter()
            .any(|(l, t)| l["step"] == "build" && t.ends_with("started")),
        "{lines:?}"
    );
    assert!(lines.iter().any(|(_, t)| t.contains(OUT)));
}
