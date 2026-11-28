//! 012 T010 (plan step S5). Targets `archive::read_config(&Path) -> Result<ImageConfig, &'static str>`
//! on a `docker save` tar (data-model.md "Archive config"). Tars are built in memory, then written to a file named
//! `image-amd64.tar` so the platform the name implies is the one the config says.
//!
//! `ImageConfig` has `os`, `architecture` and `user` (String), `entrypoint` and `cmd` (Vec<String>).
//!
//! `read_config` judges nothing. The policy errors of data-model.md (`image runs as root`,
//! `image has a shell entrypoint`, `image platform mismatch`) are applied by `publish` and tested through the
//! job in `tests/job_publish.rs` (QA-1). Every config below is policy-clean.

use crate::archive::read_config;
use std::path::PathBuf;

const BAD: &str = "archive unreadable";
const HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct Tmp(PathBuf);

impl Tmp {
    fn new() -> Tmp {
        let p = std::env::temp_dir().join(format!("fci-archive-{}", crate::util::random_id()));
        std::fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }

    fn file(&self, bytes: &[u8]) -> PathBuf {
        let p = self.0.join("image-amd64.tar");
        std::fs::write(&p, bytes).unwrap();
        p
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn octal(buf: &mut [u8], v: u64) {
    let n = buf.len();
    let s = format!("{v:0w$o}", w = n - 1);
    buf[..n - 1].copy_from_slice(s.as_bytes());
    buf[n - 1] = 0;
}

fn header(name: &[u8], size: u64, kind: u8) -> [u8; 512] {
    let mut h = [0u8; 512];
    h[..name.len()].copy_from_slice(name);
    octal(&mut h[100..108], 0o644);
    octal(&mut h[108..116], 0);
    octal(&mut h[116..124], 0);
    octal(&mut h[124..136], size);
    octal(&mut h[136..148], 0);
    h[156] = kind;
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    h[148..156].copy_from_slice(b"        ");
    let sum: u32 = h.iter().map(|b| *b as u32).sum();
    let s = format!("{sum:06o}\0 ");
    h[148..156].copy_from_slice(s.as_bytes());
    h
}

/// One tar member: header, data, padding to 512.
fn member(name: &str, data: &[u8]) -> Vec<u8> {
    let mut out = header(name.as_bytes(), data.len() as u64, b'0').to_vec();
    out.extend_from_slice(data);
    out.resize(out.len().div_ceil(512) * 512, 0);
    out
}

fn typed(name: &str, kind: u8, data: &[u8]) -> Vec<u8> {
    let mut out = header(name.as_bytes(), data.len() as u64, kind).to_vec();
    out.extend_from_slice(data);
    out.resize(out.len().div_ceil(512) * 512, 0);
    out
}

fn end(mut v: Vec<u8>) -> Vec<u8> {
    v.extend_from_slice(&[0u8; 1024]);
    v
}

fn config_json(user: &str, entry: &str, cmd: &str) -> String {
    format!(
        "{{\"architecture\":\"amd64\",\"os\":\"linux\",\"config\":{{\"User\":\"{user}\",\"Entrypoint\":{entry},\"Cmd\":{cmd}}},\"rootfs\":{{\"type\":\"layers\",\"diff_ids\":[]}}}}"
    )
}

fn manifest_json(config_path: &str) -> String {
    format!("[{{\"Config\":\"{config_path}\",\"RepoTags\":[\"image-amd64:latest\"],\"Layers\":[\"layer.tar\"]}}]")
}

fn blob_path() -> String {
    format!("blobs/sha256/{HEX}")
}

fn good_config() -> String {
    config_json("65532:65532", "[\"/hello\"]", "null")
}

/// A well-formed `docker save` tar: layer, config, manifest.
fn good_tar() -> Vec<u8> {
    let mut v = member("layer.tar", &[7u8; 1500]);
    v.extend(member(&blob_path(), good_config().as_bytes()));
    v.extend(member(
        "manifest.json",
        manifest_json(&blob_path()).as_bytes(),
    ));
    end(v)
}

fn read(bytes: &[u8]) -> Result<crate::archive::ImageConfig, &'static str> {
    let t = Tmp::new();
    read_config(&t.file(bytes))
}

#[test]
fn reads_the_blobs_form() {
    // 012 T010; S5 `blobs/sha256/<hex>`; Archive config.os, .architecture, .user, .entrypoint
    let c = read(&good_tar()).unwrap();
    assert_eq!(c.os, "linux");
    assert_eq!(c.architecture, "amd64");
    assert_eq!(c.user, "65532:65532");
    assert_eq!(c.entrypoint, ["/hello"]);
    assert!(c.cmd.is_empty(), "Cmd null reads as empty");
}

#[test]
fn reads_the_hex_dot_json_form() {
    // 012 T010; S5 `<hex>.json`
    let path = format!("{HEX}.json");
    let mut v = member("layer.tar", b"x");
    v.extend(member(&path, good_config().as_bytes()));
    v.extend(member("manifest.json", manifest_json(&path).as_bytes()));
    let c = read(&end(v)).unwrap();
    assert_eq!((c.os.as_str(), c.user.as_str()), ("linux", "65532:65532"));
    assert_eq!(c.entrypoint, ["/hello"]);
}

#[test]
fn returns_entrypoint_and_cmd_as_given() {
    // 012 T010; Archive config.entrypoint
    let cfg = config_json("1000", "[\"/app\",\"--serve\"]", "[\"--port\",\"80\"]");
    let mut v = member(&blob_path(), cfg.as_bytes());
    v.extend(member(
        "manifest.json",
        manifest_json(&blob_path()).as_bytes(),
    ));
    let c = read(&end(v)).unwrap();
    assert_eq!(c.entrypoint, ["/app", "--serve"]);
    assert_eq!(c.cmd, ["--port", "80"]);
    assert_eq!(c.user, "1000");
}

#[test]
fn null_and_empty_entrypoint_and_cmd_are_allowed() {
    // 012 T010; "empty both is allowed"
    for (e, c) in [("null", "null"), ("[]", "[]")] {
        let cfg = config_json("65532", e, c);
        let mut v = member(&blob_path(), cfg.as_bytes());
        v.extend(member(
            "manifest.json",
            manifest_json(&blob_path()).as_bytes(),
        ));
        let got = read(&end(v)).unwrap();
        assert!(got.entrypoint.is_empty() && got.cmd.is_empty(), "{e} {c}");
    }
}

#[test]
fn manifest_may_come_after_or_before_the_config() {
    // 012 T010: "reads manifest.json then the config member it names" whatever the member order
    let mut v = member("manifest.json", manifest_json(&blob_path()).as_bytes());
    v.extend(member("layer.tar", &[1u8; 3000]));
    v.extend(member(&blob_path(), good_config().as_bytes()));
    assert_eq!(read(&end(v)).unwrap().user, "65532:65532");
}

#[test]
fn a_large_layer_member_is_seeked_past() {
    // 012 T010; S5 "seeking past data", "no allocation proportional to the archive": a 3 MB layer is no error and
    // the 1 MB member cap does not apply to members the reader skips
    let mut v = member("layer.tar", &vec![9u8; 3 << 20]);
    v.extend(member(&blob_path(), good_config().as_bytes()));
    v.extend(member(
        "manifest.json",
        manifest_json(&blob_path()).as_bytes(),
    ));
    assert!(read(&end(v)).is_ok());
}

#[test]
fn gnu_long_name_and_pax_headers_are_skipped() {
    // 012 T010; S5 "GNU long-name and PAX headers skipped"
    let long = format!("blobs/sha256/{}/{}", "d".repeat(80), "layer.tar");
    let mut v = typed("././@LongLink", b'L', format!("{long}\0").as_bytes());
    v.extend(member(&long[..99], &[3u8; 700]));
    let pax = "32 path=ignored/ignored/ignored\n".to_string();
    v.extend(typed("PaxHeader/x", b'x', pax.as_bytes()));
    v.extend(member("pax-target", b"zzz"));
    v.extend(typed("pax-global", b'g', b"20 comment=hi there\n"));
    v.extend(member(&blob_path(), good_config().as_bytes()));
    v.extend(member(
        "manifest.json",
        manifest_json(&blob_path()).as_bytes(),
    ));
    assert_eq!(read(&end(v)).unwrap().user, "65532:65532");
}

#[test]
fn a_directory_entry_before_the_members_is_fine() {
    // 012 T010: `docker save` lists `blobs/` and `blobs/sha256/` as directories (typeflag 5, size 0)
    let mut v = typed("blobs/", b'5', b"");
    v.extend(typed("blobs/sha256/", b'5', b""));
    v.extend(member(&blob_path(), good_config().as_bytes()));
    v.extend(member(
        "manifest.json",
        manifest_json(&blob_path()).as_bytes(),
    ));
    assert!(read(&end(v)).is_ok());
}

// ---- failure paths: every one is a fixed error, never a panic ----

#[test]
fn an_unreadable_path_is_a_fixed_error() {
    // 012 T010
    let t = Tmp::new();
    assert_eq!(
        read_config(&t.0.join("nope.tar")).unwrap_err(),
        BAD,
        "missing file"
    );
    assert_eq!(read_config(&t.0).unwrap_err(), BAD, "a directory");
}

#[test]
fn empty_garbage_and_zero_files_are_unreadable() {
    // 012 T010; "a truncated or garbage file"
    let mut seed: u64 = 42;
    let junk: Vec<u8> = (0..8192)
        .map(|_| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            (seed >> 33) as u8
        })
        .collect();
    for (name, bytes) in [
        ("empty", vec![]),
        ("one block of zeros", vec![0u8; 512]),
        ("end blocks only", vec![0u8; 1024]),
        ("short", b"not a tar".to_vec()),
        ("junk", junk),
        ("text", "FROM scratch\n".repeat(200).into_bytes()),
    ] {
        assert_eq!(read(&bytes).unwrap_err(), BAD, "{name}");
    }
}

