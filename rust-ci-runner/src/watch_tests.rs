//! T010. Targets `watch::parse`, `watch::glob_match`, `WatchEntry::matches_branch`
//! (contracts/watch-list.md, data-model.md WatchEntry). FR-004, FR-005; AS 9.1 to 9.3.

use crate::watch::{glob_match, parse};

fn one(body: &str) -> String {
    format!("[[watch]]\nrepo = \"o/r\"\n{body}\n")
}

fn problems(text: &str) -> Vec<String> {
    parse(text).expect_err("expected the list to be refused")
}

fn mentions(p: &[String], needle: &str) -> bool {
    p.iter().any(|l| l.contains(needle))
}

#[test]
fn defaults_are_all_branches_no_secrets_no_docker() {
    // AS 9.1
    let v = parse(&one("")).unwrap();
    assert_eq!(v.len(), 1);
    let e = &v[0];
    assert_eq!(e.repo, "o/r");
    assert_eq!(e.id, None);
    assert!(e.secrets.is_empty());
    assert!(!e.allow_docker);
    assert!(e.matches_branch("main") && e.matches_branch("feat/x/y"));
}

#[test]
fn empty_list_is_valid() {
    // data-model.md: zero entries is valid
    assert!(parse("").unwrap().is_empty());
    assert!(parse("# only a comment\n").unwrap().is_empty());
}

#[test]
fn unknown_keys_are_errors_at_root_and_in_an_entry() {
    // FR-004
    let p = problems(&format!("bogus = 1\n{}", one("")));
    assert!(mentions(&p, "bogus"), "{p:?}");
    let p = problems(&one("branch = [\"main\"]"));
    assert!(mentions(&p, "branch"), "{p:?}");
}

#[test]
fn bitwarden_project_is_an_unknown_key() {
    // T048 / FR-040: there are no per-repo projects
    let p = problems(&one("bitwarden_project = \"x\""));
    assert!(mentions(&p, "bitwarden_project"), "{p:?}");
}

#[test]
fn malformed_repo_names_are_refused() {
    // FR-004, data-model.md WatchEntry.repo
    let owner40 = format!("{}/x", "a".repeat(40));
    let bad = [
        "a/b/c",
        "a",
        "../x",
        "x/..",
        "é/x",
        "a\\nb/c",
        "ab-/x",
        "-ab/x",
        owner40.as_str(),
        "*",
        "[a-z]",
        "a b/c",
        "a/b c",
        "a/b..c",
        "",
    ];
    for repo in bad {
        let text = format!("[[watch]]\nrepo = \"{repo}\"\n");
        let p = problems(&text);
        assert!(!p.is_empty(), "{repo:?} was accepted");
    }
    for repo in ["a/b", "Some-Owner/some.repo_1", "a/.github"] {
        let text = format!("[[watch]]\nrepo = \"{repo}\"\n");
        assert!(parse(&text).is_ok(), "{repo:?} was refused");
    }
}

#[test]
fn missing_repo_is_a_problem() {
    assert!(!problems("[[watch]]\nallow_docker = true\n").is_empty());
}

#[test]
fn duplicates_compare_case_insensitively() {
    let t = "[[watch]]\nrepo = \"Foo/Bar\"\n[[watch]]\nrepo = \"foo/bar\"\n";
    let p = problems(t);
    assert!(mentions(&p, "duplicate of entry 1"), "{p:?}");
}

#[test]
fn malformed_globs_are_refused() {
    // data-model.md WatchEntry.branches
    let long = "a".repeat(101);
    for g in [
        "",
        "feat[",
        "a]b",
        "{a,b}",
        "a b",
        "a..b",
        "a\\\\b",
        "!x",
        "^x",
        "~x",
        "a:b",
        long.as_str(),
    ] {
        let p = problems(&one(&format!("branches = [{g:?}]")));
        assert!(!p.is_empty(), "glob {g:?} accepted");
    }
    let many: Vec<String> = (0..21).map(|i| format!("\"b{i}\"")).collect();
    assert!(!problems(&one(&format!("branches = [{}]", many.join(",")))).is_empty());
    assert!(parse(&one("branches = [\"main\", \"release/*\", \"v?.0\"]")).is_ok());
}

