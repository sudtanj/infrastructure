//! 012 T004, T006 (test-first, plan step S1). Targets `image::{check, name_ok, publishes, image_project}` and
//! the constants of data-model.md "Constants" and "Check rules".
//!
//! Registered in `lib.rs` beside `pub mod image;` (T005); only the `docker/hello-static` test stays ignored until T018.
//!
//! Every failing case is the passing fixture (`tests/fixtures/image/pass`) with one change, built in memory, so a
//! rule that never fires cannot hide: `the_passing_fixture_returns_empty` pins the baseline and each helper asserts
//! that the text it rewrites is still in the fixture.

use crate::image::{
    check, image_project, name_ok, publishes, ARCHIVE_MAX, CRANE_IMAGE, HUB_BASE, NAMESPACE,
    PLATFORMS, PUBLISH_BRANCH, REGISTRY, TAG_HEX, TOKEN_NAME, USERNAME_NAME,
};
use std::path::{Path, PathBuf};

const DF: &str = include_str!("../tests/fixtures/image/pass/Dockerfile");
const TOML: &str = include_str!("../tests/fixtures/image/pass/.factory-ci.toml");
const README: &str = include_str!("../tests/fixtures/image/pass/README.md");

fn hex(c: char) -> String {
    c.to_string().repeat(64)
}

struct Tmp(PathBuf);

impl Tmp {
    fn new() -> Tmp {
        let p = std::env::temp_dir().join(format!("fci-image-{}", crate::util::random_id()));
        std::fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn project(df: &[u8], toml: &str) -> Tmp {
    let t = Tmp::new();
    std::fs::write(t.0.join("Dockerfile"), df).unwrap();
    std::fs::write(t.0.join("README.md"), README).unwrap();
    std::fs::write(t.0.join(".factory-ci.toml"), toml).unwrap();
    t
}

fn run(t: &Tmp) -> Vec<&'static str> {
    check(&t.0)
}

fn ids(df: &str) -> Vec<&'static str> {
    run(&project(df.as_bytes(), TOML))
}

fn ids_toml(toml: &str) -> Vec<&'static str> {
    run(&project(DF.as_bytes(), toml))
}

fn df_with(from: &str, to: &str) -> String {
    assert!(DF.contains(from), "fixture no longer holds {from:?}");
    DF.replacen(from, to, 1)
}

fn toml_with(from: &str, to: &str) -> String {
    assert!(TOML.contains(from), "fixture no longer holds {from:?}");
    TOML.replacen(from, to, 1)
}

/// A line inside the build stage (not the final one).
fn in_build(line: &str) -> String {
    df_with("WORKDIR /src\n", &format!("WORKDIR /src\n{line}\n"))
}

/// A line inside the final stage, before `ENTRYPOINT`.
fn in_final(line: &str) -> String {
    df_with("ENTRYPOINT [", &format!("{line}\nENTRYPOINT ["))
}

fn expect_only(got: Vec<&'static str>, id: &str, case: &str) {
    assert_eq!(got, [id], "{case}");
}

fn expect_has(got: Vec<&'static str>, id: &str, case: &str) {
    assert!(got.contains(&id), "{case}: want {id} in {got:?}");
}

// ---- baseline, constants, name and path helpers ----

#[test]
fn the_passing_fixture_returns_empty() {
    // 012 T006; S1 "Passing fixture returns empty"
    assert_eq!(ids(DF), Vec::<&str>::new());
}

#[test]
fn the_fixture_dir_on_disk_passes_too() {
    // 012 T004: the committed fixture folder, not the in-memory copy
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/image/pass");
    assert_eq!(check(&dir), Vec::<&str>::new());
}

#[test]
#[ignore = "test-first"]
fn the_real_hello_static_passes() {
    // 012 T006 (needs T018): the shipped first image passes its own check; FR-006
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docker/hello-static");
    assert_eq!(check(&dir), Vec::<&str>::new());
}

#[test]
fn constants_match_the_data_model() {
    // 012 T005; data-model.md "Constants"
    assert_eq!(NAMESPACE, "sudtanj");
    assert_eq!(PUBLISH_BRANCH, "main");
    assert_eq!(TOKEN_NAME, "DOCKERHUB_TOKEN");
    assert_eq!(USERNAME_NAME, "DOCKERHUB_USERNAME");
    assert_eq!(HUB_BASE, "https://hub.docker.com");
    assert_eq!(REGISTRY, "index.docker.io");
    assert_eq!(TAG_HEX, 12);
    assert_eq!(ARCHIVE_MAX, 2 * 1024 * 1024 * 1024);
    assert_eq!(PLATFORMS[0], ("amd64", "image-amd64.tar"));
    assert_eq!(PLATFORMS[1], ("arm64", "image-arm64.tar"));
    assert_eq!(PLATFORMS.len(), 2);
    let digest = CRANE_IMAGE
        .strip_prefix("gcr.io/go-containerregistry/crane:debug@sha256:")
        .unwrap_or_else(|| panic!("CRANE_IMAGE is not the pinned crane:debug image"));
    assert!(digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()));
}

#[test]
fn name_ok_boundaries_2_and_64_pass_1_and_65_fail() {
    // 012 T006; data-model.md Image project.name; FR-002
    assert!(!name_ok("a"), "length 1");
    assert!(name_ok("ab"), "length 2");
    assert!(name_ok(&"a".repeat(64)), "length 64");
    assert!(!name_ok(&"a".repeat(65)), "length 65");
    assert!(!name_ok(""));
}

#[test]
fn name_ok_follows_the_dash_rule() {
    // 012 T006; `^[a-z0-9]+(-[a-z0-9]+)*$`
    for good in ["hello-static", "a-b", "00", "a1-b2-c3", "x9"] {
        assert!(name_ok(good), "{good}");
    }
    for bad in [
        "-ab", "ab-", "a--b", "Ab", "aB", "a_b", "a.b", "a b", "ab/cd", "../x", "ab\n", "é1", "a-",
        "-",
    ] {
        assert!(!name_ok(bad), "{bad:?}");
    }
}

#[test]
fn publishes_needs_docker_prefix_and_main() {
    // 012 T005; data-model.md `publishes`; FR-005, FR-023
    assert!(publishes("docker/x", "main"));
    assert!(publishes("docker/hello-static", "main"));
    assert!(!publishes("docker/x", "dev"));
    assert!(!publishes("docker/x", "Main"));
    assert!(!publishes("docker/x", "main2"));
    assert!(!publishes("docker/x", ""));
    assert!(!publishes("rust-ci-runner", "main"));
    assert!(!publishes("ci-canary", "main"));
    assert!(!publishes(".", "main"));
    assert!(!publishes("dockerx/y", "main"));
    assert!(!publishes("docker", "main"));
    assert!(!publishes("a/docker/x", "main"));
}

