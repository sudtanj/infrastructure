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
}
