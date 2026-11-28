use crate::util::base64;

/// Replaces known secret values in text. Every output path goes through it.
pub struct Redactor {
    secrets: Vec<String>,
}

impl Redactor {
    // shortcut: besides the value, only its base64 and the base64 of the clone's Basic
    // header are caught, and a secret split across two output lines is not; other
    // encodings (url, hex) are not. Upgrade if steps may transform secrets.
    pub fn new<I: IntoIterator<Item = String>>(values: I) -> Self {
        let mut secrets = Vec::new();
        for v in values.into_iter().filter(|s| !s.is_empty()) {
            secrets.push(base64(v.as_bytes()));
            secrets.push(base64(format!("x-access-token:{v}").as_bytes()));
            // A multi-line value leaks line by line (A22); shorter lines match too much.
            secrets.extend(
                v.lines()
                    .map(str::trim)
                    .filter(|l| l.chars().count() >= 4)
                    .map(str::to_string),
            );
            secrets.push(v);
        }
        // Longest first, so a secret that contains another is replaced whole.
        secrets.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        secrets.dedup();
        Self { secrets }
    }

    pub fn apply(&self, text: &str) -> String {
        let mut out = text.to_string();
        for s in &self.secrets {
            out = out.replace(s.as_str(), "***");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_every_occurrence() {
        let r = Redactor::new(["tok123".to_string(), "pw".to_string()]);
        assert_eq!(r.apply("a tok123 b tok123 pw"), "a *** b *** ***");
    }

    #[test]
    fn longest_first_and_empty_ignored() {
        let r = Redactor::new(["abc".to_string(), "abcdef".to_string(), String::new()]);
        assert_eq!(r.apply("abcdef"), "***");
    }

    // T005 (A22, FR-053): each trimmed line of 4 or more characters of a multi-line value.
    #[test]
    fn multi_line_value_redacts_each_long_line_and_leaves_short_ones() {
        let r = Redactor::new(["-----BEGIN\n  body1234  \nab\n-----END".to_string()]);
        assert_eq!(r.apply("x body1234 y"), "x *** y");
        assert_eq!(r.apply("-----BEGIN and -----END"), "*** and ***");
        assert_eq!(r.apply("ab stays"), "ab stays");
        assert_eq!(r.apply("-----BEGIN\n  body1234  \nab\n-----END"), "***");
    }

    // 010 T004 (test-first, each new test is `#[ignore = "test-first"]`). FR-011, FR-013; I7.
    // Planned: `Redactor::add(&mut self, values: I)` with `I: IntoIterator<Item = String>` (one batch,
    // sorted and deduped once; `new` calls it) and `Redactor::forms(&self) -> Vec<String>` (every string it
    // masks). Fake values only.
    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    fn pct(s: &str) -> String {
        s.bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect()
    }

    fn url_safe(s: &str) -> String {
        s.replace('+', "-").replace('/', "_")
    }

    fn unpadded(s: &str) -> String {
        s.trim_end_matches('=').to_string()
    }

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn added(v: &[&str]) -> Redactor {
        let mut r = Redactor::new([]);
        r.add(strs(v));
        r
    }

    /// Length of the longest run of `enc` that `out` still holds.
    fn leaked(out: &str, enc: &str) -> usize {
        let (a, b) = (out.as_bytes(), enc.as_bytes());
        let mut best = 0;
        let mut prev = vec![0usize; b.len() + 1];
        for i in 1..=a.len() {
            let mut cur = vec![0usize; b.len() + 1];
            for j in 1..=b.len() {
                if a[i - 1] == b[j - 1] {
                    cur[j] = prev[j - 1] + 1;
                    best = best.max(cur[j]);
                }
            }
            prev = cur;
        }
        best
    }

    // The stable middle leaves up to about 4 bytes of base64 at each end (spec R2): under 10 characters.
    const LEFT: usize = 10;

    #[test]
    #[ignore = "test-first"]
    fn new_still_masks_a_two_character_value() {
        let r = Redactor::new(["pw".to_string()]);
        assert_eq!(r.apply("a pw b"), "a *** b");
        assert_eq!(r.apply(&base64(b"pw")), "***");
        assert_eq!(r.apply(&base64(b"x-access-token:pw")), "***");
        // the floor is on derived forms only: a one-character value is still masked
        assert_eq!(Redactor::new(["q".to_string()]).apply("a q b"), "a *** b");
    }

    #[test]
    #[ignore = "test-first"]
    fn add_masks_value_and_base64_forms() {
        for v in [
            "canary-Zq81-value-key",
            "canary-Zq81-value-key!",
            "canary-Zq81-value-key!!",
            "key-???>>>-???>>>-end!",
        ] {
            let r = added(&[v]);
            assert_eq!(r.apply(&format!("a {v} b")), "a *** b", "{v}");
            // `echo "$V" | base64`, `printf 'x%s' "$V" | base64`, a two-byte prefix, a suffix, both
            for (label, bytes) in [
                ("echo", format!("{v}\n")),
                ("x prefix", format!("x{v}")),
                ("xy prefix", format!("xy{v}")),
                ("suffix", format!("{v}tail")),
                ("prefix and newline", format!("x{v}\n")),
            ] {
                let enc = base64(bytes.as_bytes());
                for (kind, form) in [
                    ("standard", enc.clone()),
                    ("url-safe", url_safe(&enc)),
                    ("unpadded", unpadded(&enc)),
                    ("url-safe unpadded", unpadded(&url_safe(&enc))),
                ] {
                    let out = r.apply(&form);
                    assert!(out.contains("***"), "{label} {kind} of {v}: {out}");
                    assert!(leaked(&out, &form) < LEFT, "{label} {kind} of {v}: {out}");
                }
            }
        }
    }

    // FR-011 says line wrapping does not hide a value. `base64` wraps at 76 columns and `Log::write` redacts
    // one line at a time, so a line of 76 characters has to be masked on its own. A form that must match
    // across the newline cannot do that: the plan's stable middle is one run of characters. If the plan
    // keeps the middle only, the claim has to leave the spec; otherwise `add` needs the wrapped lines.
    #[test]
    #[ignore = "test-first"]
    fn add_masks_wrapped_base64_line_by_line() {
        let v = format!("canary-Zq81-{}", "k9".repeat(44));
        assert_eq!(v.len(), 100);
        let enc = base64(format!("{v}\n").as_bytes());
        let r = added(&[&v]);
        let lines: Vec<&str> = (0..enc.len())
            .step_by(76)
            .map(|i| &enc[i..(i + 76).min(enc.len())])
            .collect();
        assert!(lines.len() >= 2);
        for l in lines {
            let out = r.apply(l);
            assert!(leaked(&out, l) < LEFT, "{l} -> {out}");
        }
    }

    #[test]
    #[ignore = "test-first"]
    fn add_masks_percent_json_and_hex_forms() {
        let v = "pa ss/wo\"rd\\é=1+x#";
        let r = added(&[v]);
        assert_eq!(r.apply(&pct(v)), "***");
        assert_eq!(r.apply(&format!("q={}&z=1", pct(v))), "q=***&z=1");
        let json = serde_json::to_string(v).unwrap();
        let inner = &json[1..json.len() - 1];
        let out = r.apply(&json);
        assert!(out == "\"***\"" || out == "***", "{out}");
        assert!(!r.apply(&format!("{{\"k\": {json}}}")).contains(inner));
        assert_eq!(r.apply(&hex(v.as_bytes())), "***");
        // a multi-line value escapes its newlines in JSON
        let m = "line-one-1234\nline-two-5678";
        let json = serde_json::to_string(m).unwrap();
        let inner = &json[1..json.len() - 1];
        assert!(inner.contains("\\n"));
        let out = added(&[m]).apply(&json);
        assert!(!out.contains("line-one") && !out.contains("line-two"), "{out}");
    }

    #[test]
    #[ignore = "test-first"]
    fn add_skips_derived_forms_under_8_chars_and_over_48_kib() {
        // the value, its plain base64 and the clone header form are masked whatever the length
        let r = added(&["abc"]);
        assert_eq!(r.apply("abc"), "***");
        assert_eq!(r.apply(&base64(b"abc")), "***");
        assert_eq!(r.apply(&base64(b"x-access-token:abc")), "***");
        // hex of 3 bytes is 6 characters: under the floor, not masked
        assert_eq!(r.apply("616263"), "616263");
        // hex of 4 bytes is 8 characters: at the floor, masked
        assert_eq!(added(&["abcd"]).apply("61626364"), "***");
        // the documented limit (spec R2): `echo "$V" | base64` of a value under 6 bytes
        let r = added(&["abcde"]);
        let echo = base64(b"abcde\n");
        assert_eq!(r.apply(&echo), echo);
        assert_eq!(r.apply(&base64(b"abcde")), "***");
        // 48 KiB cap on a derived form: hex of 24576 bytes is 49152 characters, one more byte is over
        let at = "a".repeat(24576);
        assert_eq!(added(&[&at]).apply(&hex(at.as_bytes())), "***");
        let over = "a".repeat(24577);
        let h = hex(over.as_bytes());
        assert_eq!(added(&[&over]).apply(&h), h);
        // a 32 KiB value: value and plain base64 masked, hex (64 KiB) and percent (96 KiB) skipped
        let big = "=".repeat(32768);
        let r = added(&[&big]);
        assert_eq!(r.apply(&big), "***");
        assert_eq!(r.apply(&base64(big.as_bytes())), "***");
        assert!(r.forms().iter().all(|f| f.len() <= 48 * 1024));
        let h = hex(big.as_bytes());
        assert_eq!(r.apply(&h), h);
        let p = pct(&big);
        assert_eq!(r.apply(&p), p);
    }

    #[test]
    #[ignore = "test-first"]
    fn forms_lists_every_form() {
        let v = "canary Zq81/value=key!";
        let r = added(&[v]);
        let f = r.forms();
        let b = v.as_bytes();
        let wanted = [
            v.to_string(),
            base64(b),
            base64(format!("x-access-token:{v}").as_bytes()),
            base64(&b[0..21]),
            base64(&b[1..22]),
            base64(&b[2..20]),
            pct(v),
            hex(b),
        ];
        for w in &wanted {
            assert!(f.contains(w), "{w} missing from {f:?}");
        }
        // URL-safe middle of a value whose base64 holds `+` and `/`
        let u = "key-???>>>-???>>>-end!";
        let f = added(&[u]).forms();
        let ub = u.as_bytes();
        for mid in [&ub[0..21], &ub[1..22], &ub[2..20]] {
            let std = base64(mid);
            assert!(std.contains('+') || std.contains('/'), "{std}");
            assert!(f.contains(&std), "{std}");
            assert!(f.contains(&url_safe(&std)), "{std}");
        }
        // no duplicates, and an empty value adds nothing
        let mut s = f.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), f.len());
        assert!(added(&[""]).forms().is_empty());
        // each trimmed line of 4 or more characters of a multi-line value, and not a shorter one
        let f = added(&["line-one-1234\n  ab \nline-two-5678"]).forms();
        assert!(f.contains(&"line-one-1234".to_string()));
        assert!(f.contains(&"line-two-5678".to_string()));
        assert!(!f.contains(&"ab".to_string()));
    }