#[test]
fn image_project_is_only_the_docker_prefix() {
    // 012 T005
    assert!(image_project("docker/x"));
    assert!(image_project("docker/hello-static"));
    assert!(!image_project("rust-ci-runner"));
    assert!(!image_project("."));
    assert!(!image_project("docker"));
    assert!(!image_project("dockerx/y"));
    assert!(!image_project("a/docker/x"));
}

// ---- layout ----

#[test]
fn layout_fails_for_each_missing_required_file() {
    // 012 T006; Image project.required files
    for gone in ["Dockerfile", "README.md", ".factory-ci.toml"] {
        let t = project(DF.as_bytes(), TOML);
        std::fs::remove_file(t.0.join(gone)).unwrap();
        expect_has(run(&t), "layout", gone);
    }
}

#[test]
fn layout_fails_for_an_empty_folder_and_a_missing_folder() {
    // 012 T006
    let t = Tmp::new();
    expect_has(run(&t), "layout", "empty folder");
    expect_has(check(&t.0.join("nope")), "layout", "missing folder");
}

#[test]
fn layout_fails_for_a_symlink_or_directory_in_place_of_a_file() {
    // 012 T006; each required file must be a regular file, not a symlink (read with symlink_metadata)
    use std::os::unix::fs::symlink;
    for name in ["Dockerfile", "README.md", ".factory-ci.toml"] {
        let t = project(DF.as_bytes(), TOML);
        let other = t.0.join("go.mod");
        std::fs::write(&other, "module x\n").unwrap();
        let p = t.0.join(name);
        std::fs::remove_file(&p).unwrap();
        symlink(&other, &p).unwrap();
        expect_has(run(&t), "layout", &format!("symlink {name}"));

        let t = project(DF.as_bytes(), TOML);
        let p = t.0.join(name);
        std::fs::remove_file(&p).unwrap();
        std::fs::create_dir(&p).unwrap();
        expect_has(run(&t), "layout", &format!("directory {name}"));
    }
}

#[test]
fn layout_fails_for_a_dockerfile_over_256_kib() {
    // 012 T006; S1 "capped at 256 KB"; a 1 MB file too
    for size in [256 * 1024 + 1, 1 << 20] {
        let pad = format!("# {}\n", "x".repeat(size));
        expect_has(ids(&format!("{pad}{DF}")), "layout", &format!("{size}"));
    }
}

#[test]
fn layout_fails_for_nul_and_invalid_utf8_and_never_panics() {
    // 012 T006; S1 "NUL and invalid UTF-8 return layout, never panic"
    let nul = format!("{DF}\0");
    expect_has(run(&project(nul.as_bytes(), TOML)), "layout", "NUL");
    let mut bad = DF.as_bytes().to_vec();
    bad.extend_from_slice(&[0xff, 0xfe, b'\n']);
    expect_has(run(&project(&bad, TOML)), "layout", "invalid UTF-8");
    let mut mid = b"# ".to_vec();
    mid.extend_from_slice(&[0xc3, 0x28]);
    mid.push(b'\n');
    mid.extend_from_slice(DF.as_bytes());
    expect_has(
        run(&project(&mid, TOML)),
        "layout",
        "invalid UTF-8 first line",
    );
}

#[test]
fn check_never_panics_on_junk() {
    // 012 T006 (I2): 1 MB, NUL, invalid UTF-8, pseudo-random bytes; output stays sorted and unique
    let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as u8
    };
    let mut inputs: Vec<Vec<u8>> = vec![
        vec![],
        vec![0; 1 << 20],
        vec![b'\\'; 4096],
        b"FROM\nFROM \\\n\\\n<<\nRUN <<\n".to_vec(),
        b"RUN <<EOF\n".to_vec(),
        "\u{feff}".as_bytes().to_vec(),
        b"\r\r\r\n\\\r\n".to_vec(),
    ];
    for len in [1, 7, 64, 513, 4096, 70_000] {
        inputs.push((0..len).map(|_| next()).collect());
        let text: Vec<u8> = (0..len)
            .map(|_| b" \t\nFROMRUN<=-\\#$@:/a1"[next() as usize % 21])
            .collect();
        inputs.push(text);
    }
    for (i, junk) in inputs.iter().enumerate() {
        let got = run(&project(junk, "[steps.x\n\0"));
        assert!(
            got.windows(2).all(|w| w[0] < w[1]),
            "input {i}: not sorted and unique: {got:?}"
        );
    }
    expect_has(
        run(&project(&vec![b'a'; 1 << 20], TOML)),
        "layout",
        "1 MB of one byte",
    );
}

#[test]
fn output_is_sorted_and_each_id_once() {
    // 012 T006; "returns the ids that fail, sorted, once each"
    let df = format!("# syntax=x\n# escape=`\n{DF}ADD a b\nADD c d\nONBUILD RUN x\nVOLUME /v\n");
    let got = ids(&df);
    assert!(
        got.contains(&"directive") && got.contains(&"instruction"),
        "{got:?}"
    );
    let mut want = got.clone();
    want.sort();
    want.dedup();
    assert_eq!(got, want);
}

#[test]
fn crlf_bom_and_both_are_not_a_layout_problem() {
    // 012 T006; edge answers of `layout`
    let crlf = DF.replace('\n', "\r\n");
    assert_eq!(ids(&crlf), Vec::<&str>::new(), "CRLF");
    let bom = format!("\u{feff}{DF}");
    assert_eq!(ids(&bom), Vec::<&str>::new(), "BOM");
    let both = format!("\u{feff}{crlf}");
    assert_eq!(ids(&both), Vec::<&str>::new(), "BOM and CRLF");
    let toml_crlf = TOML.replace('\n', "\r\n");
    assert_eq!(ids_toml(&toml_crlf), Vec::<&str>::new(), "toml CRLF");
}

#[test]
fn crlf_and_bom_do_not_hide_a_bad_instruction() {
    // 012 T006: the strip must not drop the rule
    let bad = format!("\u{feff}{}", in_build("ADD a b")).replace('\n', "\r\n");
    expect_has(ids(&bad), "instruction", "BOM and CRLF ADD");
    let bad = format!("\u{feff}# syntax=x\r\n{DF}");
    expect_has(ids(&bad), "directive", "BOM then directive");
}

