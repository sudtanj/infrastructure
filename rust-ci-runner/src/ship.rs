use crate::log::Sink;
use crate::util::{base64, short_sha};
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const URL_VAR: &str = "FACTORY_CI_LOG_URL";
const USER_VAR: &str = "FACTORY_CI_LOG_USER";
const TOKEN_VAR: &str = "FACTORY_CI_LOG_TOKEN";
const LINK_VAR: &str = "FACTORY_CI_LOG_LINK";

/// How long `finish` waits for the queue to drain (FR-034).
pub const FLUSH_WINDOW: Duration = Duration::from_secs(60);

/// The log store settings (data-model.md LogConfig). Every field except `link` is secret.
#[derive(Clone)]
pub struct LogConfig {
    pub url: String,
    pub user: String,
    pub token: String,
    pub link: Option<String>,
}

fn web_url(s: &str) -> bool {
    s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")).is_some_and(|r| !r.is_empty())
}

impl LogConfig {
    /// `None` when none of the three variables is set. Messages name variables, never values.
    pub fn from_env(get: &dyn Fn(&str) -> Option<String>) -> Result<Option<LogConfig>, Vec<String>> {
        let var = |k: &str| get(k).filter(|v| !v.is_empty());
        let (url, user, token) = (var(URL_VAR), var(USER_VAR), var(TOKEN_VAR));
        let missing: Vec<&str> = [(URL_VAR, &url), (USER_VAR, &user), (TOKEN_VAR, &token)]
            .into_iter()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k)
            .collect();
        let (Some(url), Some(user), Some(token)) = (url, user, token) else {
            if missing.len() == 3 {
                return Ok(None);
            }
            return Err(vec![format!("FACTORY_CI_LOG_* must be all set or all unset; missing {}", missing.join(", "))]);
        };
        let mut bad = vec![];
        let url = url.trim_end_matches('/').to_string();
        let authority = url.split_once("://").map_or("", |(_, r)| r.split(['/', '?', '#']).next().unwrap_or(""));
        // shortcut: plain http only to the runner's own machine (tests run a fake store there).
        let local = ["127.0.0.1", "localhost", "[::1]"]
            .iter()
            .any(|h| authority.strip_prefix(h).is_some_and(|r| r.is_empty() || r.starts_with(':')));
        let secure = url.starts_with("https://") || (url.starts_with("http://") && local);
        if !secure || authority.contains('@') {
            bad.push(format!("{URL_VAR}: log URL must be https, without a user name or password"));
        }
        let link = var(LINK_VAR);
        if link.as_deref().is_some_and(|l| !web_url(l) || !l.contains("{run_id}")) {
            bad.push(format!("{LINK_VAR} must start with http:// or https:// and contain {{run_id}}"));
        }
        if bad.is_empty() {
            Ok(Some(LogConfig { url, user, token, link }))
        } else {
            Err(bad)
        }
    }
}

/// The status link when shipping is on and a template is set (FR-031).
pub fn link(template: &str, run_id: &str) -> String {
    template.replace("{run_id}", run_id)
}

/// The Actions run, the link when shipping is off.
pub fn actions_url(get: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let var = |k: &str| get(k).filter(|v| !v.is_empty());
    let (s, r, i) = (var("GITHUB_SERVER_URL")?, var("GITHUB_REPOSITORY")?, var("GITHUB_RUN_ID")?);
    Some(format!("{}/{r}/actions/runs/{i}", s.trim_end_matches('/')))
}

/// Sends one heartbeat line (`loki-push.md`, FR-056) with its own shipper and waits up to 10 s.
/// `Err` carries fixed text; the caller ignores it.
pub fn send_heartbeat(cfg: &LogConfig, run_id: &str, text: &str) -> Result<(), String> {
    let labels = Labels { repo: "factory-ci".to_string(), project: ".".to_string() };
    let meta = Meta { run_id: run_id.to_string(), sha: String::new(), branch: String::new() };
    let s = Shipper::start(cfg.clone(), labels, meta);
    s.push("heartbeat", "stdout", text);
    if s.finish(Duration::from_secs(10)) {
        Ok(())
    } else {
        Err("heartbeat not delivered".to_string())
    }
}

