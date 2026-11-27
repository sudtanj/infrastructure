use std::fmt::Write;

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// 128 random bits as hex: the unguessable run ID.
pub fn random_id() -> String {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).expect("operating system randomness unavailable");
    hex(&b)
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Standard alphabet with padding (RFC 4648), for the Basic auth header of the log push.
pub fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(c.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(c.get(2).copied().unwrap_or(0));
        for i in 0..4 {
            if i <= c.len() {
                s.push(A[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// `YYYY-MM-DDTHH:MM:SSZ` to Unix seconds. Anything else, including a bad date, is `None`.
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let shape = b.len() == 20
        && [
            (4, b'-'),
            (7, b'-'),
            (10, b'T'),
            (13, b':'),
            (16, b':'),
            (19, b'Z'),
        ]
        .iter()
        .all(|&(i, c)| b[i] == c);
    if !shape {
        return None;
    }
    let num = |a: usize, z: usize| -> Option<i64> {
        let t = s.get(a..z)?;
        if t.bytes().all(|c| c.is_ascii_digit()) {
            t.parse().ok()
        } else {
            None
        }
    };
    let (y, m, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, sec) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let dim = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=12).contains(&m) || d < 1 || d > dim[m as usize - 1] || h > 23 || mi > 59 || sec > 59 {
        return None;
    }
    // days from civil (Hinnant): months counted from March
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    Some(days * 86_400 + h * 3600 + mi * 60 + sec)
}

/// The first 8 characters, or the whole input when it is shorter.
pub fn short_sha(s: &str) -> &str {
    s.get(..8).unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_encodes() {
        assert_eq!(hex(&[0, 255, 16]), "00ff10");
    }

    #[test]
    fn ids_are_unique_run_ids() {
        let a = random_id();
        assert_ne!(a, random_id());
    }

    // T003: base64 (RFC 4648 vectors), parse_rfc3339, short_sha.
    #[test]
    fn base64_matches_rfc4648_vectors() {
        let v = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
            ("user:token", "dXNlcjp0b2tlbg=="),
        ];
        for (plain, enc) in v {
            assert_eq!(base64(plain.as_bytes()), enc, "{plain:?}");
        }
        assert_eq!(base64(&[0xfb, 0xff, 0xfe]), "+//+");
    }

    #[test]
    fn rfc3339_parses_utc_and_rejects_garbage() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339("2026-11-26T00:00:00Z"), Some(1_795_651_200));
        assert_eq!(parse_rfc3339("2026-11-26T01:02:03Z"), Some(1_795_654_923));
        let a = parse_rfc3339("2024-02-28T00:00:00Z").unwrap();
        let b = parse_rfc3339("2024-02-29T00:00:00Z").unwrap();
        let c = parse_rfc3339("2024-03-01T00:00:00Z").unwrap();
        assert_eq!((b - a, c - b), (86_400, 86_400), "leap day");
        for bad in [
            "",
            "garbage",
            "2026-11-26",
            "2026-13-01T00:00:00Z",
            "2026-11-26T00:00:00",
        ] {
            assert_eq!(parse_rfc3339(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn short_sha_is_eight_chars_or_the_whole_input() {
        assert_eq!(short_sha("0123456789abcdef"), "01234567");
        assert_eq!(short_sha("abc"), "abc");
        assert_eq!(short_sha(""), "");
    }
}