#[test]
fn keywords_are_case_insensitive_both_ways() {
    // 012 T006; "Instruction keywords are case-insensitive"
    let lower: String = DF
        .lines()
        .map(|l| {
            let (w, rest) = l.split_once(' ').unwrap_or((l, ""));
            if w.chars().all(|c| c.is_ascii_uppercase()) && !w.is_empty() {
                format!("{} {rest}\n", w.to_lowercase())
            } else {
                format!("{l}\n")
            }
        })
        .collect();
    assert!(lower.contains("from ") && lower.contains("run "), "{lower}");
    assert_eq!(ids(&lower), Vec::<&str>::new(), "lowercase fixture");
    expect_has(ids(&in_build("add a b")), "instruction", "add");
    expect_has(ids(&in_build("Add a b")), "instruction", "Add");
    expect_has(ids(&in_build("onbuild RUN x")), "instruction", "onbuild");
    let mixed = DF.replace("COPY --from=build", "Copy --from=BUILD");
    assert_eq!(
        ids(&mixed),
        Vec::<&str>::new(),
        "mixed-case copy and stage name"
    );
}

#[test]
fn a_commented_out_instruction_is_not_an_instruction() {
    // 012 T006; comment-FROM edge
    let df = format!("# FROM alpine\n# ADD a b\n    # ENV TOKEN=1\n{DF}\n# RUN true\n");
    assert_eq!(ids(&df), Vec::<&str>::new());
}

#[test]
fn continuations_join_and_comments_inside_them_are_skipped() {
    // 012 T006; "joins `\` continuations"
    let df = df_with(
        "RUN GOARCH=$TARGETARCH go build -trimpath -o /hello .",
        "RUN GOARCH=$TARGETARCH \\\n    go build \\\n    # a comment inside\n    -trimpath -o /hello .",
    );
    assert_eq!(ids(&df), Vec::<&str>::new());
    let tail = in_build("ENV A=1 \\\n    B=2");
    assert_eq!(ids(&tail), Vec::<&str>::new(), "ENV over two lines");
    expect_has(
        ids(&in_build("ENV A=1 \\\n    MY_SECRET=2")),
        "secret-name",
        "name on the continued line",
    );
}

// ---- directive ----

#[test]
fn directive_fails_for_each_spelling() {
    // 012 T006; data-model.md `directive`; S1 five spellings
    for line in [
        "# syntax=x",
        "#syntax = x",
        "# SYNTAX=x",
        "# Escape=`",
        "#   syntax=x",
        "\t# syntax=docker/dockerfile:1",
        "  #escape=\\",
        "# Syntax =x",
    ] {
        expect_only(ids(&format!("{line}\n{DF}")), "directive", line);
    }
}

#[test]
fn directive_fails_anywhere_in_the_file_and_in_a_heredoc_body() {
    // 012 T006; "anywhere in the file"
    let mid = in_build("# syntax=x");
    expect_has(ids(&mid), "directive", "after FROM");
    let end = format!("{DF}# escape=`\n");
    expect_has(ids(&end), "directive", "last line");
    let body = in_build("RUN <<EOF\n# syntax=x\nEOF");
    expect_has(ids(&body), "directive", "inside a RUN heredoc body");
}

#[test]
fn a_comment_that_only_mentions_syntax_is_fine() {
    // 012 T006: the rule needs `=`
    let df = format!("# syntax is pinned elsewhere\n# the escape hatch\n{DF}");
    assert_eq!(ids(&df), Vec::<&str>::new());
}

// ---- instruction ----

#[test]
fn instruction_fails_for_each_disallowed_keyword() {
    // 012 T006; S1 bypasses `ADD`, `ONBUILD`, `SHELL`, `HEALTHCHECK`, `VOLUME`; plus the table's others
    for line in [
        "ADD a.tar /x",
        "ADD https://example.test/x /x",
        "ONBUILD RUN true",
        "SHELL [\"/bin/sh\", \"-c\"]",
        "HEALTHCHECK CMD true",
        "VOLUME /data",
        "STOPSIGNAL SIGTERM",
        "MAINTAINER someone",
        "NOTANINSTRUCTION x",
    ] {
        expect_has(ids(&in_build(line)), "instruction", line);
    }
}

#[test]
fn the_allowlist_instructions_pass() {
    // 012 T006: FROM ARG ENV WORKDIR COPY RUN USER ENTRYPOINT CMD LABEL EXPOSE are not `instruction`
    let df =
        in_build("LABEL org.example.name=x\nEXPOSE 8080\nARG X=1\nCMD [\"/hello\"]\nUSER 1000");
    assert_eq!(ids(&df), Vec::<&str>::new());
}

// ---- heredoc ----

#[test]
fn heredoc_fails_in_env_arg_label_and_from() {
    // 012 T006; S1 `<<` in `ENV`, `ARG`, `LABEL`, `FROM`
    for line in [
        "ENV A=<<X",
        "ARG A=<<X",
        "LABEL a=<<X",
        "ENV A=<<-X",
        "ENV A=<<\"X\"",
    ] {
        expect_has(ids(&in_build(line)), "heredoc", line);
    }
    let from = df_with(
        &format!("FROM gcr.io/distroless/static@sha256:{}", hex('b')),
        &format!("FROM gcr.io/distroless/static@sha256:{} <<X", hex('b')),
    );
    expect_has(ids(&from), "heredoc", "FROM");
}

#[test]
fn run_and_copy_heredoc_bodies_are_skipped() {
    // 012 T006; S1: bodies are not instructions, a `FROM`/`ADD` inside is ignored
    for opener in ["RUN <<EOF", "RUN <<-EOF", "RUN <<\"EOF\"", "RUN <<'EOF'"] {
        let line =
            format!("{opener}\nADD x y\nFROM alpine\nCOPY --from=alpine a b\nENV TOKEN=1\nEOF");
        assert_eq!(ids(&in_build(&line)), Vec::<&str>::new(), "{opener}");
    }
    let copy = in_build("COPY <<EOF /out.txt\nFROM alpine\nADD x y\nEOF");
    assert_eq!(ids(&copy), Vec::<&str>::new(), "COPY <<EOF");
    let tab = in_build("RUN <<-EOF\n\tADD x y\n\tEOF");
    assert_eq!(
        ids(&tab),
        Vec::<&str>::new(),
        "<<- with a tab-indented terminator"
    );
}

