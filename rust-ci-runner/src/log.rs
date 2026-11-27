use crate::redact::Redactor;
use std::io::Write;
use std::sync::{Arc, Mutex};

/// A line is shipped in pieces of at most this many bytes (FR-036).
const MAX_PIECE: usize = 16 * 1024;
const CUT_TEXT: &str = "log cut off at the size limit";

/// Receives every redacted piece the log keeps, for shipping to a log store.
pub trait Sink: Send + Sync {
    fn push(&self, who: &str, stream: &str, text: &str);
}

/// The run log. Every line is prefixed with its source (step name or `runner`), passed
/// through the redactor, and cut off at the size limit.
pub struct Log {
    inner: Mutex<Inner>,
    red: Redactor,
    sink: Option<Arc<dyn Sink>>,
    quiet: bool,
}

struct Inner {
    out: Box<dyn Write + Send>,
    left: u64,
    cut: bool,
}

impl Log {
    pub fn new(out: Box<dyn Write + Send>, red: Redactor, limit: u64) -> Log {
        Log {
            inner: Mutex::new(Inner {
                out,
                left: limit,
                cut: false,
            }),
            red,
            sink: None,
            quiet: false,
        }
    }

    pub fn with_sink(mut self, sink: Arc<dyn Sink>) -> Log {
        self.sink = Some(sink);
        self
    }

    /// Quiet: `out` text is not printed (still shipped); `event` lines are.
    pub fn quiet(mut self, quiet: bool) -> Log {
        self.quiet = quiet;
        self
    }

    pub fn line(&self, who: &str, text: &str) {
        self.out(who, "stdout", text);
    }

    /// Step or clone output: printed unless quiet, shipped with its stream.
    pub fn out(&self, who: &str, stream: &str, text: &str) {
        self.write(who, stream, text, !self.quiet);
    }

    /// A runner event (started, result, duration): always printed. Callers pass fixed text.
    pub fn event(&self, who: &str, text: &str) {
        self.write(who, "stdout", text, true);
    }

    fn write(&self, who: &str, stream: &str, text: &str, print: bool) {
        // Control characters other than tab could rewrite a terminal line or forge a log line (R7).
        let text = self.red.apply(text.trim_end_matches(['\r', '\n']));
        let bad = |c: char| c.is_control() && c != '\t';
        let text: String = text.chars().map(|c| if bad(c) { '\u{fffd}' } else { c }).collect();
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
            if !self.emit(&mut g, who, stream, piece, print) || tail.is_empty() {
                return;
            }
            rest = tail;
        }
    }

    /// False once the size limit is hit; the cut-off marker is printed and queued once.
    fn emit(&self, g: &mut Inner, who: &str, stream: &str, piece: &str, print: bool) -> bool {
        if g.cut {
            return false;
        }
        let msg = format!("[{who}] {piece}\n");
        // shortcut: write errors (disk full) are dropped; the run result is still kept.
        if msg.len() as u64 > g.left {
            g.cut = true;
            let _ = writeln!(g.out, "[runner] {CUT_TEXT}");
            if let Some(s) = &self.sink {
                s.push("runner", "stdout", CUT_TEXT);
            }
            let _ = g.out.flush();
            return false;
        }
        g.left -= msg.len() as u64;
        if print {
            let _ = g.out.write_all(msg.as_bytes());
            let _ = g.out.flush();
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
        assert_eq!(pushed(&rec), [("build".into(), "stderr".into(), "oops".into())]);
    }

    #[test]
    fn sink_gets_redacted_text_only() {
        // I7
        let (log, buf, rec) = with_rec(&["hunter2"], 1000);
        log.out("s", "stdout", "pw=hunter2");
        log.event("s", "started");
        let all: String = pushed(&rec).iter().map(|p| p.2.clone()).collect::<Vec<_>>().join("|");
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
        assert_eq!(buf.text().matches("log cut off at the size limit").count(), 1);
        let marks = pushed(&rec)
            .iter()
            .filter(|p| p.2.contains("log cut off at the size limit"))
            .count();
        assert_eq!(marks, 1);
    }
}
