use crate::redact::Redactor;
use crate::store::REQUIRED_KEYS;
use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError, RwLock};

/// A line is shipped in pieces of at most this many bytes (FR-036).
const MAX_PIECE: usize = 16 * 1024;
const CUT_TEXT: &str = "log cut off at the size limit";

/// Every text `Log::fault` prints in public mode (FR-019). The two `secret <KEY>:` lines follow a rule.
pub const FAULTS: &[&str] = &[
    "bad arguments",
    "repository or ref not in the watch list",
    "BWS_ACCESS_TOKEN is not set",
    "secret store unreachable",
    "secret store rejected the access token",
    LOG_STORE_HINT,
    "log store check failed",
    "cannot reach the container engine",
    "repository identity check failed",
    "clone failed",
    "pipeline file refused",
    "docker steps are refused",
    "runner fault",
    "internal error",
    "step secret not allowed",
    "step secret unavailable",
];

/// Printed when the log store answers 401 or 403 to the probe.
pub const LOG_STORE_HINT: &str = "log store check failed: check FACTORY_CI_LOG_URL (host only, e.g. https://logs-prod-xxx.grafana.net), FACTORY_CI_LOG_USER (numeric user id) and FACTORY_CI_LOG_TOKEN (needs logs:write)";

fn is_fault(text: &str) -> bool {
    if FAULTS.contains(&text) {
        return true;
    }
    let Some((key, rest)) = text
        .strip_prefix("secret ")
        .and_then(|r| r.split_once(": "))
    else {
        return false;
    };
    REQUIRED_KEYS.contains(&key)
        && (rest == "unusable value"
            || rest
                .strip_prefix("expected 1, found ")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())))
}

/// The one place the public output grammar lives (FR-018): the line to print, or `None`. The reason
/// after `failed in Ns` and `skipped` is cut; `runner` alone says the pipeline and log store lines.
pub fn public_form(who: &str, text: &str) -> Option<String> {
    if !(1..=64).contains(&who.len())
        || !who
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return None;
    }
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let secs = |s: &str, p: &str| {
        s.strip_prefix(p)
            .and_then(|r| r.strip_suffix('s'))
            .is_some_and(digits)
    };
    if who == "runner" {
        let ok = [
            "pipeline passed",
            "pipeline failed",
            "log lines were dropped",
        ]
        .contains(&text)
            || text
                .strip_prefix("log store answered HTTP ")
                .is_some_and(|c| c.len() == 3 && digits(c));
        return ok.then(|| format!("[runner] {text}"));
    }
    let head = text.split_once(':').map_or(text, |(h, _)| h);
    if text == "started" || secs(text, "success in ") {
        Some(format!("[{who}] {text}"))
    } else if head == "skipped" || secs(head, "failed in ") {
        Some(format!("[{who}] {head}"))
    } else {
        None
    }
}

/// Receives every redacted piece the log keeps, for shipping to a log store.
pub trait Sink: Send + Sync {
    fn push(&self, who: &str, stream: &str, text: &str);
}

/// The run log. Every line is prefixed with its source (step name or `runner`), passed
/// through the redactor, and cut off at the size limit.
pub struct Log {
    inner: Mutex<Inner>,
    red: RwLock<Redactor>,
    sink: Option<Arc<dyn Sink>>,
    quiet: bool,
    public: bool,
}

struct Inner {
    out: Box<dyn Write + Send>,
    err: Box<dyn Write + Send>,
    left: u64,
    cut: bool,
}

impl Log {
    pub fn new(out: Box<dyn Write + Send>, red: Redactor, limit: u64) -> Log {
        Log {
            inner: Mutex::new(Inner {
                out,
                err: Box::new(std::io::stderr()),
                left: limit,
                cut: false,
            }),
            red: RwLock::new(red),
            sink: None,
            quiet: false,
            public: false,
        }
    }

    pub fn with_sink(mut self, sink: Arc<dyn Sink>) -> Log {
        self.sink = Some(sink);
        self
    }

