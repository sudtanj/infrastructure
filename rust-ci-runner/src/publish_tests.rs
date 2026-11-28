//! 012 T015 (test-first, plan step S6) and the source rules of S7. Targets the pure `publish::verify_index(&str)
//! -> Result<(), &'static str>` (contracts/publish-step.md 5, data-model.md "Tags and the manifest list"; I10),
//! the constants the publish step needs, and a few source-level rules that no behaviour test can see.
//!
//! The flows of S6 (token, login name, archives, tips, teardown, Hub order, containers, reuse, exit codes,
//! deadline) are in `tests/job_publish.rs`: the plan does not pin the signature of `publish::publish`, so they run
//! through `job`, where the real arguments exist.
//!
//! Not yet registered: with `pub mod publish;` in `lib.rs`, add `#[cfg(test)] mod publish_tests;` beside it, then
//! remove each `#[ignore = "test-first"]` as its test passes (T016, T017). Run this module as its own cargo command
//! (`cargo test --locked --lib publish_tests::`) as the plan says (MINOR-1); nothing here starts a thread or a
//! server, so the split is for the integration flows that do.

use crate::publish::verify_index;
use std::time::Duration;

const OCI: &str = "application/vnd.oci.image.index.v1+json";
const DOCKER: &str = "application/vnd.docker.distribution.manifest.list.v2+json";
const AMD: (&str, &str) = ("linux", "amd64");
const ARM: (&str, &str) = ("linux", "arm64");
const ATTEST: (&str, &str) = ("unknown", "unknown");

fn digest(c: char) -> String {
    format!("sha256:{}", c.to_string().repeat(64))
}

/// One manifest entry. `platform` None leaves the field out; `digest` is written as given.
fn entry(platform: Option<(&str, &str)>, digest: &str) -> String {
    let p = match platform {
        Some((os, arch)) => {
            format!(",\"platform\":{{\"architecture\":\"{arch}\",\"os\":\"{os}\"}}")
        }
        None => String::new(),
    };
    format!(
        "{{\"mediaType\":\"application/vnd.oci.image.manifest.v1+json\",\"digest\":\"{digest}\",\"size\":123{p}}}"
    )
}

fn index(media: &str, entries: &[String]) -> String {
    format!(
        "{{\"schemaVersion\":2,\"mediaType\":\"{media}\",\"manifests\":[{}]}}",
        entries.join(",")
    )
}

/// An index with one entry per platform, each with its own digest.
fn plats(media: &str, plats: &[(&str, &str)]) -> String {
    let es: Vec<String> = plats
        .iter()
        .enumerate()
        .map(|(i, p)| entry(Some(*p), &digest((b'a' + i as u8) as char)))
        .collect();
    index(media, &es)
}

fn good() -> String {
    plats(OCI, &[AMD, ARM])
}

#[test]
#[ignore = "test-first"]
fn both_platforms_pass_in_both_media_types_and_either_order() {
    // 012 T015; data-model.md "Manifest list.mediaType, .manifests"
    for media in [OCI, DOCKER] {
        for order in [[AMD, ARM], [ARM, AMD]] {
            let j = plats(media, &order);
            assert!(verify_index(&j).is_ok(), "{j}");
        }
    }
}

#[test]
#[ignore = "test-first"]
fn extra_fields_in_an_entry_or_the_index_do_not_matter() {
    // 012 T015: only media type, the two platforms and the digests are judged
    let e1 = "{\"mediaType\":\"x\",\"digest\":\"DIGEST_A\",\"size\":1,\"annotations\":{\"a\":\"b\"},\"platform\":{\"architecture\":\"amd64\",\"os\":\"linux\",\"os.version\":\"1\"}}"
        .replace("DIGEST_A", &digest('a'));
    let e2 = entry(Some(ARM), &digest('b'));
    let j = format!(
        "{{\"schemaVersion\":2,\"mediaType\":\"{OCI}\",\"annotations\":{{\"k\":\"v\"}},\"manifests\":[{e1},{e2}]}}"
    );
    assert!(verify_index(&j).is_ok(), "{j}");
}

