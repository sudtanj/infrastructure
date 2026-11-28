//! T057. Sanitizer, reserved names, repo-name rule, redaction forms.
//! FR-051, FR-053; I21, I22; M2, M3, M6.
//! Assumed: `text::sanitize(&str, max_chars: usize) -> String`, and
//! that `Redactor::new` also covers base64(value) and base64("x-access-token:" + value).
//! Key collision (A24, I25) has no Rust function (FR-038 keeps key building in the shell): it
//! is pinned in test-factory-ci-secrets.sh.

use crate::pipeline::valid_secret_name;
use crate::redact::Redactor;
use crate::text::sanitize;
use crate::util::base64;
use crate::watch::parse;

fn allowed(c: char) -> bool {
    c.is_ascii_alphanumeric() || " _.:/-".contains(c)
}

#[test]
fn the_sanitizer_keeps_only_the_safe_alphabet() {
    assert_eq!(
        sanitize("step build failed: exit code 1", 140),
        "step build failed: exit code 1"
    );
    assert_eq!(sanitize("a<b>&c|d`e$(id)", 140), "a_b__c_d_e__id_");
    assert_eq!(sanitize("line1\nline2\r\ttab", 140), "line1_line2__tab");
    let u = sanitize("日本語 é", 140);
    assert!(u.chars().all(allowed), "{u}");
    assert!(u.contains(' '));
}

#[test]
fn the_sanitizer_truncates_to_the_limit() {
    let s = sanitize(&"a".repeat(500), 140);
    assert_eq!(s.chars().count(), 140);
    assert_eq!(sanitize("abcdef", 3).chars().count(), 3);
    assert_eq!(sanitize("", 10), "");
}

#[test]
fn a_leading_workflow_command_marker_gets_a_prefix() {
    // M2
    for input in [
        "::error::x",
        "::add-mask::abc",
        "::set-output name=a::b",
        "::",
    ] {
        let s = sanitize(input, 140);
        assert!(!s.starts_with("::"), "{input:?} -> {s:?}");
        assert!(s.contains("error::x") || !input.contains("error"), "{s}");
        assert!(s.chars().count() <= 140);
    }
    let s = sanitize(&format!("::{}", "a".repeat(200)), 20);
    assert!(!s.starts_with("::") && s.chars().count() <= 20, "{s}");
    assert_eq!(
        sanitize("a::b", 10),
        "a::b",
        "only a leading marker is prefixed"
    );
}

#[test]
fn sanitized_text_always_fits_the_alphabet_the_limit_and_the_marker_rule() {
    // I21
    let mut x: u64 = 77;
    let pool: Vec<char> = "ab Z9_.:/-\n\t\r::<>&|`$()'\"\\日é\u{0}\u{1b}[;"
        .chars()
        .collect();
    for _ in 0..500 {
        let len = {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((x >> 33) % 300) as usize
        };
        let input: String = (0..len)
            .map(|_| {
                x = x
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                pool[((x >> 33) as usize) % pool.len()]
            })
            .collect();
        let max = 1 + ((x >> 20) % 150) as usize;
        let s = sanitize(&input, max);
        assert!(s.chars().all(allowed), "{s:?}");
        assert!(s.chars().count() <= max, "{} > {max}", s.chars().count());
        assert!(!s.starts_with("::"), "{s:?}");
    }
}

#[test]
fn reserved_names_match_case_insensitively() {
    // M3, FR-019
    for n in [
        "bws_x",
        "Github_Token",
        "item_repo",
        "ITEM_BLOB",
        "all_secrets",
        "NODE_OPTIONS",
        "bash_env",
        "BASH_FUNC_x",
        "env",
        "Path",
        "ld_library_path",
        "git_ssh",
        "ssl_cert_dir",
        "no_proxy",
        "runner_temp",
        "actions_runtime_token",
        "docker_host",
        "containerd",
        "xdg_config_home",
        "registry_auth",
        "fci_key_sep",
        "factory_ci_token",
        "hostname",
        "tmpdir",
        "shell",
        "home",
        "user",
    ] {
        assert!(!valid_secret_name(n), "{n} must be reserved");
    }
    for n in ["NPM_TOKEN", "DEPLOY_KEY", "AWS_ACCESS_KEY_ID", "api_key"] {
        assert!(valid_secret_name(n), "{n}");
    }
}

#[test]
fn repo_names_with_path_tricks_are_rejected_and_owner_slash_name_accepted() {
    // M3
    let t = |repo: &str| parse(&format!("[[watch]]\nrepo = {repo:?}\n"));
    for bad in [
        "../x", "x/..", "a/b/c", "*", "[a-z]", "a b", "a/b c", "", "/", "a/", "/b",
    ] {
        assert!(t(bad).is_err(), "{bad:?} accepted");
    }
    assert!(t("a/b").is_ok());
}

#[test]
fn the_redactor_covers_base64_of_a_value_and_of_the_clone_header() {
    // M6
    let r = Redactor::new(["tok-ABCD-1234".to_string()]);
    let plain = "tok-ABCD-1234";
    assert_eq!(r.apply(&base64(plain.as_bytes())), "***");
    let header = base64(format!("x-access-token:{plain}").as_bytes());
    assert_eq!(
        r.apply(&format!("Authorization: Basic {header}")),
        "Authorization: Basic ***"
    );
    assert_eq!(r.apply(&format!("a {plain} b")), "a *** b");
}

#[test]
fn values_their_base64_and_their_lines_vanish_from_any_text() {
    // I22
    let mut x: u64 = 1234;
    let mut next = |m: u64| {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (x >> 33) % m
    };
    let upper: Vec<char> = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".chars().collect();
    for _ in 0..200 {
        let lines: Vec<String> = (0..1 + next(3))
            .map(|_| {
                (0..4 + next(9))
                    .map(|_| upper[next(upper.len() as u64) as usize])
                    .collect()
            })
            .collect();
        let value = lines.join("\n");
        let r = Redactor::new([value.clone()]);
        let mut forms = vec![value.clone(), base64(value.as_bytes())];
        forms.extend(lines.iter().cloned());
        let text = format!(
            "start {} mid {} end {}",
            forms[0],
            forms[1],
            forms[2..].join(" and ")
        );
        let out = r.apply(&text);
        for f in &forms {
            assert!(!out.contains(f.as_str()), "{f:?} survived in {out:?}");
        }
    }
}