#[test]
fn every_truncation_inside_the_needed_members_is_unreadable_and_none_panics() {
    // 012 T010: cut the good tar at each length; before the manifest and config are whole it must fail
    let tar = good_tar();
    let config_end = 512 + 1536 + 512 + good_config().len().div_ceil(512) * 512;
    let manifest_data_end = config_end + 512 + manifest_json(&blob_path()).len();
    let t = Tmp::new();
    let mut cut = 0;
    while cut < tar.len() {
        let r = read_config(&t.file(&tar[..cut]));
        if cut < manifest_data_end {
            assert_eq!(r.unwrap_err(), BAD, "cut at {cut}");
        } else if let Err(e) = r {
            assert_eq!(e, BAD, "cut at {cut}");
        }
        cut += 97;
    }
    assert!(read_config(&t.file(&tar)).is_ok());
}

#[test]
fn a_header_with_a_huge_or_garbage_size_is_unreadable() {
    // 012 T010: size 0o77777777777 (8 GiB - 1) on a member of a tiny file; and non-octal size bytes
    let mut v = header(b"layer.tar", 0o77777777777, b'0').to_vec();
    v.extend_from_slice(&[0u8; 1024]);
    assert_eq!(read(&v).unwrap_err(), BAD, "huge size");
    let mut h = header(b"layer.tar", 10, b'0');
    h[124..135].copy_from_slice(b"zzzzzzzzzzz");
    let mut v = h.to_vec();
    v.extend_from_slice(&[0u8; 1024]);
    assert_eq!(read(&v).unwrap_err(), BAD, "non-octal size");
}