    #[test]
    #[ignore = "test-first"]
    fn add_batch_equals_new() {
        let (a, b) = (
            ["canary-Zq81-value-key", "pw"],
            ["key-???>>>-???>>>-end!", "multi\nline-two-5678"],
        );
        let all: Vec<&str> = a.iter().chain(b.iter()).copied().collect();
        let one = Redactor::new(strs(&all));
        let mut two = Redactor::new(strs(&a));
        two.add(strs(&b));
        let (mut f1, mut f2) = (one.forms(), two.forms());
        f1.sort();
        f2.sort();
        assert_eq!(f1, f2);
        for text in [
            "pw and canary-Zq81-value-key and line-two-5678".to_string(),
            base64(b"canary-Zq81-value-key\n"),
            format!("{} {}", pct("key-???>>>-???>>>-end!"), hex(b"pw")),
        ] {
            assert_eq!(one.apply(&text), two.apply(&text), "{text}");
        }
        // an empty batch changes nothing
        let mut three = Redactor::new(strs(&all));
        three.add(Vec::<String>::new());
        assert_eq!(three.forms().len(), f1.len());
    }

    // I7 over generated values and texts (no proptest in the crate: a fixed LCG, as in `watch_tests`).
    #[test]
    #[ignore = "test-first"]
    fn add_then_add_masks_what_new_of_both_masks_for_generated_values() {
        let mut x: u64 = 7;
        let mut next = |n: usize| {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((x >> 33) as usize) % n
        };
        let alphabet = b"abcdefgh01 -_=/+?>";
        for _ in 0..200 {
            let mut values = Vec::new();
            for _ in 0..(1 + next(4)) {
                let len = 1 + next(24);
                values.push(
                    (0..len)
                        .map(|_| alphabet[next(alphabet.len())] as char)
                        .collect::<String>(),
                );
            }
            let cut = next(values.len() + 1);
            let mut text = String::new();
            for _ in 0..6 {
                let v = &values[next(values.len())];
                text.push_str(&match next(5) {
                    0 => v.clone(),
                    1 => base64(v.as_bytes()),
                    2 => hex(v.as_bytes()),
                    3 => pct(v),
                    _ => "plain words".to_string(),
                });
                text.push(' ');
            }
            let one = Redactor::new(values.clone());
            let mut two = Redactor::new(values[..cut].to_vec());
            two.add(values[cut..].to_vec());
            assert_eq!(one.apply(&text), two.apply(&text), "{values:?} {text:?}");
            // `new([v])` masks `v` for any non-empty `v`
            for v in &values {
                assert_eq!(Redactor::new([v.clone()]).apply(v), "***", "{v:?}");
            }
        }
    }

    #[test]
    #[ignore = "test-first"]
    fn add_keeps_longest_first_across_batches() {
        let mut r = Redactor::new(["abcdef".to_string()]);
        r.add(["abcdef-longer-value".to_string()]);
        assert_eq!(r.apply("abcdef-longer-value"), "***");
        assert_eq!(r.apply("abcdef"), "***");
        // the other order gives the same text
        let mut r = Redactor::new(["abcdef-longer-value".to_string()]);
        r.add(["abcdef".to_string()]);
        assert_eq!(r.apply("abcdef-longer-value"), "***");
        assert_eq!(r.apply("x abcdef y"), "x *** y");
    }
}
