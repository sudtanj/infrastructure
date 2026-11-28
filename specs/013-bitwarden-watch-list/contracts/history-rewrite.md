# Runbook: scrub the private names from the public repo (T030 to T033)

**T033 is irreversible. It is a force-push of every ref of a public repo. The lead runs it only after a fresh, explicit owner go given after reading this file, and only after features 012, 011 and 013 have all merged and no other branch or open pull request remains. Nothing in this file is run by an agent on its own.**

This file never writes a private name. It derives every string with git so the list is exact and reproducible. After T030 the tree holds no private name, and `git filter-repo` would turn any name written here into its replacement anyway.

## Strings to scrub (derived, not typed)

Run in a fresh clone, at the last commit before the scrub commit (the old file still exists in history; `8660df5` is the commit this plan was written at):

| Id | What | How to get it |
|---|---|---|
| S1 | the `owner/name` of the private entry | `git show 8660df5:.github/factory-ci-repos.toml \| grep '^repo'`, the line that is not the infrastructure repo |
| S2 | the bare name of S1 (the part after `owner/`); it also appears alone and inside the file names of OLD_MANUAL | cut S1 at `/` |
| S3 | the numeric `id` of the private entry | the `id` line under S1 in the same file |
| S4 | the old file names | `git log --all --name-only --format= \| sort -u \| grep -i S2`: the manual workflow file and its shell test (two path renames) |
| S5 | other `owner/name` strings in history | `git log --all -p \| grep -oiE 'sudtanj/[A-Za-z0-9_.-]+' \| sort -u`, minus the public infrastructure repo and the public runner image; the owner marks each remaining one real or example. The old file comment used two example names; scrub them only if the owner says they are real repos |
| S6 | the Loki label form and any other quoting of S1 | covered by S1 and S2 as substrings |

Replacement text for S1 and S2: `PRIVATE-REPO`; for S3: `0`. The replace file is `replace.txt` on the owner's machine (not committed), one `literal==>replacement` per line, longest strings first (S1 before S2). Path renames: `OLD_MANUAL workflow path:NEW_MANUAL workflow path` and the same for the test script, with `NEW_MANUAL` as defined in spec.md.

## Tool

`git filter-repo` (not `filter-branch`). Replace in blobs with `--replace-text replace.txt`, in commit and tag messages with `--replace-message replace.txt`, and rename paths with `--path-rename`. If the installed version offers `--sensitive-data-removal`, use it too (unverified for this version; read `git filter-repo --help` first).

## Before the go (all must be true)

1. T030 (tree scrub and rename) and T031 (old run logs deleted) are done.
2. 012, 011 and 013 are merged; `gh pr list` and `git branch -r` show only `main`.
3. 012's publish step is idempotent for a re-run on identical content (read `publish.rs` and its tests); 011's claims are keyed by slot, not by SHA.
4. The owner has the private backup (step B1) and has listed every clone, worktree and CI cache that holds the old history.
5. The owner types an explicit go in a fresh message after reading the effects below.

## Steps

