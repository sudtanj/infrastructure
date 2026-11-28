use crate::util::base64;

/// Replaces known secret values in text. Every output path goes through it.
pub struct Redactor {
    secrets: Vec<String>,
}

/// Column widths of wrapped base64: most tools use 76, `openssl base64` uses 64.
const WRAPS: [usize; 2] = [76, 64];
const DERIVED_MIN: usize = 8;
const DERIVED_MAX: usize = 48 * 1024;

fn url_safe(s: &str) -> String {
    s.replace('+', "-").replace('/', "_")
}

/// Percent-encoding that leaves ASCII letters, digits and `keep` alone; `plus` writes a space as `+`.
fn percent(s: &str, keep: &str, upper: bool, plus: bool) -> String {
    s.bytes().fold(String::new(), |mut out, b| {
        if b.is_ascii_alphanumeric() || keep.contains(b as char) {
            out.push(b as char);
        } else if plus && b == b' ' {
            out.push('+');
        } else if upper {
            out.push_str(&format!("%{b:02X}"));
        } else {
            out.push_str(&format!("%{b:02x}"));
        }
        out
    })
}

/// A JSON string body with every non-ASCII character written as `\uXXXX`.
fn json_ascii(inner: &str) -> String {
    inner.chars().fold(String::new(), |mut out, c| {
        if c.is_ascii() {
            out.push(c);
        } else {
            for unit in c.encode_utf16(&mut [0; 2]) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
        out
    })
}

impl Redactor {
    // shortcut: a secret split across two output lines is not caught, and a form under 8
    // characters (`echo "$V" | base64` of a value under 3 bytes) is not derived. Upgrade if steps
    // may print such values.
    pub fn new<I: IntoIterator<Item = String>>(values: I) -> Self {
        let mut r = Self {
            secrets: Vec::new(),
        };
        r.add(values);
        r
    }

    /// One batch: the value, its base64 and the clone's Basic header form, each trimmed line of 4
    /// or more characters, and derived forms of 8 to 48 KiB characters (base64 middles at three
    /// alignments, wrapped base64 lines, percent, JSON and hex).
    pub fn add<I: IntoIterator<Item = String>>(&mut self, values: I) {
        for v in values.into_iter().filter(|s| !s.is_empty()) {
            let b64 = base64(v.as_bytes());
            self.secrets
                .push(base64(format!("x-access-token:{v}").as_bytes()));
            // A multi-line value leaks line by line (A22); shorter lines match too much.
            self.secrets.extend(
                v.lines()
                    .map(str::trim)
                    .filter(|l| l.chars().count() >= 4)
                    .map(str::to_string),
            );
            let mut derived = Vec::new();
            // The middle drops a prefix, a trailing newline or padding, so the form still matches.
            for k in 0..3 {
                let n = 3 * (v.len().saturating_sub(k) / 3);
                if n > 0 {
                    let mid = base64(&v.as_bytes()[k..k + n]);
                    // `Log::write` redacts one line at a time and `base64` wraps: the whole-group form
                    // wraps the same way as the value followed by anything else.
                    if k == 0 {
                        for w in WRAPS.into_iter().filter(|w| mid.len() > *w) {
                            derived.extend(
                                (0..mid.len())
                                    .step_by(w)
                                    .map(|i| mid[i..(i + w).min(mid.len())].to_string()),
                            );
                        }
                    }
                    derived.push(url_safe(&mid));
                    derived.push(mid);
                }
            }
            // `echo "$V" | base64`: the newline changes the padding group, which the middle drops.
            derived.push(base64(format!("{v}\n").as_bytes()));
            // Unreserved sets of RFC 3986 and of `encodeURIComponent`, lowercase hex, `+` for a space.
            for (keep, upper, plus) in [
                ("-._~", true, false),
                ("-._~", false, false),
                ("-_.!~*'()", true, false),
                ("-._~", true, true),
            ] {
                derived.push(percent(&v, keep, upper, plus));
            }
            if let Ok(json) = serde_json::to_string(&v) {
                let inner = &json[1..json.len() - 1];
                let slashed = inner.replace('/', "\\/");
                derived.extend([json_ascii(inner), json_ascii(&slashed)]);
                derived.extend([inner.to_string(), slashed]);
            }
            derived.push(v.bytes().map(|b| format!("{b:02x}")).collect());
            derived.push(v.bytes().map(|b| format!("{b:02X}")).collect());
            self.secrets.extend(
                derived
                    .into_iter()
                    .filter(|f| (DERIVED_MIN..=DERIVED_MAX).contains(&f.len())),
            );
            self.secrets.push(b64);
            self.secrets.push(v);
        }
        // Longest first, so a secret that contains another is replaced whole.
        self.secrets
            .sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        self.secrets.dedup();
    }

    /// Every string `apply` masks.
    pub fn forms(&self) -> Vec<String> {
        self.secrets.clone()
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

    // 010 T004. FR-011, FR-013; I7.
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
    fn new_still_masks_a_two_character_value() {
        let r = Redactor::new(["pw".to_string()]);
        assert_eq!(r.apply("a pw b"), "a *** b");
        assert_eq!(r.apply(&base64(b"pw")), "***");
        assert_eq!(r.apply(&base64(b"x-access-token:pw")), "***");
        // the floor is on derived forms only: a one-character value is still masked
        assert_eq!(Redactor::new(["q".to_string()]).apply("a q b"), "a *** b");
    }

    #[test]
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
    // across the newline cannot do that, so `add` also masks each 76-column piece of the standard base64 form.
    #[test]
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

    // S1, S2, M4 of the review: `echo "$V" | base64`, `openssl base64` (64 columns), lowercase percent,
    // `encodeURIComponent`, `+` for a space, uppercase hex and the JSON `\/` and `\uXXXX` forms.
    #[test]
    fn add_masks_the_review_forms() {
        let v = "canary-Zq81-value-key";
        assert_eq!(added(&[v]).apply(&base64(format!("{v}\n").as_bytes())), "***");

        let long = format!("canary-Zq81-{}", "k9".repeat(44));
        let enc = base64(format!("{long}\n").as_bytes());
        let r = added(&[&long]);
        for width in [64, 76] {
            for i in (0..enc.len()).step_by(width) {
                let l = &enc[i..(i + width).min(enc.len())];
                assert!(leaked(&r.apply(l), l) < LEFT, "{width}: {l}");
            }
        }

        let v = "pa ss/wo\"rd\\é=1+x#";
        let r = added(&[v]);
        let lower: String = v
            .bytes()
            .map(|b| match b {
                b if b.is_ascii_alphanumeric() || b"-._~".contains(&b) => (b as char).to_string(),
                b => format!("%{b:02x}"),
            })
            .collect();
        assert_eq!(r.apply(&lower), "***");
        assert_eq!(r.apply(&pct(v).replace("%20", "+")), "***");
        assert_eq!(r.apply(&hex(v.as_bytes()).to_uppercase()), "***");

        let v = "key!(1)*'x/y z~";
        let ecu: String = v
            .bytes()
            .map(|b| match b {
                b if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) => (b as char).to_string(),
                b => format!("%{b:02X}"),
            })
            .collect();
        assert_eq!(added(&[v]).apply(&ecu), "***");

        let v = "a/b/c-secret-key";
        assert_eq!(added(&[v]).apply(&v.replace('/', "\\/")), "***");
        let v = "pässwörd-漢字-😀-key";
        let ascii = "p\\u00e4ssw\\u00f6rd-\\u6f22\\u5b57-\\ud83d\\ude00-key";
        assert_eq!(added(&[v]).apply(ascii), "***");
        let v = "é/漢-secret-key";
        let both = "\\u00e9\\/\\u6f22-secret-key";
        assert_eq!(added(&[v]).apply(both), "***");
    }

    #[test]
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
        assert!(
            !out.contains("line-one") && !out.contains("line-two"),
            "{out}"
        );
    }

    #[test]
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
        // the documented limit (spec R2): `echo "$V" | base64` of a value under 3 bytes is under the floor
        let r = added(&["ab"]);
        let echo = base64(b"ab\n");
        assert_eq!(r.apply(&echo), echo);
        assert_eq!(r.apply(&base64(b"ab")), "***");
        // from 3 bytes up it is 8 characters and masked whole (S1)
        assert_eq!(added(&["abc"]).apply(&base64(b"abc\n")), "***");
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