#[test]
fn code_after_a_heredoc_terminator_is_checked_again() {
    // 012 T006: the skip ends at the terminator line
    let df = in_build("RUN <<EOF\necho hi\nEOF\nADD x y");
    expect_has(ids(&df), "instruction", "ADD after EOF");
    let df = in_build("RUN <<EOF\necho hi\nEOF\nENV API_TOKEN=1");
    expect_has(ids(&df), "secret-name", "ENV after EOF");
}

#[test]
fn heredoc_skip_survives_crlf() {
    // 012 T006
    let df = in_build("RUN <<EOF\nADD x y\nEOF").replace('\n', "\r\n");
    assert_eq!(ids(&df), Vec::<&str>::new());
}

// ---- external-source ----

#[test]
fn external_source_fails_for_a_from_that_is_not_an_earlier_stage() {
    // 012 T006; S1 `COPY --from=alpine`, `--from=docker.io/x`, `--from=5`, `RUN --mount=...from=alpine`
    let cases = [
        in_build("COPY --from=alpine /a /a"),
        in_build("COPY --from=docker.io/library/alpine /a /a"),
        in_build("COPY --from=5 /a /a"),
        in_build("RUN --mount=type=bind,from=alpine,target=/x true"),
        in_build("RUN --mount=type=cache,target=/c,from=alpine true"),
        // stage 1 is the final stage itself: "not earlier"
        df_with("COPY --from=build", "COPY --from=1"),
        // a stage cannot read itself or a later one
        in_build("COPY --from=build /a /a"),
    ];
    for (i, df) in cases.iter().enumerate() {
        expect_has(ids(df), "external-source", &format!("case {i}"));
    }
}

#[test]
fn external_source_passes_for_earlier_stage_names_and_numbers() {
    // 012 T006; `--from=build` and `--from=0` pass; names are case-insensitive
    assert_eq!(ids(DF), Vec::<&str>::new(), "--from=build");
    let by_number = df_with("COPY --from=build", "COPY --from=0");
    assert_eq!(ids(&by_number), Vec::<&str>::new(), "--from=0");
    let upper = df_with("COPY --from=build", "COPY --from=BUILD");
    assert_eq!(ids(&upper), Vec::<&str>::new(), "--from=BUILD");
    let mount = df_with(
        "FROM gcr.io/distroless/static",
        &format!(
            "FROM --platform=$BUILDPLATFORM gcr.io/distroless/static@sha256:{} AS mid\nRUN --mount=type=bind,from=build,target=/x true\nFROM gcr.io/distroless/static",
            hex('b')
        ),
    );
    assert_eq!(ids(&mount), Vec::<&str>::new(), "RUN --mount from=build");
}

// ---- from-unpinned, from-arg ----

#[test]
fn from_unpinned_fails_without_a_full_digest() {
    // 012 T006; data-model.md `from-unpinned`; tag only, short, 63 and 65 hex
    let a = hex('a');
    let base = format!("mirror.gcr.io/library/golang:1.24@sha256:{a}");
    for r in [
        "mirror.gcr.io/library/golang:1.24".to_string(),
        "mirror.gcr.io/library/golang".to_string(),
        "mirror.gcr.io/library/golang@sha256:abc".to_string(),
        format!("mirror.gcr.io/library/golang@sha256:{}", &a[..63]),
        format!("mirror.gcr.io/library/golang@sha256:{a}a"),
        format!("mirror.gcr.io/library/golang@sha512:{a}"),
        format!("mirror.gcr.io/library/golang@sha256:{a}:latest"),
    ] {
        expect_has(ids(&df_with(&base, &r)), "from-unpinned", &r);
    }
}

#[test]
fn from_unpinned_a_stage_name_counts_only_after_its_own_line() {
    // 012 T006; `FROM alpine AS alpine`
    let df = format!(
        "FROM --platform=$BUILDPLATFORM alpine AS alpine\nFROM gcr.io/distroless/static@sha256:{}\n",
        hex('b')
    );
    expect_has(ids(&df), "from-unpinned", "alpine AS alpine");
}

#[test]
fn from_unpinned_passes_scratch_stage_names_and_the_platform_flag() {
    // 012 T006; scratch, an earlier stage as the reference, flag is not the reference
    let df = "FROM scratch AS base\nCOPY hello /hello\nFROM base AS mid\nUSER 65532\nFROM mid\nENTRYPOINT [\"/hello\"]\n";
    assert_eq!(ids(df), Vec::<&str>::new(), "stage chain over scratch");
    assert_eq!(
        ids(DF),
        Vec::<&str>::new(),
        "--platform=$BUILDPLATFORM before the reference"
    );
}

#[test]
fn from_arg_fails_with_and_without_an_arg_default() {
    // 012 T006; data-model.md `from-arg`
    let a = hex('a');
    let with_default = format!("ARG BASE=mirror.gcr.io/library/golang:1.24@sha256:{a}\n{DF}")
        .replacen(
            &format!("mirror.gcr.io/library/golang:1.24@sha256:{a} AS build"),
            "$BASE AS build",
            1,
        );
    expect_has(ids(&with_default), "from-arg", "ARG default");
    let none = df_with(
        &format!("mirror.gcr.io/library/golang:1.24@sha256:{a}"),
        "${IMAGE}",
    );
    expect_has(ids(&none), "from-arg", "no default");
    let inside = df_with(
        &format!("golang:1.24@sha256:{a}"),
        &format!("golang:$V@sha256:{a}"),
    );
    expect_has(ids(&inside), "from-arg", "$ inside the reference");
}

// ---- emulation ----

#[test]
fn emulation_fails_when_a_run_stage_lacks_the_buildplatform_flag() {
    // 012 T006; FR-027
    let no_flag = df_with("FROM --platform=$BUILDPLATFORM ", "FROM ");
    expect_has(ids(&no_flag), "emulation", "stage 0 without the flag");
    let other = df_with("--platform=$BUILDPLATFORM", "--platform=linux/arm64");
    expect_has(
        ids(&other),
        "emulation",
        "a fixed platform is not BUILDPLATFORM",
    );
}

#[test]
fn emulation_passes_with_either_spelling_and_for_stages_without_run() {
    // 012 T006
    let braces = df_with("--platform=$BUILDPLATFORM", "--platform=${BUILDPLATFORM}");
    assert_eq!(ids(&braces), Vec::<&str>::new(), "${{BUILDPLATFORM}}");
    // the final stage of the fixture has no RUN and no flag
    assert_eq!(ids(DF), Vec::<&str>::new(), "no RUN, no flag");
}

