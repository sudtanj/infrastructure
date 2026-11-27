//! Uses a stand-in for the engine CLI that records its arguments and environment, to show
//! what the host process list can see. No container engine is needed.

use rust_ci_runner::engine::{execute, Engine, RunCtx, State};
use rust_ci_runner::log::Log;
use rust_ci_runner::pipeline;
use rust_ci_runner::redact::Redactor;
use rust_ci_runner::util::random_id;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const TOKEN: &str = "tok-SECRET-789";

#[test]
fn tokens_and_secrets_travel_in_the_environment_never_in_arguments() {
    let dir: PathBuf = std::env::temp_dir().join(format!("fci-argv-{}", random_id()));
    std::fs::create_dir_all(&dir).unwrap();
    let stub = dir.join("engine.sh");
    std::fs::write(
        &stub,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.argv\"\nenv > \"$0.env\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    let argv = || std::fs::read_to_string(dir.join("engine.sh.argv")).unwrap();
    let env = || std::fs::read_to_string(dir.join("engine.sh.env")).unwrap();
    let engine = Engine::new(stub.to_str().unwrap());

    // Clone.
    let ws = dir.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let log = Log::new(Box::new(std::io::sink()), Redactor::new([]), 1 << 20);
    let end = Instant::now() + Duration::from_secs(30);
    let (url, sha) = ("https://host/o/r.git", "a".repeat(40));
    let st = engine.clone_repo("alpine/git", "run1", &ws, url, TOKEN, &sha, &[], &log, end);
    assert_eq!(st, State::Success);
    assert!(!argv().contains(TOKEN), "token in argv:\n{}", argv());
    assert!(env().contains(&format!("FCI_TOKEN={TOKEN}")));

    // A step: secrets by name, and an image or command with shell syntax stays one argument.
    let marker = dir.join("PWNED");
    let image = format!("x; touch {}", marker.display());
    let p = pipeline::parse(&format!(
        "[steps.s]\nimage = {image:?}\nsecrets = [\"NPM\"]\ncommands = ['echo \"$NPM\" ; echo $(id)']\n"
    ))
    .unwrap();
    let ctx = RunCtx {
        engine: &engine,
        run_id: "run1",
        workspace: &ws,
        env: vec![],
        secrets: [("NPM".to_string(), TOKEN.to_string())]
            .into_iter()
            .collect(),
        max_parallel: 1,
        deadline: Instant::now() + Duration::from_secs(30),
        log: &log,
        dind_image: "docker:dind",
    };
    let out = execute(&ctx, &p).unwrap();
    assert!(out.ok());
    let a = argv();
    assert!(!a.contains(TOKEN), "secret in argv:\n{a}");
    assert!(
        a.lines().any(|l| l == image),
        "image must be one argument:\n{a}"
    );
    assert!(a.lines().any(|l| l == "NPM"));
    assert!(env().lines().any(|l| l == format!("NPM={TOKEN}")));
    assert!(!marker.exists());
    assert!(!a.contains("--privileged") && !a.contains("docker.sock"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_step_container_drops_net_raw_mknod_and_setfcap() {
    // FR-054 (T030)
    let dir: PathBuf = std::env::temp_dir().join(format!("fci-caps-{}", random_id()));
    std::fs::create_dir_all(&dir).unwrap();
    let stub = dir.join("engine.sh");
    std::fs::write(
        &stub,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$0.argv\"\necho -- >> \"$0.argv\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    let engine = Engine::new(stub.to_str().unwrap());
    let ws = dir.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let log = Log::new(Box::new(std::io::sink()), Redactor::new([]), 1 << 20);
    let p = pipeline::parse(
        "[steps.a]\nimage = \"x\"\ncommands = [\"true\"]\n[steps.b]\nimage = \"x\"\ncommands = [\"true\"]\ndepends_on = [\"a\"]\n",
    )
    .unwrap();
    let ctx = RunCtx {
        engine: &engine,
        run_id: "run2",
        workspace: &ws,
        env: vec![],
        secrets: Default::default(),
        max_parallel: 1,
        deadline: Instant::now() + Duration::from_secs(30),
        log: &log,
        dind_image: "docker:dind",
    };
    assert!(execute(&ctx, &p).unwrap().ok());
    let text = std::fs::read_to_string(dir.join("engine.sh.argv")).unwrap();
    let calls: Vec<Vec<&str>> = text.split("--\n").map(|c| c.lines().collect()).collect();
    let steps: Vec<_> = calls
        .iter()
        .filter(|c| {
            c.first() == Some(&"run") && c.iter().any(|a| a.ends_with("-a") || a.ends_with("-b"))
        })
        .collect();
    assert_eq!(steps.len(), 2, "{text}");
    for c in steps {
        for cap in ["NET_RAW", "MKNOD", "SETFCAP"] {
            assert!(
                c.windows(2).any(|w| w[0] == "--cap-drop" && w[1] == cap),
                "{cap} missing:\n{c:?}"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