**B1 backup** (owner's machine, private disk, never a shared VM or a repo): `git clone --mirror git@github.com:<owner>/<repo>.git backup-<date>.git`; keep `git rev-list --all > old-shas-<date>.txt` next to it. This is the only way back.

**B2 pause** (owner or lead with owner rights): `gh workflow disable factory-ci-tick`; `gh workflow disable rust-ci-runner-release` (a moved tag would otherwise start a release run); `gh workflow disable factory-ci-selftest` if it triggers on pushes. Note the Environments' branch rule (`main`) needs no change.

**B3 allow the force-push**: temporarily lift the branch protection or ruleset on `main` and the tag rule on `rust-ci-runner-v*` (owner). Note both settings first so they can be restored exactly.

**B4 rewrite** (a fresh clone, as filter-repo requires): `git clone git@github.com:<owner>/<repo>.git work && cd work && git filter-repo --replace-text ../replace.txt --replace-message ../replace.txt --path-rename OLD:NEW ...`; filter-repo removes `origin`, so `git remote add origin git@github.com:<owner>/<repo>.git`.

**B5 verify before pushing** (the verification grep below, on `work`). Any hit stops the run; restore nothing yet (nothing is pushed).

**B6 push**: `git push --force --all origin` then `git push --force --tags origin`. This is the irreversible step.

**B7 restore**: re-enable branch protection and the tag rule exactly as noted; `gh workflow enable rust-ci-runner-release` (the moved tags already ran no release because the workflow was disabled), `gh workflow enable factory-ci-selftest`, then `gh workflow enable factory-ci-tick`.

**B8 first tick**: see "Effects on the tick". Then run the verification grep on a fresh `git clone --mirror` of the public repo.

**B9 GitHub Support**: send the request below only after B8.

**B10 discard** every old clone and worktree (the old history sits in them) except the private backup.

## Effects

- Every SHA in the repo changes. Tags `rust-ci-runner-v*` move to new commits; the release objects stay and point at the tags. Container image tags and digests are untouched, and the `RUNNER_IMAGE_DIGEST` and `RUNNER_IMAGE` pins stay valid because they name image digests, not git SHAs.
- Open pull requests and branches break (none may exist at the go). Every clone and fork has an unrelated history.
- GitHub keeps the old commits reachable by SHA, and through `refs/pull/*` and cached views, until GitHub Support purges them. Public forks keep their own copy; only their owners can delete them (`gh api repos/<owner>/<repo>/forks` lists them).
- The commit statuses the tick walk relies on vanish with the old SHAs: the new tip has none.
- The old workflow runs and their logs stay until deleted (T031). Workflow run history is keyed by workflow file and name, so deleting the runs is how the old workflow name leaves the Actions list.

## Effects on the tick (`walk.rs` `decide` and `walk_one`)

For each project at a watched tip, `decide` first asks for the project's own status on the tip. The new tip has none, so the project is `open`, `needs_row` is true and `decide` lists the commits and builds the first-parent chain of at most 50 ancestors. `walk_one` asks for a status of the project's context on each ancestor; every ancestor is new and has none, so it returns `Run { prev: None }`. Result: one run per project at each watched tip of the rewritten repo, once, with no `prev`. The run posts statuses on the new tip; the next tick sees `Done`. It does not run every commit, and it does not fail, because "no status anywhere" is the normal first-run case. Other watched repos are untouched. Expect one run for each project of the infrastructure repo's `main` (the runner project, the canary project and each image folder; for an image folder the publish step runs again, which is why its idempotence is a precondition). Feature 011's schedule claims are keyed by slot and are not affected.

## Verification grep (B5 and B8)

On a fresh `git clone --mirror` (B8) or the rewritten `work` clone (B5), with `strings.txt` holding S1, S2, S3 and each S5 the owner marked real, one per line:

- `git log --all --format='%H %an %ae %cn %ce %s%n%b' | grep -i -F -f strings.txt` finds nothing (messages and identities).
- `git rev-list --all --objects | cut -d' ' -f2- | grep -i -F -f strings.txt` finds nothing (paths).
- `for c in $(git rev-list --all); do git grep -I -i -F -f strings.txt "$c"; done` finds nothing (blobs; slow but this repo is small).
- `git for-each-ref --format='%(refname)' | grep -i -F -f strings.txt` finds nothing (branch and tag names).
- `gh api repos/<owner>/<repo>/pulls?state=all` shows no pull request whose title or body holds a string; `gh run list` for the old workflow is empty.

A hit in B5 stops the run before the push. A hit in B8 means the support request must name the surviving object.

## Draft request to GitHub Support (do not send before B8)

> Subject: Remove cached views of rewritten history, public repository `<owner>/<repo>`
>
> I rewrote the full history of `<owner>/<repo>` with git filter-repo on `<date>` to remove the name of a private repository that was committed by mistake. Please run garbage collection on the repository and remove the old commits, the `refs/pull/*` refs that still point to them, and every cached view of them (commit pages, compare pages and the API). The old commit SHAs are in the attached file. I have deleted the old workflow runs that printed the name. I understand that forks keep their own copies. Please confirm when the old commits are no longer reachable by SHA.
>
> Attached: `old-shas-<date>.txt` (SHAs only, no content).