#[test]
fn emulation_applies_to_a_middle_stage() {
    // 012 T006
    let df = df_with(
        "FROM gcr.io/distroless/static",
        &format!(
            "FROM mirror.gcr.io/library/golang:1.24@sha256:{} AS mid\nRUN true\nFROM gcr.io/distroless/static",
            hex('a')
        ),
    );
    expect_has(ids(&df), "emulation", "middle stage with RUN");
}

// ---- final-base, final-run, final-shell, final-user ----

#[test]
fn final_base_fails_for_debug_and_non_distroless_bases() {
    // 012 T006; data-model.md `final-base`
    let b = hex('b');
    let good = format!("gcr.io/distroless/static@sha256:{b}");
    for r in [
        format!("gcr.io/distroless/static:debug@sha256:{b}"),
        format!("gcr.io/distroless/static:debug-nonroot@sha256:{b}"),
        format!("gcr.io/distroless/base-debug@sha256:{b}"),
        format!("alpine@sha256:{b}"),
        format!("docker.io/library/busybox@sha256:{b}"),
        format!("gcr.io/distroless-evil/static@sha256:{b}"),
        format!("mirror.gcr.io/distroless/static@sha256:{b}"),
        format!("eu.gcr.io/distroless/static@sha256:{b}"),
    ] {
        expect_has(ids(&df_with(&good, &r)), "final-base", &r);
    }
}

#[test]
fn final_base_passes_scratch_and_distroless() {
    // 012 T006
    let b = hex('b');
    let good = format!("FROM gcr.io/distroless/static@sha256:{b}");
    assert_eq!(
        ids(&df_with(&good, "FROM scratch")),
        Vec::<&str>::new(),
        "scratch"
    );
    for r in [
        format!("gcr.io/distroless/base-nossl@sha256:{b}"),
        format!("gcr.io/distroless/static-debian12:nonroot@sha256:{b}"),
        format!("gcr.io/distroless/cc:latest@sha256:{b}"),
    ] {
        let df = df_with(&good, &format!("FROM {r}"));
        assert_eq!(ids(&df), Vec::<&str>::new(), "{r}");
    }
}

fn chain(n: usize) -> String {
    let mut df = String::from("FROM scratch AS s0\n");
    for i in 1..n {
        df.push_str(&format!("FROM s{} AS s{i}\n", i - 1));
    }
    df.push_str(&format!(
        "FROM s{}\nUSER 1000\nENTRYPOINT [\"/x\"]\n",
        n - 1
    ));
    df
}

#[test]
fn final_base_follows_stage_links_up_to_20() {
    // 012 T006; "chain depth limit 20". The ends of the range are the plan's words: a short chain ending
    // at scratch passes, a chain past the limit cannot be proved safe and fails (ambiguity reported to the lead).
    assert_eq!(ids(&chain(5)), Vec::<&str>::new(), "5 links");
    expect_has(ids(&chain(25)), "final-base", "25 links");
    let bad = chain(5).replace(
        "FROM scratch AS s0",
        &format!("FROM alpine@sha256:{} AS s0", hex('b')),
    );
    expect_has(
        ids(&bad),
        "final-base",
        "chain ends at a non-distroless base",
    );
}

#[test]
fn final_run_fails_for_a_run_anywhere_on_the_final_chain() {
    // 012 T006; data-model.md `final-run`
    expect_has(
        ids(&in_final("RUN true")),
        "final-run",
        "RUN in the final stage",
    );
    let df = "FROM --platform=$BUILDPLATFORM scratch AS base\nRUN true\nFROM base AS mid\nFROM mid\nUSER 1000\nENTRYPOINT [\"/x\"]\n";
    expect_has(
        ids(df),
        "final-run",
        "RUN in a stage the final stage extends",
    );
}

#[test]
fn a_run_in_a_stage_off_the_final_chain_is_not_final_run() {
    // 012 T006: the build stage of the fixture has a RUN and the final stage copies from it
    let got = ids(DF);
    assert!(!got.contains(&"final-run"), "{got:?}");
}

#[test]
fn final_shell_fails_for_shell_form_cmd_and_entrypoint() {
    // 012 T006; data-model.md `final-shell`
    let shell_entry = df_with("ENTRYPOINT [\"/hello\"]", "ENTRYPOINT /hello");
    expect_has(ids(&shell_entry), "final-shell", "ENTRYPOINT shell form");
    for line in ["CMD /hello", "CMD echo hi", "cmd /hello --x"] {
        expect_has(ids(&in_final(line)), "final-shell", line);
    }
    assert_eq!(
        ids(&in_final("CMD [\"/hello\"]")),
        Vec::<&str>::new(),
        "exec-form CMD"
    );
    // only the final stage counts
    assert_eq!(
        ids(&in_build("CMD echo build")),
        Vec::<&str>::new(),
        "shell-form CMD in a non-final stage is not flagged"
    );
}

#[test]
fn final_shell_fails_for_a_copy_of_a_shell_or_package_manager() {
    // 012 T006; the basename list of data-model.md
    for n in [
        "sh", "bash", "dash", "ash", "busybox", "apk", "apt", "apt-get", "dpkg", "yum", "dnf",
        "microdnf", "zypper", "pip", "npm",
    ] {
        expect_has(
            ids(&in_final(&format!(
                "COPY --from=build /out/{n} /usr/bin/{n}"
            ))),
            "final-shell",
            n,
        );
        expect_has(
            ids(&in_final(&format!("COPY {n} /usr/bin/tool"))),
            "final-shell",
            &format!("source {n}"),
        );
    }
    let near = in_final("COPY --from=build /hello /shy");
    assert_eq!(
        ids(&near),
        Vec::<&str>::new(),
        "basename must match exactly"
    );
}

#[test]
fn final_user_fails_when_absent_non_numeric_or_root() {
    // 012 T006; data-model.md `final-user`; `USER $X` fails as non-numeric
    for u in [
        "",
        "USER root\n",
        "USER 0\n",
        "USER 0:0\n",
        "USER 0:1000\n",
        "USER $X\n",
        "USER nobody\n",
        "USER 1000:abc\n",
        "USER 1000:\n",
        "USER :1000\n",
        "USER -1\n",
        "USER 1000:1000:1\n",
    ] {
        expect_has(
            ids(&df_with("USER 65532:65532\n", u)),
            "final-user",
            &format!("{u:?}"),
        );
    }
}