    /// The stream `fault` writes to.
    pub fn with_err(self, err: Box<dyn Write + Send>) -> Log {
        if let Ok(mut g) = self.inner.lock() {
            g.err = err;
        }
        self
    }

    /// Quiet: `out` text is not printed (still shipped); `event` lines are.
    pub fn quiet(mut self, quiet: bool) -> Log {
        self.quiet = quiet;
        self
    }

    /// Public: only `event` lines that match `public_form` and `fault` lines print; the sink gets all.
    pub fn public_events(mut self, public: bool) -> Log {
        self.public = public;
        self
    }

    /// Masks `values` in every line written from now on; earlier lines stay as they were.
    pub fn add_secrets(&self, values: Vec<String>) {
        self.red
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .add(values);
    }

    pub fn secret_forms(&self) -> Vec<String> {
        self.red
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .forms()
    }

    /// Length of the longest form, 0 when there is none.
    pub fn longest_secret(&self) -> usize {
        self.secret_forms()
            .iter()
            .map(String::len)
            .max()
            .unwrap_or(0)
    }

    /// Prints `[runner] <text>` on the fault stream when `text` is in `FAULTS`; drops anything else.
    pub fn fault(&self, text: &str) {
        if !is_fault(text) {
            return;
        }
        if let Ok(mut g) = self.inner.lock() {
            let _ = writeln!(g.err, "[runner] {text}");
            let _ = g.err.flush();
        }
    }

    pub fn line(&self, who: &str, text: &str) {
        self.out(who, "stdout", text);
    }

    /// Step or clone output: printed unless quiet or public, shipped with its stream.
    pub fn out(&self, who: &str, stream: &str, text: &str) {
        self.write(who, stream, text, !self.quiet && !self.public);
    }

    /// A runner event (started, result, duration): always printed. Callers pass fixed text.
    pub fn event(&self, who: &str, text: &str) {
        self.write(who, "stdout", text, true);
    }

    fn write(&self, who: &str, stream: &str, text: &str, print: bool) {
        // Control characters other than tab could rewrite a terminal line or forge a log line (R7).
        let raw = text.trim_end_matches(['\r', '\n']);
        let text = self
            .red
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .apply(raw);
        let bad = |c: char| c.is_control() && c != '\t';
        let text: String = text
            .chars()
            .map(|c| if bad(c) { '\u{fffd}' } else { c })
            .collect();
        let Ok(mut g) = self.inner.lock() else {
            return;
        };
        let mut rest = text.as_str();
        loop {
            let mut n = rest.len().min(MAX_PIECE);
            while !rest.is_char_boundary(n) {
                n -= 1;
            }
            let (piece, tail) = rest.split_at(n);
            if !self.emit(&mut g, who, stream, piece, raw, print) || tail.is_empty() {
                return;
            }
            rest = tail;
        }
    }