/// Sent on every stream: Grafana Logs Drilldown groups logs by `service_name`.
pub const SERVICE_NAME: &str = "factory-ci";

/// The two labels that do not change during a run.
pub struct Labels {
    pub repo: String,
    pub project: String,
}

/// Goes in the line prefix, not in labels (FR-033).
pub struct Meta {
    pub run_id: String,
    pub sha: String,
    pub branch: String,
}

#[derive(Clone)]
pub struct Timing {
    pub batch: Duration,
    pub retry_window: Duration,
    /// First retry delay; it doubles up to 16 times this.
    pub backoff: Duration,
    pub request_timeout: Duration,
    pub max_batch: usize,
    pub max_queue: usize,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing {
            batch: Duration::from_secs(1),
            retry_window: Duration::from_secs(60),
            backoff: Duration::from_secs(1),
            request_timeout: Duration::from_secs(10),
            max_batch: 1 << 20,
            max_queue: 32 << 20,
        }
    }
}

struct Line {
    ts_ns: u128,
    step: String,
    stream: String,
    text: String,
}

#[derive(Default)]
struct Queue {
    lines: VecDeque<Line>,
    bytes: usize,
    /// When the oldest queued line arrived.
    first: Option<Instant>,
    closing: bool,
    done: bool,
}

/// Ships lines to Loki from one thread. Nothing here can fail a step: a lost line only sets `dropped`.
pub struct Shipper {
    cfg: LogConfig,
    labels: Labels,
    meta: Meta,
    t: Timing,
    q: Mutex<Queue>,
    cv: Condvar,
    dropped: AtomicBool,
    stopped: AtomicBool,
}

impl Shipper {
    pub fn start(cfg: LogConfig, labels: Labels, meta: Meta) -> Arc<Shipper> {
        Shipper::start_with(cfg, labels, meta, Timing::default())
    }

    pub fn start_with(cfg: LogConfig, labels: Labels, meta: Meta, t: Timing) -> Arc<Shipper> {
        let s = Arc::new(Shipper {
            cfg,
            labels,
            meta,
            t,
            q: Mutex::default(),
            cv: Condvar::new(),
            dropped: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
        });
        let worker = s.clone();
        std::thread::spawn(move || worker.run());
        s
    }