#[test]
fn final_user_passes_numeric_non_root_and_the_last_user_wins() {
    // 012 T006; `USER 1000:0` passes
    for u in [
        "USER 1000:0\n",
        "USER 65532\n",
        "USER 1\n",
        "USER 0\nUSER 65532\n",
    ] {
        assert_eq!(
            ids(&df_with("USER 65532:65532\n", u)),
            Vec::<&str>::new(),
            "{u:?}"
        );
    }
    expect_has(
        ids(&df_with("USER 65532:65532\n", "USER 65532\nUSER 0\n")),
        "final-user",
        "root last",
    );
}

#[test]
fn final_user_follows_the_chain_back() {
    // 012 T006: a USER set in a stage the final stage extends counts
    assert_eq!(ids(&chain(3)), Vec::<&str>::new());
    let df = "FROM scratch AS base\nUSER 1000\nFROM base\nENTRYPOINT [\"/x\"]\n";
    assert_eq!(
        ids(df),
        Vec::<&str>::new(),
        "USER inherited from the parent stage"
    );
}

// ---- secret-name ----

#[test]
fn secret_name_fails_for_credential_like_arg_and_env_names() {
    // 012 T006; data-model.md `secret-name`; both `ENV A=1 B=2` and `ENV KEY value` forms
    for line in [
        "ENV API_TOKEN=x",
        "ARG PASSWORD",
        "ARG my_password=1",
        "ENV A=1 MY_SECRET=2",
        "ENV KEY value",
        "ENV passwd=x",
        "ENV AWS_CREDENTIALS=x",
        "ENV MONKEY=1",
        "ARG GITHUB_TOKEN",
        "ENV token=x",
    ] {
        expect_only(ids(&in_build(line)), "secret-name", line);
        expect_has(
            ids(&in_final(line)),
            "secret-name",
            &format!("final: {line}"),
        );
    }
}

#[test]
fn secret_name_passes_for_plain_names_and_names_in_values() {
    // 012 T006; `ARG TARGETARCH` passes
    for line in [
        "ENV A=1 B=2",
        "ARG TARGETARCH",
        "ARG TARGETPLATFORM",
        "ENV GREETING=\"a token and a key\"",
        "RUN echo TOKEN",
    ] {
        let df = if line.starts_with("RUN") {
            in_build(&format!("{line} > /dev/null"))
        } else {
            in_build(line)
        };
        assert_eq!(ids(&df), Vec::<&str>::new(), "{line}");
    }
}

#[test]
fn secret_name_fails_for_a_toml_env_key() {
    // 012 T006; "or a `.factory-ci.toml` `env` key does"
    for k in ["API_TOKEN", "MY_PASSWORD", "SSH_KEY", "client_secret"] {
        let t = toml_with(
            "timeout = 1800\n",
            &format!("timeout = 1800\nenv = {{ {k} = \"x\" }}\n"),
        );
        expect_only(ids_toml(&t), "secret-name", k);
    }
    let ok = toml_with(
        "timeout = 1800\n",
        "timeout = 1800\nenv = { MODE = \"x\" }\n",
    );
    assert_eq!(ids_toml(&ok), Vec::<&str>::new());
}

// ---- secret-value ----

fn tok(prefix: &str, n: usize, c: char) -> String {
    format!("{prefix}{}", c.to_string().repeat(n))
}

#[test]
fn secret_value_fails_at_each_threshold_and_passes_one_below() {
    // 012 T006; data-model.md `secret-value`; synthetic strings only
    let cases: [(&str, usize, char); 9] = [
        ("dckr_pat_", 20, 'A'),
        ("ghp_", 30, 'a'),
        ("gho_", 30, 'a'),
        ("ghu_", 30, 'a'),
        ("ghs_", 30, 'a'),
        ("ghr_", 30, 'a'),
        ("github_pat_", 30, 'a'),
        ("AKIA", 16, 'A'),
        ("AIza", 30, 'a'),
    ];
    for (prefix, n, c) in cases {
        let hit = tok(prefix, n, c);
        expect_only(ids(&format!("# {hit}\n{DF}")), "secret-value", &hit);
        let near = tok(prefix, n - 1, c);
        assert_eq!(
            ids(&format!("# {near}\n{DF}")),
            Vec::<&str>::new(),
            "{near}"
        );
    }
}

#[test]
fn secret_value_scans_the_toml_and_comments_and_private_key_headers() {
    // 012 T006; "scans comments too"; `-----BEGIN ... PRIVATE KEY-----`
    let hit = tok("dckr_pat_", 24, 'A');
    expect_only(
        ids_toml(&format!("# {hit}\n{TOML}")),
        "secret-value",
        "toml comment",
    );
    let inline = toml_with(
        "timeout = 1800\n",
        &format!("timeout = 1800\nenv = {{ MODE = \"{hit}\" }}\n"),
    );
    expect_has(ids_toml(&inline), "secret-value", "toml env value");
    for head in [
        "-----BEGIN PRIVATE KEY-----",
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----",
    ] {
        expect_only(ids(&format!("# {head}\n{DF}")), "secret-value", head);
    }
    let public = format!("# -----BEGIN PUBLIC KEY-----\n{DF}");
    assert_eq!(
        ids(&public),
        Vec::<&str>::new(),
        "a public key is not a secret"
    );
}

#[test]
fn secret_value_matches_the_class_not_just_the_prefix() {
    // 012 T006: `dckr_pat_` takes [A-Za-z0-9_-]; `AKIA` takes [0-9A-Z]
    let dash = format!(
        "dckr_pat_{}-{}_{}",
        "A".repeat(7),
        "b".repeat(7),
        "9".repeat(6)
    );
    expect_has(
        ids(&format!("# {dash}\n{DF}")),
        "secret-value",
        "dash and underscore",
    );
    let lower = format!("AKIA{}", "a".repeat(16));
    assert_eq!(
        ids(&format!("# {lower}\n{DF}")),
        Vec::<&str>::new(),
        "AKIA needs upper case"
    );
    let gh = format!("ghp_{}", "a".repeat(15) + "-" + &"a".repeat(14));
    assert_eq!(
        ids(&format!("# {gh}\n{DF}")),
        Vec::<&str>::new(),
        "ghp_ takes alphanumerics only"
    );
}

// ---- secret-flag ----