#[test]
fn secret_branches_follow_the_glob_rules() {
    // T048
    assert!(parse(&one("secret_branches = [\"main\", \"rel/*\"]")).is_ok());
    assert!(!problems(&one("secret_branches = [\"a[b\"]")).is_empty());
    assert!(!problems(&one("secret_branches = [\"\"]")).is_empty());
}

#[test]
fn glob_matching_star_crosses_slash_and_question_matches_one() {
    assert!(glob_match("main", "main"));
    assert!(!glob_match("main", "main2"));
    assert!(glob_match("release/*", "release/1.0/rc"));
    assert!(glob_match("*", "a/b/c"));
    assert!(glob_match("v?.0", "v1.0"));
    assert!(!glob_match("v?.0", "v10.0"));
    assert!(glob_match("*x*y*", "aaxbbyccc"));
    assert!(!glob_match("*x*y*", "aaybbxcc"));
    assert!(glob_match("", ""));
    // a hostile pattern must not take exponential time
    let pat = "*a".repeat(30) + "b";
    assert!(!glob_match(&pat, &"a".repeat(200)));
}

#[test]
fn matches_branch_uses_any_of_the_globs() {
    let v = parse(&one("branches = [\"main\", \"release/*\"]")).unwrap();
    assert!(v[0].matches_branch("main") && v[0].matches_branch("release/9"));
    assert!(!v[0].matches_branch("feature"));
}

#[test]
fn reserved_secret_names_are_refused() {
    // FR-005 / FR-019, T048: BWS_ACCESS_TOKEN
    for n in [
        "FACTORY_CI_LOG_URL",
        "FACTORY_CI_LOG_USER",
        "FACTORY_CI_LOG_TOKEN",
        "GITHUB_X",
        "DOCKER_HOST",
        "BWS_ACCESS_TOKEN",
        "PATH",
        "1BAD",
        "A-B",
    ] {
        let p = problems(&one(&format!("secrets = [{n:?}]")));
        assert!(mentions(&p, n), "{n}: {p:?}");
    }
    assert!(parse(&one("secrets = [\"NPM_TOKEN\"]")).is_ok());
}

#[test]
fn duplicate_secret_and_too_many_secrets_are_refused() {
    assert!(!problems(&one("secrets = [\"A\", \"A\"]")).is_empty());
    let many: Vec<String> = (0..21).map(|i| format!("\"S{i}\"")).collect();
    assert!(!problems(&one(&format!("secrets = [{}]", many.join(",")))).is_empty());
    let twenty: Vec<String> = (0..20).map(|i| format!("\"S{i}\"")).collect();
    assert!(parse(&one(&format!("secrets = [{}]", twenty.join(",")))).is_ok());
}

#[test]
fn id_must_be_a_number() {
    // data-model.md WatchEntry.id; contract line `id "abc" is not a number`
    let p = problems(&one("id = \"abc\""));
    assert!(!p.is_empty(), "{p:?}");
    assert_eq!(
        parse(&one("id = 123456789")).unwrap()[0].id,
        Some(123_456_789)
    );
}

#[test]
fn limits_on_entries_and_file_size() {
    // data-model.md: 100 entries, 64 KB
    let n = |k: usize| {
        (0..k)
            .map(|i| format!("[[watch]]\nrepo = \"o/r{i}\"\n"))
            .collect::<String>()
    };
    assert_eq!(parse(&n(100)).unwrap().len(), 100);
    assert!(!problems(&n(101)).is_empty());
    let big = format!("{}\n{}", "# pad".repeat(14_000), n(1));
    assert!(big.len() > 64 * 1024);
    let p = problems(&big);
    assert!(mentions(&p, "64"), "{p:?}");
}

#[test]
fn every_problem_is_reported_at_once_one_per_line() {
    // AS 9.3, contracts/watch-list.md format
    let t = "[[watch]]\nrepo = \"a/b/c\"\n[[watch]]\nrepo = \"x/y\"\nsecrets = [\"GITHUB_X\"]\nbranches = [\"a[\"]\n[[watch]]\nrepo = \"x/y\"\n";
    let p = problems(t);
    assert!(p.len() >= 4, "{p:?}");
    assert!(p.iter().all(|l| !l.contains('\n')), "{p:?}");
    assert!(p.iter().any(|l| l.starts_with("entry 1:")), "{p:?}");
    assert!(
        p.iter()
            .any(|l| l.starts_with("x/y:") && l.contains("GITHUB_X")),
        "{p:?}"
    );
    assert!(
        p.iter()
            .any(|l| l.starts_with("x/y:") && l.contains("duplicate of entry 2")),
        "{p:?}"
    );
}