#[test]
#[ignore = "test-first"]
fn the_wrong_number_or_mix_of_platforms_fails() {
    // 012 T015; contracts/publish-step.md 5: one entry, three, two `amd64`, no `arm64`, an attestation entry
    let cases: Vec<(&str, String)> = vec![
        ("one amd64", plats(OCI, &[AMD])),
        ("one arm64", plats(OCI, &[ARM])),
        ("no entries", index(OCI, &[])),
        ("three", plats(OCI, &[AMD, ARM, ("linux", "arm")])),
        ("two amd64", plats(OCI, &[AMD, AMD])),
        ("two arm64", plats(OCI, &[ARM, ARM])),
        ("no arm64", plats(OCI, &[AMD, ("linux", "ppc64le")])),
        ("attestation beside both", plats(OCI, &[AMD, ARM, ATTEST])),
        ("only attestations", plats(OCI, &[ATTEST, ATTEST])),
        ("amd64 and an attestation", plats(OCI, &[AMD, ATTEST])),
        ("windows amd64", plats(OCI, &[("windows", "amd64"), ARM])),
        (
            "os that is not linux",
            plats(DOCKER, &[AMD, ("darwin", "arm64")]),
        ),
    ];
    for (label, j) in cases {
        assert!(verify_index(&j).is_err(), "{label}: {j}");
    }
}

#[test]
#[ignore = "test-first"]
fn a_malformed_digest_fails() {
    // 012 T015; data-model.md "Manifest list.digest": `sha256:` plus 64 lowercase hex
    let upper = format!("sha256:{}", "A".repeat(64));
    let short = format!("sha256:{}", "a".repeat(63));
    let long = format!("sha256:{}", "a".repeat(65));
    let sha512 = format!("sha512:{}", "a".repeat(64));
    let bare = "a".repeat(64);
    let nonhex = format!("sha256:{}g", "a".repeat(63));
    let spaced = format!("sha256:{} ", "a".repeat(63));
    for bad in [
        upper.as_str(),
        short.as_str(),
        long.as_str(),
        sha512.as_str(),
        bare.as_str(),
        nonhex.as_str(),
        spaced.as_str(),
        "",
        "sha256:",
    ] {
        for slot in [0, 1] {
            let mut es = vec![
                entry(Some(AMD), &digest('a')),
                entry(Some(ARM), &digest('b')),
            ];
            es[slot] = entry(Some(if slot == 0 { AMD } else { ARM }), bad);
            let j = index(OCI, &es);
            assert!(verify_index(&j).is_err(), "slot {slot}: {bad:?}");
        }
    }
    // a digest that is missing, and one that is not a string
    let no_digest = "{\"mediaType\":\"m\",\"size\":1,\"platform\":{\"architecture\":\"amd64\",\"os\":\"linux\"}}";
    let num_digest = "{\"mediaType\":\"m\",\"digest\":7,\"size\":1,\"platform\":{\"architecture\":\"amd64\",\"os\":\"linux\"}}";
    for e in [no_digest, num_digest] {
        let j = index(OCI, &[e.to_string(), entry(Some(ARM), &digest('b'))]);
        assert!(verify_index(&j).is_err(), "{e}");
    }
}

#[test]
#[ignore = "test-first"]
fn a_missing_or_partial_platform_fails() {
    // 012 T015: missing `platform`, missing `os`, missing `architecture`
    let no_os = "{\"mediaType\":\"m\",\"digest\":\"DIGEST\",\"size\":1,\"platform\":{\"architecture\":\"amd64\"}}"
        .replace("DIGEST", &digest('a'));
    let no_arch =
        "{\"mediaType\":\"m\",\"digest\":\"DIGEST\",\"size\":1,\"platform\":{\"os\":\"linux\"}}"
            .replace("DIGEST", &digest('a'));
    let partial = [entry(None, &digest('a')), no_os, no_arch];
    for e in partial {
        let j = index(OCI, &[e.clone(), entry(Some(ARM), &digest('b'))]);
        assert!(verify_index(&j).is_err(), "{e}");
    }
}