#[test]
fn a_missing_manifest_or_config_member_is_unreadable() {
    // 012 T010; "a missing member"
    let mut no_manifest = member("layer.tar", b"x");
    no_manifest.extend(member(&blob_path(), good_config().as_bytes()));
    assert_eq!(
        read(&end(no_manifest)).unwrap_err(),
        BAD,
        "no manifest.json"
    );
    let mut no_config = member("layer.tar", b"x");
    no_config.extend(member(
        "manifest.json",
        manifest_json(&blob_path()).as_bytes(),
    ));
    assert_eq!(
        read(&end(no_config)).unwrap_err(),
        BAD,
        "manifest names a path not in the tar"
    );
}

#[test]
fn a_manifest_naming_a_path_that_is_not_in_the_tar_is_unreadable() {
    // 012 T010; the tar holds a config, but not the one the manifest names
    let mut v = member(&blob_path(), good_config().as_bytes());
    let other = format!("blobs/sha256/{}", "f".repeat(64));
    v.extend(member("manifest.json", manifest_json(&other).as_bytes()));
    assert_eq!(read(&end(v)).unwrap_err(), BAD);
}

#[test]
fn a_manifest_naming_a_path_outside_the_tar_layout_is_unreadable() {
    // 012 T010: `../x`, an absolute path and an empty name never resolve to a member
    for p in ["../manifest.json", "/etc/passwd", ""] {
        let mut v = member("layer.tar", good_config().as_bytes());
        v.extend(member("manifest.json", manifest_json(p).as_bytes()));
        assert_eq!(read(&end(v)).unwrap_err(), BAD, "{p:?}");
    }
}

