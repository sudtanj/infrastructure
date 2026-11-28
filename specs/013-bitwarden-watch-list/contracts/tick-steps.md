# Contract: tick workflow changes (`factory-ci-tick.yml`)

## Trigger (T024, PR 3)

```yaml
on:
  workflow_dispatch:
  schedule:
    - cron: "*/5 * * * *"
```

## plan job: two new steps (T019, PR 2)

After `install bws`, before `plan`:

```yaml
      - name: fetch watch list
        env:
          BWS_ACCESS_TOKEN: ${{ secrets.BWS_ACCESS_TOKEN }}
          FACTORY_CI_BWS_PROJECT: ${{ vars.FACTORY_CI_BWS_PROJECT }}
          FACTORY_CI_BWS_SERVER_URL: ${{ vars.FACTORY_CI_BWS_SERVER_URL }}
        run: |
          set -eu
          umask 077
          rust-ci-runner watch-list --bws "$RUNNER_TEMP/bin/bws" --project "$FACTORY_CI_BWS_PROJECT" --out .github/factory-ci-repos.toml
```

After `plan`:

```yaml
      - name: remove watch list
        if: always()
        run: rm -f .github/factory-ci-repos.toml
```

Lint rules these steps satisfy (`factory-ci-lint-workflow.sh`): no `${{` inside a `run:` block; values reach the script through `env:`; no `if:` reads `secrets.`; the guard step still precedes the wrapper call; no new `uses:`; `run` and `run-docker` are untouched.

## Checks in the selftest (T016, T024)

1. `fetch watch list` after `install bws` and before `plan` in the `plan` job.
2. No `watch-list` word in `run`, `run-docker`, `verdict` or `factory-monorepo-ci.yml`.
3. No `upload-artifact` in the tick.
4. Exactly one `^  schedule:$`, exactly one `cron:`, equal to `    - cron: "*/5 * * * *"`; `workflow_dispatch:` present.
5. The lint and its `--self-test` run on a copy of `.github/workflows` whose tick lacks the two schedule lines.