#[test]
#[ignore = "test-first"]
fn a_single_manifest_or_another_media_type_fails() {
    // 012 T015: a single image manifest (the `latest` tag must never point at one), no `manifests`, odd types
    let manifest = "{\"schemaVersion\":2,\"mediaType\":\"application/vnd.oci.image.manifest.v1+json\",\"config\":{\"digest\":\"DIGEST\"},\"layers\":[]}"
        .replace("DIGEST", &digest('a'));
    let docker_manifest = "{\"schemaVersion\":2,\"mediaType\":\"application/vnd.docker.distribution.manifest.v2+json\",\"config\":{},\"layers\":[]}";
    let both = vec![
        entry(Some(AMD), &digest('a')),
        entry(Some(ARM), &digest('b')),
    ];
    let cases = [
        manifest,
        docker_manifest.to_string(),
        format!("{{\"schemaVersion\":2,\"mediaType\":\"{OCI}\"}}"),
        index("application/json", &both),
        index("", &both),
        index("application/vnd.oci.image.manifest.v1+json", &both),
        format!(
            "{{\"schemaVersion\":2,\"manifests\":[{},{}]}}",
            both[0], both[1]
        ),
    ];
    for j in cases {
        assert!(verify_index(&j).is_err(), "{j}");
    }
}

#[test]
#[ignore = "test-first"]
fn not_json_and_wrong_shapes_fail_without_a_panic() {
    // 012 T015; I2-style: any text, no panic
    let deep = "[".repeat(2000);
    let big = "a".repeat(1 << 20);
    let cases = [
        "",
        " ",
        "not json",
        "{",
        "}",
        "null",
        "true",
        "42",
        "\"x\"",
        "[]",
        "{}",
        "{\"manifests\":5}",
        "{\"manifests\":\"x\"}",
        "{\"manifests\":null}",
        "{\"manifests\":[null]}",
        "{\"manifests\":[1,2]}",
        "{\"manifests\":[{},{}]}",
        "{\"manifests\":[[],[]]}",
        "{\"mediaType\":5,\"manifests\":[]}",
        "{\"mediaType\":[],\"manifests\":{}}",
        "\u{0}",
        "{\"manifests\":[{\"platform\":5,\"digest\":5}]}",
        "{\"manifests\":[{\"platform\":{\"os\":5,\"architecture\":6},\"digest\":\"x\"}]}",
        deep.as_str(),
        big.as_str(),
        "\u{feff}{}",
        "{\"a\":\"\u{1f600}\"}",
    ];
    for j in cases {
        let shown: String = j.chars().take(40).collect();
        assert!(verify_index(j).is_err(), "{shown:?}");
    }
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, m: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % m
    }
}

#[test]
#[ignore = "test-first"]
fn verify_index_accepts_exactly_one_amd64_and_one_arm64() {
    // 012 T015; I10 over generated lists: every permutation, drop, duplicate, add and relabel
    let pool = [
        AMD,
        ARM,
        ("linux", "arm"),
        ATTEST,
        ("windows", "amd64"),
        ("linux", "s390x"),
    ];
    let medias = [OCI, DOCKER, "application/json"];
    let mut rng = Lcg(5);
    let (mut accepted, mut refused) = (0, 0);
    for _ in 0..2000 {
        let n = rng.next(5) as usize;
        let list: Vec<(&str, &str)> = (0..n)
            .map(|_| {
                let i = if rng.next(3) < 2 {
                    rng.next(2)
                } else {
                    rng.next(6)
                };
                pool[i as usize]
            })
            .collect();
        let media = medias[rng.next(3) as usize];
        let amd = list.iter().filter(|p| **p == AMD).count();
        let arm = list.iter().filter(|p| **p == ARM).count();
        let want = media != "application/json" && n == 2 && amd == 1 && arm == 1;
        let j = plats(media, &list);
        assert_eq!(verify_index(&j).is_ok(), want, "{media} {list:?}");
        if want {
            accepted += 1;
        } else {
            refused += 1;
        }
    }
    assert!(
        accepted > 20 && refused > 10,
        "the generator is lopsided: {accepted}/{refused}"
    );
}