#[test]
fn secret_flag_fails_for_secret_and_ssh_mounts() {
    // 012 T006; data-model.md `secret-flag`
    for line in [
        "RUN --mount=type=secret,id=x true",
        "RUN --mount=type=ssh true",
        "RUN --mount=type=bind,target=/x --mount=type=secret,id=y true",
    ] {
        expect_has(ids(&in_build(line)), "secret-flag", line);
    }
    let cache = in_build("RUN --mount=type=cache,target=/c true");
    assert_eq!(ids(&cache), Vec::<&str>::new(), "cache mount is fine");
}

#[test]
fn secret_flag_fails_for_toml_commands() {
    // 012 T006: `--secret`, `--ssh`, `--build-arg` in any command
    for flag in ["--secret id=x", "--ssh default", "--build-arg X=1"] {
        let t = toml_with("-t image-amd64 .", &format!("{flag} -t image-amd64 ."));
        expect_has(ids_toml(&t), "secret-flag", flag);
    }
}

// ---- pipeline-secrets, step-image ----

#[test]
fn pipeline_secrets_fails_for_a_step_with_secrets_or_a_broken_file() {
    // 012 T006; data-model.md `pipeline-secrets`
    let listed = toml_with(
        "timeout = 1800\n",
        "timeout = 1800\nsecrets = [\"DEPLOY_NAME\"]\n",
    );
    expect_has(ids_toml(&listed), "pipeline-secrets", "secrets listed");
    let second = format!(
        "{TOML}\n[steps.two]\nimage = \"docker:27.5.1-cli@sha256:{}\"\ncommands = [\"true\"]\nsecrets = [\"X_Y\"]\n",
        hex('c')
    );
    expect_has(
        ids_toml(&second),
        "pipeline-secrets",
        "secrets on a second step",
    );
    for bad in [
        "[steps.build\n",
        "not toml at all = = =\n",
        "[steps.build]\nbogus = 1\n",
        "\0",
    ] {
        expect_has(ids_toml(bad), "pipeline-secrets", &format!("{bad:?}"));
    }
    let ok = toml_with("timeout = 1800\n", "timeout = 1800\nsecrets = []\n");
    assert_eq!(
        ids_toml(&ok),
        Vec::<&str>::new(),
        "an empty list is no secret"
    );
}

#[test]
fn step_image_fails_for_unpinned_images_on_any_step() {
    // 012 T006; S1 `image = "alpine"`, `image = "alpine:3.23"`; FR-029
    let pinned = format!("image = \"docker:27.5.1-cli@sha256:{}\"", hex('c'));
    for r in [
        "image = \"alpine\"".to_string(),
        "image = \"alpine:3.23\"".to_string(),
        "image = \"docker:27.5.1-cli\"".to_string(),
        "image = \"docker:27.5.1-cli@sha256:abc\"".to_string(),
        format!("image = \"docker:27.5.1-cli@sha256:{}\"", &hex('c')[..63]),
        format!("image = \"docker:27.5.1-cli@sha256:{}c\"", hex('c')),
    ] {
        expect_only(ids_toml(&toml_with(&pinned, &r)), "step-image", &r);
    }
    // an empty image may also fail the pipeline parse; step-image must still be named or the parse id
    let empty = ids_toml(&toml_with(&pinned, "image = \"\""));
    assert!(
        empty.contains(&"step-image") || empty.contains(&"pipeline-secrets"),
        "{empty:?}"
    );
    let extra = format!("{TOML}\n[steps.extra]\nimage = \"alpine\"\ncommands = [\"true\"]\n");
    expect_has(ids_toml(&extra), "step-image", "a second, unpinned step");
}

// ---- review fixes: heredoc openers, quoted flags, platform overrides, split tokens, JSON form ----

#[test]
fn a_heredoc_marker_that_is_not_a_whole_word_is_refused() {
    // BuildKit opens a heredoc only for a whole word `\d*<<-?WORD`; anything else must not skip lines
    for line in [
        "RUN echo a<<#",
        "RUN echo \"<<X\"",
        "RUN [\"a\",\"<<X\"]",
        "RUN echo x<<<y",
        "COPY a<<X /b",
        "RUN echo \"a <<X\"",
        "RUN cat <<\"X",
    ] {
        expect_has(ids(&in_build(line)), "heredoc", line);
    }
    let fd = in_build("RUN cat 3<<EOF\nbody\nEOF");
    assert_eq!(ids(&fd), Vec::<&str>::new(), "fd-prefixed opener");
}

#[test]
fn a_hidden_add_after_a_fake_heredoc_is_still_seen() {
    // 012 review MAJOR 1: `a<<#` must not skip the ADD that follows
    let got = ids(&in_build("RUN echo a<<#\nADD https://evil.test/x /y\n#"));
    expect_has(got.clone(), "heredoc", "fake opener");
    expect_has(got, "instruction", "ADD after the fake opener");
}

#[test]
fn quoted_csv_fields_in_mount_and_from_are_refused() {
    // 012 review MAJOR 2
    for line in [
        "RUN --mount=type=bind,\"from=alpine\",target=/x true",
        "RUN --mount=type=bind,\"type=secret\",target=/x true",
        "RUN --mount=type=bind,'from=alpine',target=/x true",
        "COPY --from=\"alpine\" /a /a",
        "COPY --from='build' /a /a",
    ] {
        let got = ids(&in_build(line));
        expect_has(got.clone(), "external-source", line);
        expect_has(got, "secret-flag", line);
    }
}

#[test]
fn assigning_a_platform_variable_is_emulation() {
    // 012 review MINOR 2
    for var in [
        "BUILDPLATFORM",
        "TARGETPLATFORM",
        "TARGETARCH",
        "TARGETOS",
        "BUILDARCH",
    ] {
        for line in [
            format!("ARG {var}=arm64"),
            format!("ENV {var}=arm64"),
            format!("ENV {var} arm64"),
        ] {
            expect_has(ids(&in_build(&line)), "emulation", &line);
        }
    }
    assert_eq!(ids(&in_build("ARG TARGETOS")), Vec::<&str>::new());
}

#[test]
fn a_token_split_across_a_continuation_is_found() {
    // 012 review MINOR 3: neither half reaches 20, the joined text does
    let df = in_build("RUN echo dckr_pat_AAAAAAAAAA\\\nAAAAAAAAAA");
    expect_has(ids(&df), "secret-value", "split token");
}