#[test]
fn malformed_toml_is_one_problem_not_a_panic() {
    assert!(!problems("[[watch]\nrepo = ").is_empty());
}

// I10: generator from valid parts validates; an unknown key anywhere makes it fail.
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

fn generated(rng: &mut Lcg) -> (String, usize) {
    let n = 1 + rng.next(8) as usize;
    let mut text = String::new();
    for i in 0..n {
        text += &format!("[[watch]]\nrepo = \"owner{}/repo{i}\"\n", rng.next(5));
        if rng.next(2) == 0 {
            text += &format!("id = {}\n", rng.next(1 << 40));
        }
        if rng.next(2) == 0 {
            text += "branches = [\"main\", \"rel/*\"]\n";
        }
        if rng.next(2) == 0 {
            text += "secret_branches = [\"main\"]\n";
        }
        if rng.next(2) == 0 {
            text += "secrets = [\"NPM_TOKEN\", \"DEPLOY_KEY\"]\n";
        }
        if rng.next(2) == 0 {
            text += &format!("allow_docker = {}\n", rng.next(2) == 0);
        }
    }
    (text, n)
}

#[test]
fn generated_lists_validate_and_an_unknown_key_breaks_them() {
    let mut rng = Lcg(0x5eed);
    for _ in 0..200 {
        let (text, n) = generated(&mut rng);
        assert_eq!(parse(&text).unwrap().len(), n, "{text}");
        // append an unknown key to the last table
        let bad = format!("{text}zzz_unknown = 1\n");
        assert!(mentions(&problems(&bad), "zzz_unknown"), "{bad}");
    }
}

// ---- 008 T007 (test-first): `watch::read(&Path) -> Result<Vec<WatchEntry>, String>`. FR-007 shared ----

fn temp_file(bytes: &[u8]) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("fci-watch-{}.toml", crate::util::random_id()));
    std::fs::write(&p, bytes).unwrap();
    p
}

#[test]
fn read_helper_parses_and_caps_like_the_old_load_list() {
    let p = temp_file(b"[[watch]]\nrepo = \"o/r\"\nid = 7\nbranches = [\"main\"]\n");
    let v = crate::watch::read(&p).unwrap();
    assert_eq!((v.len(), v[0].id), (1, Some(7)));
    // a parse problem comes back joined, one per line
    let q = temp_file(b"[[watch]]\nrepo = \"bad\"\nbogus = 1\n");
    let e = crate::watch::read(&q).unwrap_err();
    assert!(e.lines().count() >= 2 && e.contains("bogus"), "{e}");
    // missing file
    let gone = std::env::temp_dir().join(format!("fci-watch-none-{}", crate::util::random_id()));
    let e = crate::watch::read(&gone).unwrap_err();
    assert!(e.starts_with("cannot read "), "{e}");
    // not UTF-8
    let u = temp_file(&[b'#', 0xff, 0xfe, b'\n']);
    assert_eq!(
        crate::watch::read(&u).unwrap_err(),
        "watch list is not valid UTF-8"
    );
    // one byte over the cap is refused, at the cap it is read
    let big = temp_file(&vec![b'#'; crate::watch::MAX_BYTES + 1]);
    let e = crate::watch::read(&big).unwrap_err();
    assert!(e.contains("larger than"), "{e}");
    let edge = temp_file(&vec![b'#'; crate::watch::MAX_BYTES]);
    assert!(crate::watch::read(&edge).unwrap().is_empty());
    for f in [p, q, u, big, edge] {
        let _ = std::fs::remove_file(f);
    }
}

