//! T052. The binary knows no secret vendor. FR-038, FR-046; I19.

mod common;
use common::*;

#[test]
fn no_source_file_but_pipeline_rs_names_the_vendor() {
    // `pipeline.rs` holds the reserved `BWS_` prefix. The `ci` command is the one place that reads
    // Bitwarden: `store.rs` and `ci.rs`, the engine's token scrub, `main.rs` (token and usage) and the
    // fixed fault lines in `log.rs`. Test files may name anything.
    let allowed = ["pipeline.rs", "store.rs", "ci.rs", "engine.rs", "main.rs", "log.rs"];
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = vec![];
    for entry in std::fs::read_dir(&src).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".rs")
            || allowed.contains(&name.as_str())
            || name.ends_with("_tests.rs")
        {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap().to_lowercase();
        for word in ["bws", "bitwarden"] {
            if text.contains(word) {
                offenders.push(format!("{name} mentions {word}"));
            }
        }
    }
    assert!(offenders.is_empty(), "{offenders:?}");
}

#[test]
fn the_source_directory_was_found() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert!(src.join("lib.rs").exists());
}

#[test]
fn run_completes_with_no_store_variable_set() {
    // FR-046: `run` never reads a secret store and needs none
    let sb = Sandbox::new();
    let d = sb.dir.join("proj");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join(".factory-ci.toml"), pipeline(&[("build", "")])).unwrap();
    let o = sb.cmd(&["run", "--dir", dir_str(&d)]).output().unwrap();
    assert!(o.status.success(), "{}", out_text(&o));
    let envs = sb.client_envs();
    assert!(
        !envs.contains("BWS_") && !envs.contains("FACTORY_CI_BWS_"),
        "{envs}"
    );
}
