# Contract: Loki push

`POST {FACTORY_CI_LOG_URL}/loki/api/v1/push`
`Content-Type: application/json`, `Authorization: Basic base64(user:token)`.

```json
{"streams":[
  {"stream":{"repo":"o/r","project":"go-invoices","step":"build","stream":"stdout"},
   "values":[["1764115200123456789","run=0123456789abcdef sha=1a2b3c4d branch=main compiling..."]]}
]}
```

- Exactly the four labels, plus `service_name="factory-ci"` since 009. Values: `repo` from the item, `project` (`.` for root), `step` (a step
  name, or `runner`), `stream` (`stdout` or `stderr`).
- One stream entry per label set per batch; values sorted by timestamp.
- A batch is sent when 1 s has passed since its first line or its text reaches 1 MB.
- Response handling: 2xx delivered. 429, 5xx, transport error: retry with backoff 1 2 4 8 16 s
  until 60 s since the first attempt, then drop. 401, 403: stop shipping for the job, drop. Other
  4xx: drop, no retry.
- The URL, user and token never appear in any log line, status, summary or error text.

## Heartbeat

After `plan` phase C posts its statuses it sends one line with labels `repo="factory-ci"`, `project="."`,
`step="heartbeat"`, `stream="stdout"` and text `run=<id> tick items=<N> problems=<M>`, when the log credentials
resolved. It uses the same retry rules with a 10-second flush window and ignores failure. Do not alert on the absence
of this line: idle ticks make no store call and so send none (see the note below).

The dead-man signal is a second, independent channel: a plain `GET` to `FACTORY_CI_HEARTBEAT_URL` from the `verdict` job on every tick. Either, both
or neither can be configured (FR-056). See `cli.md`.

Amended by `research.md` D12: the Loki line is sent from `plan` phase C only on ticks that have candidate rows,
because the log credentials come from the secret store and an idle tick makes no store call. It is therefore a
"tick with work" marker, not a dead-man signal. The dead-man alert uses the ping, sent by the `verdict` job on every
tick (FR-056). For a private repo the runner sends step names and events here and never to the Actions log (FR-059).
