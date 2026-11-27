# Contract: watch list file

Path `.github/factory-ci-repos.toml` in B. Read by `plan --repos`. B is public, so so is this file.

```toml
# One table per watched repo. Unknown keys are errors.
[[watch]]
repo = "sudtanj/invoices"          # required, owner/name
id = 123456789                     # optional numeric repo id; required to use owner/name<SEP>NAME keys
branches = ["main", "release/*"]   # optional, default: every branch
secret_branches = ["main"]         # optional, default: the repo's default branch; other branches get no secrets
secrets = ["NPM_TOKEN"]            # optional shared key names in the default project; default: none
allow_docker = false               # optional, default false
```

Rules: see `data-model.md WatchEntry`. Validation collects every problem and reports all of them, one per line, as
`<repo or entry N>: <problem>`. Examples:

- `entry 2: unknown key "branch"` (and `bitwarden_project`: there are no per-repo projects)
- `sudtanj/app: duplicate of entry 1`
- `sudtanj/app: secret FACTORY_CI_LOG_TOKEN is reserved`
- `sudtanj/app: malformed glob "feat[" (unsupported character [)`
- `entry 3: repo "a/b/c" is not owner/name`
- `entry 4: repo "../x" is not owner/name` (also `*`, `[a-z]`, `a b`, `a/b/c`; `a/b` is accepted)
- `sudtanj/app: id "abc" is not a number`

The file is read from B's checkout at the dispatched ref. Removing an entry stops all contact with that repo; its old
statuses stay.

`secrets` is not a gate on what a pipeline may read from its own repo. A pipeline lists the names it needs in its
steps. Those names come from `owner/name<SEP>NAME` in the default project (the docker project for `allow_docker`
repos), with no list entry, provided the entry records `id`. `secrets` only adds shared keys (`NAME` in the default
project). See `data-model.md` Secret names and sources.

`id`: `plan` calls `GET /repos/{owner}/{name}` for each entry. When `id` is recorded and the API answers a different
id (a rename plus a new repo under the old name), the repo is skipped with a problem naming both ids. Without `id` the
prefixed keys are not used for that entry; the shared names still work. The repo name pattern is
`^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$` without `..`.

`secret_branches`: the same glob rules as `branches`. An item on a branch that does not match gets no secrets: a step
that lists one ends in `error` ("secrets not allowed on this branch"). Steps that list none run. The default branch
comes from the same repo lookup.