#[test]
fn the_manifest_must_hold_exactly_one_image_entry() {
    // 012 T010; "one image entry required"
    let one = manifest_json(&blob_path());
    let two = format!("[{},{}]", &one[1..one.len() - 1], &one[1..one.len() - 1]);
    for (name, m) in [
        ("two entries", two.as_str()),
        ("none", "[]"),
        ("not a list", "{}"),
    ] {
        let mut v = member(&blob_path(), good_config().as_bytes());
        v.extend(member("manifest.json", m.as_bytes()));
        assert_eq!(read(&end(v)).unwrap_err(), BAD, "{name}");
    }
}

#[test]
fn invalid_json_in_the_manifest_or_the_config_is_unreadable() {
    // 012 T010
    let mj = manifest_json(&blob_path());
    for (m, c) in [
        ("[{\"Config\":".to_string(), good_config()),
        ("not json".to_string(), good_config()),
        (mj.clone(), "{\"os\":".to_string()),
        (mj.clone(), "\0\0\0".to_string()),
        (mj, "[]".to_string()),
    ] {
        let mut v = member(&blob_path(), c.as_bytes());
        v.extend(member("manifest.json", m.as_bytes()));
        assert_eq!(read(&end(v)).unwrap_err(), BAD, "{m} / {c}");
    }
}

#[test]
fn a_config_or_manifest_member_over_1_mb_is_unreadable() {
    // 012 T010; S5 "member size cap 1 MB": one byte over, JSON padded with spaces so only the cap fails it
    let over = (1 << 20) + 1;
    let padded = format!(
        "{}{}",
        good_config(),
        " ".repeat(over - good_config().len())
    );
    assert_eq!(padded.len(), over);
    let mut v = member(&blob_path(), padded.as_bytes());
    v.extend(member(
        "manifest.json",
        manifest_json(&blob_path()).as_bytes(),
    ));
    assert_eq!(read(&end(v)).unwrap_err(), BAD, "config over the cap");

    let m = manifest_json(&blob_path());
    let padded = format!("{m}{}", " ".repeat(over - m.len()));
    let mut v = member(&blob_path(), good_config().as_bytes());
    v.extend(member("manifest.json", padded.as_bytes()));
    assert_eq!(read(&end(v)).unwrap_err(), BAD, "manifest over the cap");
}

#[test]
fn a_config_well_under_the_cap_with_a_big_history_is_read() {
    // 012 T010: the cap must not reject a normal large config (about 300 KB of history)
    let history = format!(
        "[{}]",
        "{\"created_by\":\"RUN true\"},".repeat(12_000) + "{}"
    );
    let cfg = format!(
        "{{\"architecture\":\"amd64\",\"os\":\"linux\",\"config\":{{\"User\":\"65532:65532\",\"Entrypoint\":[\"/hello\"],\"Cmd\":null}},\"history\":{history}}}"
    );
    assert!(cfg.len() > 250_000 && cfg.len() < 1 << 20, "{}", cfg.len());
    let mut v = member(&blob_path(), cfg.as_bytes());
    v.extend(member(
        "manifest.json",
        manifest_json(&blob_path()).as_bytes(),
    ));
    assert_eq!(read(&end(v)).unwrap().user, "65532:65532");
}

#[test]
fn a_member_whose_data_runs_past_the_end_of_the_file_is_unreadable() {
    // 012 T010: the header promises 4096 bytes of config, the file ends after 100
    let mut v = header(blob_path().as_bytes(), 4096, b'0').to_vec();
    v.extend_from_slice(&good_config().as_bytes()[..100]);
    assert_eq!(read(&v).unwrap_err(), BAD);
}

#[test]
fn no_error_text_carries_archive_content() {
    // 012 T010: only fixed strings leave the module; a config full of a canary never shows it
    let canary = "canary-Zq81-value-key-0123456789";
    let cfg = format!("{{\"os\":\"{canary}\",\"architecture\":\"{canary}\",\"config\":{{\"User\":\"{canary}\"}}}}");
    let mut v = member(&blob_path(), cfg.as_bytes());
    v.extend(member(
        "manifest.json",
        manifest_json(&blob_path()).as_bytes(),
    ));
    if let Err(e) = read(&end(v)) {
        assert!(!e.contains(canary), "{e}");
    }
    let bad = read(&[0xffu8; 700]).unwrap_err();
    assert_eq!(bad, BAD);
}