    pub fn dropped(&self) -> bool {
        self.dropped.load(Ordering::Relaxed)
    }

    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.q.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Waits up to `window` for the queue to empty. True when every line was delivered.
    pub fn finish(&self, window: Duration) -> bool {
        let end = Instant::now() + window;
        let mut g = self.lock();
        g.closing = true;
        self.cv.notify_all();
        while !g.done {
            let Some(left) = end.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()) else {
                self.dropped.store(true, Ordering::Relaxed);
                return false;
            };
            g = self.cv.wait_timeout(g, left).unwrap_or_else(|e| e.into_inner()).0;
        }
        !self.dropped()
    }

    fn run(&self) {
        loop {
            let batch = {
                let mut g = self.lock();
                loop {
                    let due = g.first.is_some_and(|f| f.elapsed() >= self.t.batch);
                    if !g.lines.is_empty() && (due || g.closing || g.bytes >= self.t.max_batch) {
                        break;
                    }
                    if g.lines.is_empty() && g.closing {
                        g.done = true;
                        self.cv.notify_all();
                        return;
                    }
                    let wait = g.first.map_or(self.t.batch, |f| self.t.batch.saturating_sub(f.elapsed()));
                    g = self.cv.wait_timeout(g, wait.max(Duration::from_millis(1))).unwrap_or_else(|e| e.into_inner()).0;
                }
                let mut batch = vec![];
                let mut size = 0;
                while let Some(l) = g.lines.pop_front() {
                    size += l.text.len();
                    g.bytes -= l.text.len();
                    batch.push(l);
                    if size >= self.t.max_batch {
                        break;
                    }
                }
                g.first = (!g.lines.is_empty()).then(Instant::now);
                batch
            };
            if !self.send(batch) {
                // 401 or 403: nothing more goes out for this job.
                let mut g = self.lock();
                self.dropped.store(true, Ordering::Relaxed);
                g.lines.clear();
                g.bytes = 0;
                g.done = true;
                self.cv.notify_all();
                return;
            }
        }
    }

    fn body(&self, mut batch: Vec<Line>) -> String {
        batch.sort_by_key(|l| l.ts_ns);
        let m = &self.meta;
        let prefix = format!(
            "run={} sha={} branch={} ",
            m.run_id,
            short_sha(&m.sha),
            m.branch.chars().take(40).collect::<String>()
        );
        let mut streams: BTreeMap<(String, String), Vec<[String; 2]>> = BTreeMap::new();
        for l in batch {
            let text = format!("{prefix}{}", l.text.replace('\0', "\u{fffd}"));
            streams.entry((l.step, l.stream)).or_default().push([l.ts_ns.to_string(), text]);
        }
        let streams: Vec<_> = streams
            .into_iter()
            .map(|((step, stream), values)| {
                serde_json::json!({
                    "stream": {"service_name": SERVICE_NAME, "repo": self.labels.repo, "project": self.labels.project, "step": step, "stream": stream},
                    "values": values,
                })
            })
            .collect();
        serde_json::json!({ "streams": streams }).to_string()
    }

    /// Posts one batch with retries. False only when the store refused our credentials.
    fn send(&self, batch: Vec<Line>) -> bool {
        let body = self.body(batch);
        let auth = format!("Basic {}", base64(format!("{}:{}", self.cfg.user, self.cfg.token).as_bytes()));
        let agent = ureq::AgentBuilder::new().timeout(self.t.request_timeout).redirects(0).build();
        let url = format!("{}/loki/api/v1/push", self.cfg.url);
        let start = Instant::now();
        let mut wait = self.t.backoff;
        loop {
            let res = agent.post(&url).set("Authorization", &auth).set("Content-Type", "application/json").send_string(&body);
            match res {
                Ok(r) if (200..300).contains(&r.status()) => return true,
                Err(ureq::Error::Status(401 | 403, _)) => {
                    self.stopped.store(true, Ordering::Relaxed);
                    return false;
                }
                // 429, 5xx and transport errors are retried; any other answer drops the batch.
                Err(ureq::Error::Status(c, _)) if c != 429 && c < 500 => break,
                Err(ureq::Error::Transport(_)) | Err(ureq::Error::Status(..)) => {}
                Ok(_) => break,
            }
            let left = self.t.retry_window.saturating_sub(start.elapsed());
            if left.is_zero() {
                break;
            }
            std::thread::sleep(wait.min(left));
            wait = (wait * 2).min(self.t.backoff * 16);
        }
        self.dropped.store(true, Ordering::Relaxed);
        true
    }
}

impl Sink for Shipper {
    fn push(&self, who: &str, stream: &str, text: &str) {
        if self.stopped.load(Ordering::Relaxed) {
            self.dropped.store(true, Ordering::Relaxed);
            return;
        }
        let ts_ns = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let mut g = self.lock();
        if g.closing || g.bytes + text.len() > self.t.max_queue {
            self.dropped.store(true, Ordering::Relaxed);
            return;
        }
        g.bytes += text.len();
        g.first.get_or_insert_with(Instant::now);
        g.lines.push_back(Line { ts_ns, step: who.into(), stream: stream.into(), text: text.into() });
        self.cv.notify_all();
    }
}