#[test]
fn entries_without_branches_match_every_ref() {
    let v = parse(&one("id = 1")).unwrap();
    let long = "z".repeat(200);
    for r in ["main", "feat/x/y", "v1.2.3", "a", long.as_str()] {
        assert!(v[0].matches_branch(r), "{r}");
    }
    let v = parse(&one("branches = [\"main\"]")).unwrap();
    assert!(
        v[0].matches_branch("main")
            && !v[0].matches_branch("main2")
            && !v[0].matches_branch("feat/main")
    );
}

// ---- 010 T003: FR-004, FR-010 ----
// `WatchEntry::secrets_allowed_on(&self, git_ref: &str) -> bool`: the ref matches one of `secret_branches`. Without
// `secret_branches` no ref does (M3), whatever `branches` says.

fn gate(body: &str, git_ref: &str) -> bool {
    parse(&one(body)).unwrap()[0].secrets_allowed_on(git_ref)
}

#[test]
fn each_runner_setting_in_secrets_refuses_the_file() {
    for n in [
        "FACTORY_CI_TOKEN",
        "FACTORY_CI_LOG_URL",
        "FACTORY_CI_LOG_USER",
        "FACTORY_CI_LOG_TOKEN",
        "BW_X",
        "bw_x",
        "BITWARDEN",
        "bitwarden_x",
    ] {
        let p = problems(&one(&format!("branches = [\"main\"]\nsecrets = [{n:?}]")));
        assert!(mentions(&p, n), "{n}: {p:?}");
    }
    // one such name refuses the whole file, valid names next to it do not save it
    let p = problems(&one("secrets = [\"SITE_API_KEY\", \"BW_X\"]"));
    assert!(mentions(&p, "BW_X"), "{p:?}");
    assert!(parse(&one("secrets = [\"SITE_API_KEY\"]")).is_ok());
}

#[test]
fn secrets_allowed_on_refuses_branches_omitted() {
    for r in ["main", "dev", "release/1", "feat/x/y"] {
        assert!(!gate("", r), "{r}");
        assert!(!gate("secrets = [\"SITE_API_KEY\"]", r), "{r}");
    }
}

#[test]
fn secrets_allowed_on_refuses_every_pattern_made_only_of_star_and_question_mark() {
    for pat in ["*", "?*", "*?", "**", "?", "??", "*?*"] {
        for r in ["main", "dev", "release/1"] {
            assert!(!gate(&format!("branches = [{pat:?}]"), r), "{pat} {r}");
        }
    }
    // one wild pattern spoils the list, even next to an explicit one
    assert!(!gate("branches = [\"main\", \"*\"]", "main"));
    assert!(!gate("branches = [\"?*\", \"main\"]", "main"));
}

#[test]
fn secrets_allowed_on_refuses_every_ref_without_secret_branches() {
    for b in [
        "branches = [\"main\"]",
        "branches = [\"release/*\"]",
        "branches = [\"main\", \"release/*\", \"v?.0\"]",
    ] {
        for r in ["main", "release/1.0", "release/1.0/rc", "v1.0", "dev"] {
            assert!(!gate(b, r), "{b} {r}");
        }
    }
}

#[test]
fn secrets_allowed_on_follows_secret_branches_when_set() {
    // matching and not matching, with `branches` left at its default
    assert!(gate("secret_branches = [\"main\"]", "main"));
    assert!(!gate("secret_branches = [\"main\"]", "dev"));
    assert!(gate(
        "secret_branches = [\"main\", \"release/*\"]",
        "release/2"
    ));
    assert!(!gate(
        "secret_branches = [\"main\", \"release/*\"]",
        "feature"
    ));
    // set but empty matches nothing
    assert!(!gate("secret_branches = []", "main"));
    // with `branches` explicit too, the two lists agree on `main` and differ on `dev`
    let b = "branches = [\"main\", \"dev\"]\nsecret_branches = [\"main\"]";
    assert!(gate(b, "main"));
    assert!(!gate(b, "dev"));
}

#[test]
fn secrets_allowed_on_secret_branches_star_opens_it_on_purpose() {
    for r in ["main", "dev", "release/1", "feat/x/y"] {
        assert!(gate("secret_branches = [\"*\"]", r), "{r}");
    }
    assert!(gate(
        "branches = [\"main\"]\nsecret_branches = [\"*\"]",
        "main"
    ));
}