    /// False once the size limit is hit; the cut-off marker is printed and queued once. In public
    /// mode the limit applies to step output only: the fixed event lines are short, bounded by the
    /// pipeline, and never cut, so a long step cannot hide `pipeline passed`.
    fn emit(
        &self,
        g: &mut Inner,
        who: &str,
        stream: &str,
        piece: &str,
        raw: &str,
        print: bool,
    ) -> bool {
        let fixed = self.public && print;
        if g.cut && !fixed {
            return false;
        }
        let msg = format!("[{who}] {piece}\n");
        // shortcut: write errors (disk full) are dropped; the run result is still kept.
        if !fixed && msg.len() as u64 > g.left {
            g.cut = true;
            if !self.public {
                let _ = writeln!(g.out, "[runner] {CUT_TEXT}");
            }
            if let Some(s) = &self.sink {
                s.push("runner", "stdout", CUT_TEXT);
            }
            let _ = g.out.flush();
            return false;
        }
        if !fixed {
            g.left -= msg.len() as u64;
        }
        if print {
            let shown = if self.public {
                public_form(who, raw).map(|l| l + "\n")
            } else {
                Some(msg)
            };
            if let Some(m) = shown {
                let _ = g.out.write_all(m.as_bytes());
                let _ = g.out.flush();
            }
        }
        if let Some(s) = &self.sink {
            s.push(who, stream, piece);
        }
        true
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Arc;

    /// A log sink the test can read back.
    #[derive(Clone, Default)]
    pub struct Buf(pub Arc<Mutex<Vec<u8>>>);

    impl Write for Buf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Buf {
        pub fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap()).to_string()
        }
    }

    #[test]
    fn prefixes_and_redacts() {
        let buf = Buf::default();
        let log = Log::new(
            Box::new(buf.clone()),
            Redactor::new(["hunter2".to_string()]),
            1000,
        );
        log.line("build", "password is hunter2\r\n");
        assert_eq!(buf.text(), "[build] password is ***\n");
    }

    #[test]
    fn cuts_off_at_the_limit_once() {
        let buf = Buf::default();
        let log = Log::new(Box::new(buf.clone()), Redactor::new([]), 20);
        log.line("a", "0123456789");
        log.line("a", "0123456789");
        log.line("a", "after");
        assert_eq!(
            buf.text(),
            "[a] 0123456789\n[runner] log cut off at the size limit\n"
        );
    }

    // T005 (FR-024, FR-030, FR-032, FR-036): out, event, quiet mode, sink.
    #[derive(Default)]
    struct Rec(Mutex<Vec<(String, String, String)>>);

    impl Sink for Rec {
        fn push(&self, who: &str, stream: &str, text: &str) {
            self.0
                .lock()
                .unwrap()
                .push((who.to_string(), stream.to_string(), text.to_string()));
        }
    }

    fn with_rec(secrets: &[&str], limit: u64) -> (Log, Buf, Arc<Rec>) {
        let (buf, rec) = (Buf::default(), Arc::new(Rec::default()));
        let red = Redactor::new(secrets.iter().map(|s| s.to_string()));
        let log = Log::new(Box::new(buf.clone()), red, limit).with_sink(rec.clone());
        (log, buf, rec)
    }

    fn pushed(rec: &Rec) -> Vec<(String, String, String)> {
        rec.0.lock().unwrap().clone()
    }

    #[test]
    fn out_prefixes_prints_and_pushes_with_its_stream() {
        let (log, buf, rec) = with_rec(&[], 1000);
        log.out("build", "stderr", "oops\r\n");
        assert_eq!(buf.text(), "[build] oops\n");
        assert_eq!(
            pushed(&rec),
            [("build".into(), "stderr".into(), "oops".into())]
        );
    }

    #[test]
    fn sink_gets_redacted_text_only() {
        // I7
        let (log, buf, rec) = with_rec(&["hunter2"], 1000);
        log.out("s", "stdout", "pw=hunter2");
        log.event("s", "started");
        let all: String = pushed(&rec)
            .iter()
            .map(|p| p.2.clone())
            .collect::<Vec<_>>()
            .join("|");
        assert!(!all.contains("hunter2") && all.contains("pw=***"), "{all}");
        assert!(!buf.text().contains("hunter2"));
    }

    #[test]
    fn long_lines_are_redacted_then_split_at_16kb_on_char_boundaries() {
        // I8: a secret that straddles the cut must not survive in either piece.
        let (log, _buf, rec) = with_rec(&["SECRET123"], u64::MAX);
        let text = format!("{}SECRET123{}", "a".repeat(16_380), "é".repeat(20_000));
        log.out("s", "stdout", &text);
        let parts = pushed(&rec);
        assert!(parts.len() >= 3, "{}", parts.len());
        let joined: String = parts.iter().map(|p| p.2.as_str()).collect();
        assert_eq!(joined, text.replace("SECRET123", "***"));
        for p in &parts {
            assert!(p.2.len() <= 16 * 1024, "piece of {} bytes", p.2.len());
            assert!(!p.2.contains("SECRET") && !p.2.contains("123é"));
        }
    }

    #[test]
    fn quiet_mode_prints_events_but_no_output_text() {
        // FR-030, I17
        let (log, buf, rec) = with_rec(&[], 1000);
        let log = log.quiet(true);
        log.out("build", "stdout", "compiling secret-ish things");
        log.event("build", "started");
        log.event("build", "success in 12s");
        assert_eq!(buf.text(), "[build] started\n[build] success in 12s\n");
        assert_eq!(pushed(&rec).len(), 3, "quiet mode still ships the output");
    }

    #[test]
    fn no_printed_line_starts_with_the_workflow_command_marker() {
        // I17
        let (log, buf, _rec) = with_rec(&[], 1000);
        log.out("s", "stdout", "::add-mask::x");
        log.out("s", "stderr", "::error::boom");
        log.event("s", "failed in 1s");
        for l in buf.text().lines() {
            assert!(l.starts_with('['), "{l:?}");
        }
    }

    #[test]
    fn nul_becomes_replacement_char_on_the_shipped_copy() {
        let (log, _buf, rec) = with_rec(&[], 1000);
        log.out("s", "stdout", "a\0b");
        assert_eq!(pushed(&rec)[0].2, "a\u{fffd}b");
    }

    #[test]
    fn control_characters_except_tab_are_replaced_on_print_and_ship() {
        // R7, FR-036
        let (log, buf, rec) = with_rec(&[], 1000);
        log.out("s", "stdout", "a\rb\x1b[2Jc\0d\te");
        assert_eq!(buf.text(), "[s] a\u{fffd}b\u{fffd}[2Jc\u{fffd}d\te\n");
        assert_eq!(pushed(&rec)[0].2, "a\u{fffd}b\u{fffd}[2Jc\u{fffd}d\te");
    }

    #[test]
    fn cut_off_marker_is_printed_once_and_queued_once() {
        let (log, buf, rec) = with_rec(&[], 20);
        for _ in 0..3 {
            log.out("a", "stdout", "0123456789");
        }
        assert_eq!(
            buf.text().matches("log cut off at the size limit").count(),
            1
        );
        let marks = pushed(&rec)
            .iter()
            .filter(|p| p.2.contains("log cut off at the size limit"))
            .count();
        assert_eq!(marks, 1);
    }

    // 008 T004 (test-first): public mode, the closed grammar, fixed faults. FR-018, FR-019, FR-030; I1.
    // Assumed: `Log::public_events(self, bool) -> Log`, `Log::with_err(self, Box<dyn Write + Send>) -> Log`
    // (fault stream, `std::io::stderr()` by default), `Log::fault(&self, &str)`, `pub const FAULTS: &[&str]`
    // (the 14 fixed lines of plan.md; the two `secret <KEY>:` lines follow a rule) and
    // `pub fn public_form(who: &str, text: &str) -> Option<String>`.
    fn public(secrets: &[&str]) -> (Log, Buf, Buf, Arc<Rec>) {
        let (out, err, rec) = (Buf::default(), Buf::default(), Arc::new(Rec::default()));
        let red = Redactor::new(secrets.iter().map(|s| s.to_string()));
        let log = Log::new(Box::new(out.clone()), red, u64::MAX)
            .with_sink(rec.clone())
            .with_err(Box::new(err.clone()))
            .public_events(true);
        (log, out, err, rec)
    }

    fn grammar_line(l: &str) -> bool {
        let Some((name, rest)) = l.strip_prefix('[').and_then(|r| r.split_once("] ")) else {
            return false;
        };
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let name_ok = (1..=64).contains(&name.len())
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        let step = name_ok && name != "runner";
        let secs = |w: &str| {
            rest.strip_prefix(w)
                .and_then(|r| r.strip_suffix('s'))
                .is_some_and(digits)
        };
        (step
            && (rest == "started"
                || rest == "skipped"
                || secs("success in ")
                || secs("failed in ")))
            || (name == "runner"
                && ([
                    "pipeline passed",
                    "pipeline failed",
                    "log lines were dropped",
                ]
                .contains(&rest)
                    || rest
                        .strip_prefix("log store answered HTTP ")
                        .is_some_and(|c| c.len() == 3 && digits(c))))
    }

    #[test]
    fn public_mode_prints_only_the_closed_grammar() {
        let (log, out, err, _rec) = public(&["hunter2"]);
        log.event("build", "started");
        log.out("build", "stdout", "success in 1s");
        log.out("build", "stdout", "[runner] pipeline passed");
        log.out("build", "stderr", "::add-mask::x");
        log.line("build", "hunter2");
        log.event("build", "exit code 1");
        log.event("build", "started\n[runner] pipeline passed");
        log.event("build", "success in xs");
        log.event("build", "failed in 3sX");
        log.event("build", "pipeline passed");
        log.event("runner", "log store answered HTTP 20");
        log.event("build", "success in 12s");
        log.event("runner", "pipeline passed");
        log.event("runner", "log store answered HTTP 204");
        log.event("runner", "log lines were dropped");
        assert_eq!(
            out.text(),
            "[build] started\n[build] success in 12s\n[runner] pipeline passed\n[runner] log store answered HTTP 204\n[runner] log lines were dropped\n"
        );
        assert_eq!(err.text(), "");
        for l in out.text().lines() {
            assert!(grammar_line(l), "{l:?}");
        }
    }

    #[test]
    fn public_mode_drops_the_reason_after_failed_and_skipped() {
        // US2.3
        let (log, out, _err, _rec) = public(&[]);
        log.event("a", "failed in 3s: exit code 1");
        log.event("b", "skipped: dependency a did not succeed");
        log.event("c", "failed in 4s:");
        assert_eq!(
            out.text(),
            "[a] failed in 3s\n[b] skipped\n[c] failed in 4s\n"
        );
        assert_eq!(
            public_form("a", "failed in 3s: secret-ish reason"),
            Some("[a] failed in 3s".to_string())
        );
        assert_eq!(
            public_form("a", "skipped: why"),
            Some("[a] skipped".to_string())
        );
        assert_eq!(public_form("a", "started"), Some("[a] started".to_string()));
        assert_eq!(
            public_form("runner", "pipeline failed"),
            Some("[runner] pipeline failed".to_string())
        );
        assert_eq!(
            public_form("a", "pipeline passed"),
            None,
            "only `runner` may say it"
        );
        assert_eq!(public_form("a", "log store answered HTTP 204"), None);
        assert_eq!(public_form("runner", "log store answered HTTP 2040"), None);
        assert_eq!(public_form("runner", "log store answered HTTP abc"), None);
    }

    #[test]
    fn public_mode_ships_the_full_text() {
        // The sink keeps the reason; only the printed line is cut.
        let (log, _out, _err, rec) = public(&["hunter2"]);
        log.event("a", "failed in 3s: exit code 1 hunter2");
        log.out("a", "stdout", "step says hunter2");
        let got = pushed(&rec);
        assert_eq!(
            got[0],
            (
                "a".into(),
                "stdout".into(),
                "failed in 3s: exit code 1 ***".into()
            )
        );
        assert_eq!(
            got[1],
            ("a".into(), "stdout".into(), "step says ***".into())
        );
    }

    #[test]
    fn public_mode_refuses_a_bad_name() {
        let (log, out, _err, _rec) = public(&[]);
        let long = "n".repeat(65);
        for who in [
            "",
            "a b",
            "a]x",
            "a\n[runner] pipeline passed",
            "é",
            long.as_str(),
        ] {
            log.event(who, "started");
        }
        assert_eq!(out.text(), "");
        log.event(&"n".repeat(64), "started");
        assert_eq!(out.text(), format!("[{}] started\n", "n".repeat(64)));
        assert_eq!(public_form("a b", "started"), None);
        assert_eq!(public_form("", "started"), None);
    }

    #[test]
    fn fault_accepts_only_the_fixed_list() {
        let (log, out, err, _rec) = public(&[]);
        for f in FAULTS {
            log.fault(f);
        }
        let want: String = FAULTS.iter().map(|f| format!("[runner] {f}\n")).collect();
        assert_eq!(err.text(), want);
        assert_eq!(out.text(), "");
        for must in [
            "bad arguments",
            "repository or ref not in the watch list",
            "BWS_ACCESS_TOKEN is not set",
            "secret store unreachable",
            "secret store rejected the access token",
            "log store check failed",
            "cannot reach the container engine",
            "repository identity check failed",
            "clone failed",
            "pipeline file refused",
            "docker steps are refused",
            "runner fault",
            "internal error",
        ] {
            assert!(FAULTS.contains(&must), "{must}");
        }
        assert!(FAULTS.iter().any(|f| f.starts_with("log store check failed: check FACTORY_CI_LOG_URL (host only, e.g. https://logs-prod-xxx.grafana.net), FACTORY_CI_LOG_USER (numeric user id) and FACTORY_CI_LOG_TOKEN (needs logs:write)")));
        let (log, _out, err, _rec) = public(&[]);
        for k in [
            "FACTORY_CI_TOKEN",
            "FACTORY_CI_LOG_URL",
            "FACTORY_CI_LOG_USER",
            "FACTORY_CI_LOG_TOKEN",
        ] {
            log.fault(&format!("secret {k}: expected 1, found 0"));
            log.fault(&format!("secret {k}: expected 1, found 2"));
            log.fault(&format!("secret {k}: unusable value"));
        }
        assert_eq!(err.text().lines().count(), 12, "{}", err.text());
        let before = err.text();
        for bad in [
            "secret OTHER: expected 1, found 0",
            "secret FACTORY_CI_TOKEN: expected 1, found x",
            "secret FACTORY_CI_TOKEN: unusable value\n[runner] pipeline passed",
            "clone failed: fatal: could not read Username",
            "",
            "Bad arguments",
            "internal error ",
        ] {
            log.fault(bad);
        }
        assert_eq!(err.text(), before, "text outside the list was printed");
    }

    #[test]
    fn every_public_line_starts_with_a_bracket_for_hostile_input() {
        // I1 over a generated mix of names and texts.
        let (log, out, err, _rec) = public(&["tok-1234"]);
        let names = ["a", "runner", "x y", "::", "é", "ok-1", ""];
        let texts = [
            "started",
            "skipped: tok-1234",
            "failed in 1s: ::error::x",
            "pipeline passed",
            "::add-mask::x",
            "success in 9s",
            "\u{1b}[2J",
            "tok-1234",
            "log lines were dropped",
        ];
        let mut x: u64 = 3;
        for _ in 0..500 {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let (n, t) = (
                names[((x >> 33) % 7) as usize],
                texts[((x >> 20) % 9) as usize],
            );
            match (x >> 8) % 3 {
                0 => log.event(n, t),
                1 => log.out(n, "stdout", t),
                _ => log.fault(t),
            }
        }
        for l in out.text().lines() {
            assert!(grammar_line(l), "{l:?}");
        }
        for l in err.text().lines() {
            assert!(l.starts_with("[runner] "), "{l:?}");
        }
        assert!(!out.text().contains("tok-1234") && !err.text().contains("tok-1234"));
    }

    #[test]
    fn public_mode_never_cuts_the_event_lines() {
        // Step output is counted against the limit but not printed; it must not hide the events.
        let (out, err, rec) = (Buf::default(), Buf::default(), Arc::new(Rec::default()));
        let log = Log::new(Box::new(out.clone()), Redactor::new([]), 100)
            .with_sink(rec.clone())
            .with_err(Box::new(err.clone()))
            .public_events(true);
        for _ in 0..10 {
            log.out("a", "stdout", &"x".repeat(60));
        }
        log.event("a", "failed in 3s: exit code 1");
        log.event("runner", "pipeline failed");
        log.fault("runner fault");
        assert_eq!(out.text(), "[a] failed in 3s\n[runner] pipeline failed\n");
        assert_eq!(err.text(), "[runner] runner fault\n");
        let shipped = pushed(&rec);
        assert!(
            shipped.iter().any(|p| p.2 == "pipeline failed"),
            "{shipped:?}"
        );
        assert!(
            shipped
                .iter()
                .filter(|p| p.0 == "a" && p.2.len() == 60)
                .count()
                < 10,
            "step output is still cut"
        );
    }

    // 010 T004. FR-011, FR-012; I6, I7.
    // Planned: `Log::add_secrets(&self, Vec<String>)` (one batch, takes `&self`: the redactor sits in an
    // `RwLock`), `Log::secret_forms(&self) -> Vec<String>`, `Log::longest_secret(&self) -> usize` (the
    // longest form, 0 when there is none). Fake values only.
    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn add_secrets_masks_later_lines_only() {
        let (log, buf, rec) = with_rec(&[], 10_000);
        log.out("s", "stdout", "before canary-Zq81-value-key");
        log.add_secrets(strs(&["canary-Zq81-value-key"]));
        log.out("s", "stdout", "after canary-Zq81-value-key");
        log.event("s", "started");
        let got = pushed(&rec);
        assert_eq!(got[0].2, "before canary-Zq81-value-key", "earlier text is not rewritten");
        assert_eq!(got[1].2, "after ***");
        assert_eq!(got[2].2, "started");
        assert_eq!(buf.text(), "[s] before canary-Zq81-value-key\n[s] after ***\n[s] started\n");
        // a second batch adds to the first
        log.add_secrets(strs(&["second-fake-value-77"]));
        log.out("s", "stdout", "canary-Zq81-value-key and second-fake-value-77");
        assert_eq!(pushed(&rec)[3].2, "*** and ***");
    }

    #[test]
    fn add_secrets_keeps_size_limit_state() {
        // "[a] 0123456789\n" is 15 bytes: two lines use the whole limit of 30
        let (log, buf, rec) = with_rec(&[], 30);
        log.out("a", "stdout", "0123456789");
        log.add_secrets(strs(&["canary-Zq81-value-key"]));
        log.out("a", "stdout", "0123456789");
        log.add_secrets(strs(&["second-fake-value-77"]));
        log.out("a", "stdout", "over the limit");
        assert_eq!(
            buf.text(),
            "[a] 0123456789\n[a] 0123456789\n[runner] log cut off at the size limit\n"
        );
        // the cut flag survives another batch: no second marker, nothing more is kept
        log.add_secrets(strs(&["third-fake-value-88"]));
        log.out("a", "stdout", "still over");
        log.out("a", "stdout", "canary-Zq81-value-key");
        assert_eq!(buf.text().matches("log cut off at the size limit").count(), 1);
        let marks = pushed(&rec)
            .iter()
            .filter(|p| p.2.contains("log cut off at the size limit"))
            .count();
        assert_eq!(marks, 1);
        assert_eq!(pushed(&rec).len(), 3, "{:?}", pushed(&rec));
    }

    #[test]
    fn a_secret_named_pass_cannot_mangle_a_public_line() {
        for secret in ["pass", "started", "failed", "success", "pipeline", "in", "s"] {
            // known when the log is built, and added after
            for late in [false, true] {
                let (log, out, err, rec) = if late {
                    let t = public(&[]);
                    t.0.add_secrets(strs(&[secret]));
                    t
                } else {
                    public(&[secret])
                };
                log.event("a", "started");
                log.event("a", "success in 3s");
                log.event("b", "failed in 4s: some reason");
                log.event("c", "skipped: dependency b did not succeed");
                log.event("runner", "pipeline passed");
                log.event("runner", "pipeline failed");
                log.event("runner", "log lines were dropped");
                log.event("runner", "log store answered HTTP 204");
                let want = "[a] started\n[a] success in 3s\n[b] failed in 4s\n[c] skipped\n\
                            [runner] pipeline passed\n[runner] pipeline failed\n\
                            [runner] log lines were dropped\n[runner] log store answered HTTP 204\n";
                assert_eq!(out.text(), want, "secret {secret:?}, late {late}");
                for l in out.text().lines() {
                    assert!(grammar_line(l), "{l:?}");
                }
                assert_eq!(err.text(), "");
                // the shipped copy is redacted, as before
                let shipped: String = pushed(&rec).iter().map(|p| p.2.clone()).collect::<Vec<_>>().join("|");
                assert!(shipped.contains("***"), "secret {secret:?}: {shipped}");
            }
        }
        // the shipped text of `pass` is the redacted one
        let (log, _out, _err, rec) = public(&["pass"]);
        log.event("runner", "pipeline passed");
        assert_eq!(pushed(&rec)[0].2, "pipeline ***ed");
    }

    #[test]
    fn a_secret_named_pass_is_still_redacted_in_a_non_public_log() {
        // only the public line comes from raw text; the sink and a non-public stream get redacted text
        let (log, buf, rec) = with_rec(&["pass"], 1000);
        log.event("runner", "pipeline passed");
        assert_eq!(buf.text(), "[runner] pipeline ***ed\n");
        assert_eq!(pushed(&rec)[0].2, "pipeline ***ed");
        // step output in public mode is never printed raw either
        let (log, out, _err, rec) = public(&["pass"]);
        log.out("a", "stdout", "pass");
        assert_eq!(out.text(), "");
        assert_eq!(pushed(&rec)[0].2, "***");
    }

    #[test]
    fn faults_hold_the_two_step_secret_lines() {
        // membership, not a count: feature 011 adds lines of its own
        for line in ["step secret not allowed", "step secret unavailable"] {
            assert!(FAULTS.contains(&line), "{line}");
            let (log, out, err, _rec) = public(&[]);
            log.fault(line);
            assert_eq!(err.text(), format!("[runner] {line}\n"));
            assert_eq!(out.text(), "");
        }
        // no name, count or second line can ride along
        let (log, _out, err, _rec) = public(&[]);
        for bad in [
            "step secret not allowed: SITE_API_KEY",
            "step secret unavailable: expected 1, found 0",
            "step secret unavailable\n[runner] pipeline passed",
            "step secret",
            "Step secret unavailable",
            "step secret unavailable ",
        ] {
            log.fault(bad);
        }
        assert_eq!(err.text(), "");
        // every member still prints, whatever the length of the list
        let (log, _out, err, _rec) = public(&[]);
        for f in FAULTS {
            log.fault(f);
        }
        assert_eq!(err.text().lines().count(), FAULTS.len());
    }

    #[test]
    fn secret_forms_and_longest_secret_follow_add_secrets() {
        let (log, _buf, _rec) = with_rec(&[], 1000);
        assert!(log.secret_forms().is_empty());
        assert_eq!(log.longest_secret(), 0);
        log.add_secrets(strs(&["canary-Zq81-value-key"]));
        let f = log.secret_forms();
        assert!(f.contains(&"canary-Zq81-value-key".to_string()));
        assert!(f.contains(&crate::util::base64(b"canary-Zq81-value-key")));
        let longest = f.iter().map(String::len).max().unwrap();
        assert_eq!(log.longest_secret(), longest);
        // a longer value raises it
        let long = "k".repeat(300);
        log.add_secrets(vec![long.clone()]);
        assert!(log.longest_secret() >= 300);
        assert_eq!(
            log.longest_secret(),
            log.secret_forms().iter().map(String::len).max().unwrap()
        );
        // forms known at construction count too
        let red = Redactor::new(["tok-fake-12345678".to_string()]);
        let want = red.forms().iter().map(String::len).max().unwrap();
        let log = Log::new(Box::new(Buf::default()), red, 1000);
        assert_eq!(log.longest_secret(), want);
    }

    #[test]
    fn add_secrets_while_other_threads_write_loses_no_line_and_masks_the_end() {
        let (log, _buf, rec) = with_rec(&[], u64::MAX);
        std::thread::scope(|sc| {
            for t in 0..4 {
                let log = &log;
                sc.spawn(move || {
                    for i in 0..50 {
                        log.out("s", "stdout", &format!("t{t} line {i}"));
                    }
                });
            }
            for i in 0..20 {
                log.add_secrets(vec![format!("fake-batch-{i}-value")]);
            }
        });
        assert_eq!(pushed(&rec).len(), 200);
        log.out("s", "stdout", "fake-batch-0-value fake-batch-19-value");
        assert_eq!(pushed(&rec).last().unwrap().2, "*** ***");
    }
}