#[test]
fn json_form_must_be_a_string_array() {
    // 012 review MINOR 4: anything else is shell form
    for bad in [
        "ENTRYPOINT [\"/hello\"",
        "ENTRYPOINT [/hello]",
        "ENTRYPOINT [1]",
        "ENTRYPOINT [\"/hello\"] x",
    ] {
        let df = df_with("ENTRYPOINT [\"/hello\"]", bad);
        expect_has(ids(&df), "final-shell", bad);
    }
    let ok = df_with(
        "ENTRYPOINT [\"/hello\"]",
        "ENTRYPOINT [ \"/hello\", \"-v\" ]",
    );
    assert_eq!(ids(&ok), Vec::<&str>::new());
}

// ---- follow-up: scan cost, FIFO, RUN network and security flags ----

#[test]
fn repeated_prefixes_scan_in_linear_time() {
    // just under 256 KiB of `AKIA`: every 4th byte is a prefix and the class run is the whole line
    let start = std::time::Instant::now();
    let df = format!("# {}\n{DF}", "AKIA".repeat(60 * 1024));
    expect_has(ids(&df), "secret-value", "repeated prefix");
    assert!(start.elapsed().as_secs() < 10, "{:?}", start.elapsed());
}

#[test]
fn a_fifo_named_dockerfile_is_layout_and_does_not_hang() {
    // 012 review MINOR 1: skip when `mkfifo` is missing
    let t = project(DF.as_bytes(), TOML);
    let p = t.0.join("Dockerfile");
    std::fs::remove_file(&p).unwrap();
    match std::process::Command::new("mkfifo").arg(&p).status() {
        Ok(s) if s.success() => expect_has(run(&t), "layout", "fifo"),
        _ => eprintln!("skip: mkfifo not available"),
    }
}

#[test]
fn run_network_and_security_flags_are_refused() {
    for flag in [
        "--network=host",
        "--network=none",
        "--security=insecure",
        "--network host",
    ] {
        expect_has(
            ids(&in_build(&format!("RUN {flag} true"))),
            "instruction",
            flag,
        );
    }
}

// ---- review and security fixes (S6b): the step template and the folder walk ----

fn with_files(files: &[(&str, &str)]) -> Vec<&'static str> {
    let t = project(DF.as_bytes(), TOML);
    for (name, text) in files {
        let p = t.0.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    run(&t)
}

#[test]
fn step_template_fails_for_any_other_step_command_or_flag() {
    // 012 T038, security S2: one step `build`, `docker = true`, the contract commands
    let first = "docker build --platform linux/amd64 -t image-amd64 .";
    let save = "docker save -o image-amd64.tar image-amd64";
    let chmod = "chmod 0644 image-amd64.tar image-arm64.tar";
    let extra = format!(
        "{TOML}\n[steps.extra]\nimage = \"docker:27.5.1-cli@sha256:{}\"\ncommands = [\"true\"]\n",
        hex('c')
    );
    let cases = [
        ("changed", toml_with(first, "docker build -t other .")),
        ("added", toml_with(chmod, &format!("{chmod}\",\n  \"id"))),
        ("docker off", toml_with("docker = true\n", "")),
        ("another step", extra),
        ("renamed", TOML.replace("[steps.build]", "[steps.other]")),
        (
            "reordered",
            toml_with(
                &format!("  \"{first}\",\n  \"{save}\",\n"),
                &format!("  \"{save}\",\n  \"{first}\",\n"),
            ),
        ),
    ];
    for (case, t) in cases {
        expect_has(ids_toml(&t), "step-template", case);
    }
    assert_eq!(ids_toml(TOML), Vec::<&str>::new());
}

#[test]
fn credential_file_names_fail() {
    // 012 T039, security S3
    for name in [".env", "id_rsa", "id_ed25519", "server.pem", "deploy.key", "Sub/Dir/TLS.PEM"] {
        expect_only(with_files(&[(name, "x\n")]), "secret-file", name);
    }
    assert_eq!(with_files(&[("hello.go", "package main\n"), ("keys.txt", "x\n")]), Vec::<&str>::new());
}

#[test]
fn credentials_in_any_file_fail() {
    // 012 T039, security S3: new prefixes, and files other than the Dockerfile and the pipeline
    let jwt = format!("eyJ{}.{}.{}", "a".repeat(20), "b".repeat(20), "c".repeat(20));
    let hits = [
        format!("sk_live_{}", "A".repeat(24)),
        format!("xoxb-{}", "1".repeat(12)),
        format!("npm_{}", "a".repeat(36)),
        format!("glpat-{}", "a".repeat(20)),
        format!("dckr_pat_{}", "A".repeat(24)),
        jwt,
    ];
    for hit in hits {
        let text = format!("package main\n// {hit}\n");
        expect_only(with_files(&[("src/hello.go", text.as_str())]), "secret-value", &hit);
    }
    for fine in ["eyJ", "eyJabc.def.ghi", "xoxb-", "npm_short", "sk_live_short"] {
        assert_eq!(with_files(&[("a.txt", fine)]), Vec::<&str>::new(), "{fine}");
    }
}

#[test]
fn the_folder_walk_refuses_links_and_oversized_folders() {
    // 012 T039, security S3: non-regular entries and the caps are `layout`
    let t = project(DF.as_bytes(), TOML);
    std::os::unix::fs::symlink("/etc/passwd", t.0.join("link")).unwrap();
    expect_only(run(&t), "layout", "a symlink");

    let t = project(DF.as_bytes(), TOML);
    for i in 0..500 {
        std::fs::write(t.0.join(format!("f{i}")), "x").unwrap();
    }
    expect_only(run(&t), "layout", "503 entries");

    let t = project(DF.as_bytes(), TOML);
    std::fs::write(t.0.join("big.bin"), vec![b'a'; (1 << 20) + 1]).unwrap();
    expect_only(run(&t), "layout", "a file over 1 MiB");
    std::fs::write(t.0.join("big.bin"), vec![b'a'; 1 << 20]).unwrap();
    assert_eq!(run(&t), Vec::<&str>::new(), "exactly 1 MiB passes");

    let t = project(DF.as_bytes(), TOML);
    let mut dir = t.0.clone();
    for _ in 0..10 {
        dir = dir.join("d");
    }
    std::fs::create_dir_all(&dir).unwrap();
    expect_only(run(&t), "layout", "ten levels deep");
}

#[test]
fn the_two_build_archives_are_not_walked() {
    // 012 T039: `publish` checks the folder again after the build; the archives are big and hold layers
    let tar = format!("sk_live_{}", "A".repeat(24));
    let ids = with_files(&[("image-amd64.tar", tar.as_str()), ("image-arm64.tar", tar.as_str())]);
    assert_eq!(ids, Vec::<&str>::new());
}