#[test]
#[ignore = "test-first"]
fn every_single_mutation_of_a_good_index_fails() {
    // 012 T015; I10: the good list passes and each mutation of it does not
    let a = entry(Some(AMD), &digest('a'));
    let b = entry(Some(ARM), &digest('b'));
    assert!(verify_index(&good()).is_ok());
    let attest = entry(Some(ATTEST), &digest('c'));
    let mutations: Vec<(&str, String)> = vec![
        ("drop amd64", index(OCI, &[b.clone()])),
        ("drop arm64", index(OCI, &[a.clone()])),
        (
            "duplicate amd64",
            index(OCI, &[a.clone(), a.clone(), b.clone()]),
        ),
        (
            "duplicate arm64",
            index(OCI, &[a.clone(), b.clone(), b.clone()]),
        ),
        (
            "add attestation",
            index(OCI, &[a.clone(), b.clone(), attest]),
        ),
        (
            "relabel arm64",
            index(OCI, &[a.clone(), entry(Some(AMD), &digest('b'))]),
        ),
        (
            "relabel amd64",
            index(OCI, &[entry(Some(ARM), &digest('a')), b.clone()]),
        ),
        (
            "drop platform",
            index(OCI, &[entry(None, &digest('a')), b.clone()]),
        ),
        (
            "upper-case digest",
            good().replace(&digest('a'), &format!("sha256:{}", "A".repeat(64))),
        ),
        ("truncated text", good()[..good().len() - 3].to_string()),
        ("media type removed", good().replace(OCI, "")),
    ];
    for (label, j) in mutations {
        assert!(verify_index(&j).is_err(), "{label}: {j}");
    }
}

// ---- constants the publish step needs (data-model.md "Constants") ----

#[test]
#[ignore = "test-first"]
fn the_job_deadline_leaves_five_minutes_of_the_job_limit() {
    // 012 T016; FR-033: 40 minutes of the 45-minute job limit
    assert_eq!(crate::image::JOB_DEADLINE, Duration::from_secs(40 * 60));
}

#[test]
#[ignore = "test-first"]
fn crane_paths_are_absolute_and_safe_to_pass_as_one_argument() {
    // 012 T016; MAJOR-4: `--entrypoint <CRANE_SH>` and `-e CRANE=<CRANE_BIN>`
    for p in [crate::image::CRANE_SH, crate::image::CRANE_BIN] {
        assert!(p.starts_with('/') && p.len() > 1, "{p}");
        assert!(
            !p.chars()
                .any(|c| c.is_whitespace() || "'\"`$\\;&|<>()=:".contains(c)),
            "{p} would need quoting"
        );
    }
    assert_ne!(crate::image::CRANE_SH, crate::image::CRANE_BIN);
}

// ---- source rules ----

const PUBLISH_SRC: &str = include_str!("publish.rs");
const JOB_SRC: &str = include_str!("job.rs");

#[test]
#[ignore = "test-first"]
fn publish_starts_containers_only_through_the_engine() {
    // 012 T016; S6 constraints: `capture_in` for both containers, no `Command::new`, no token on a command line
    assert!(
        !PUBLISH_SRC.contains("Command::new"),
        "publish.rs spawns a process itself"
    );
    assert!(
        PUBLISH_SRC.contains("capture_in"),
        "publish.rs does not use Engine::capture_in"
    );
    for banned in [
        "DOCKERHUB_TOKEN=",
        "-e DOCKERHUB",
        "\"-e\", \"DOCKERHUB",
        "--password ",
    ] {
        assert!(
            !PUBLISH_SRC.contains(banned),
            "publish.rs contains {banned:?}"
        );
    }
    assert!(
        PUBLISH_SRC.contains("--entrypoint"),
        "the publish container names its entrypoint"
    );
}

#[test]
#[ignore = "test-first"]
fn the_hub_override_is_read_only_in_debug_builds() {
    // 012 T017; MINOR-2: every mention of `FACTORY_CI_HUB_URL` in job.rs follows a `cfg(debug_assertions)`
    let name = "FACTORY_CI_HUB_URL";
    let hits: Vec<usize> = JOB_SRC.match_indices(name).map(|(i, _)| i).collect();
    assert!(
        !hits.is_empty(),
        "job.rs never reads {name}: the binary tests cannot reach a fake Hub"
    );
    for i in hits {
        let from = JOB_SRC[..i]
            .char_indices()
            .rev()
            .nth(400)
            .map_or(0, |(k, _)| k);
        let before = &JOB_SRC[from..i];
        assert!(
            before.contains("cfg(debug_assertions)") || before.contains("cfg!(debug_assertions)"),
            "{name} at byte {i} is not under debug_assertions"
        );
    }
    for (file, src) in [
        ("publish.rs", PUBLISH_SRC),
        ("hub.rs", include_str!("hub.rs")),
        ("main.rs", include_str!("main.rs")),
    ] {
        assert!(!src.contains(name), "{file} reads {name}");
    }
}
