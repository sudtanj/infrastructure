# Runbook: scrub the private names from the public repo (T030 to T033)

**T033 is irreversible. It is a force-push of every branch and tag of a public repo. The lead runs it only after a fresh, explicit owner go given after reading this file, and only after features 012, 011 and 013 have all merged and no other branch or open pull request remains. Nothing in this file is run by an agent on its own.**

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
| S6 | variants of S1 and S2 | built by `variants.sh` on the owner's machine from S1 and S2: S2 with `-` replaced by `_`, `.` and a space; S1 with `/` as `%2F`, `\/` and `--`; the all-lowercase and all-uppercase forms are covered by `grep -i` |

Replacement text for S1, S2 and the variants: `PRIVATE-REPO`; for S3: `0`. The replace file is `replace.txt` on the owner's machine (not committed), one `literal==>replacement` per line, longest strings first (S1 and its variants before S2). Path renames: the two S4 pairs, `OLD path:NEW path`, with `NEW_MANUAL` as defined in spec.md.

## Tool

`git filter-repo` (not `filter-branch`). Replace in blobs with `--replace-text replace.txt`, in commit and annotated-tag messages with `--replace-message replace.txt`, and rename paths with `--path-rename`. If the installed version offers `--sensitive-data-removal`, use it too (unverified for this version; read `git filter-repo --help` first). Identities: the author and committer names and e-mail addresses are not rewritten; the check below proves none holds a string.

## Which refs get rewritten

The rewrite runs on a **mirror clone** (`git clone --mirror`), which carries every branch, every tag (lightweight and annotated) and every note ref, and `git filter-repo` rewrites all of them. A normal clone carries branches and tags only. After the rewrite:

- Pushable refs: `refs/heads/*`, `refs/tags/*`, `refs/notes/*`. Push them by name patterns, never with `--mirror`.
- Not pushable: `refs/pull/*` (GitHub hides them and rejects updates). They stay on GitHub and keep the old commits reachable. Only GitHub Support can purge them (see the request below). Remove `refs/pull/*` from the local mirror before the push (`git for-each-ref --format='%(refname)' refs/pull | xargs -n1 git update-ref -d`) so the push does not try.
- `git ls-remote origin` before the rewrite lists every remote ref; save it as `refs-before.txt`. After the push, `git ls-remote origin` must list the same refs minus `refs/pull/*` with new SHAs, and no extra ref.

## Before the go (all must be true)

1. T030 (tree scrub and rename) and T031 (old run logs deleted) are done.
2. 012, 011 and 013 are merged; `gh pr list --state open` and `git ls-remote origin` show only `main` and the tags.
3. 012's publish step is idempotent for a re-run on identical content (read `publish.rs` and its tests); 011's claims are keyed by slot, not by SHA.
4. The owner has the private backup (B1) and has listed every clone, worktree and CI cache that holds the old history.
5. The owner knows that signatures are lost (see "Signatures").
6. The owner types an explicit go in a fresh message after reading the effects below.

## Steps

**B1 backup** (owner's machine, private disk, never a shared VM or a repo): `git clone --mirror git@github.com:<owner>/<repo>.git backup-<date>.git`; keep `git rev-list --all > old-shas-<date>.txt` and `git ls-remote origin > refs-before.txt` next to it. This is the only way back.

**B2 pause, then prove nothing is running** (owner or lead with owner rights):

1. `gh workflow disable factory-ci-tick`, `gh workflow disable rust-ci-runner-release` (a moved tag would start a release run), `gh workflow disable factory-ci-selftest`, and the manual workflow (`factory-ci-manual`) and the mirror-images workflow.
2. Wait until no run is queued or in progress in the repo: `gh run list --status queued`, `gh run list --status in_progress` and `gh run list --status waiting` must each print nothing (repeat until they do; a tick already running finishes on its own, do not cancel it mid-post). The tick's `concurrency` group keeps at most one pending run; disabling the workflow drops a pending run, so check the three lists again after one minute.
3. Only then continue. If a run starts after this check (a workflow was missed), stop and repeat B2.

**B3 allow the force-push**: note the current branch protection or ruleset on `main` and the tag rule on `rust-ci-runner-v*` (settings and screenshot), then lift both temporarily (owner).

**B4 rewrite** in a fresh mirror clone: `git clone --mirror git@github.com:<owner>/<repo>.git work.git && cd work.git && git filter-repo --replace-text ../replace.txt --replace-message ../replace.txt --path-rename OLD:NEW ...` (filter-repo accepts a bare mirror; it removes `origin`, so `git remote add origin git@github.com:<owner>/<repo>.git`). Delete `refs/pull/*` locally as above.

**B5 pre-push check** (all must pass in `work.git`; any failure stops the run, nothing is pushed yet):

1. Verification greps below: zero hits.
2. Refs: `git for-each-ref --format='%(refname)'` lists exactly the pushable refs of `refs-before.txt` (same names, no ref missing, none extra). The number of commits per branch (`git rev-list --count <branch>`) equals the count in the backup (`git -C backup-<date>.git rev-list --count <branch>`); the number of tags equals; annotated tags are still annotated (`git for-each-ref --format='%(objecttype)' refs/tags`).
3. Tip comparison: for each branch, fetch the old tip into `work.git` as a throwaway ref (`git fetch backup-<date>.git refs/heads/<branch>:refs/old/<branch>`) and run `git diff --stat refs/old/<branch> <branch>`: only the files that held a string, the renamed files and nothing else; `git diff refs/old/<branch> <branch>` shows only replaced lines. Then `git update-ref -d` every `refs/old/*` so none is pushed.
4. The names of the two renamed paths exist and the old paths do not (`git ls-tree -r <branch> --name-only`).
5. The release workflow, the tick and the selftest are still disabled; `gh run list --status in_progress` is still empty (repeat B2 step 2).

**B6 push**: `git push --force origin 'refs/heads/*:refs/heads/*' 'refs/tags/*:refs/tags/*'`, and `'refs/notes/*:refs/notes/*'` when notes exist. Do not use `--mirror` and do not push `refs/pull/*`. This is the irreversible step.

**B7 restore**: re-apply the branch protection and the tag rule exactly as noted in B3 and check them in the settings; `gh workflow enable` for the selftest, the release workflow, the mirror-images workflow, the manual workflow, and last `gh workflow enable factory-ci-tick`. The moved tags started no release because the workflow was disabled.

**B8 first tick and verification on a fresh clone**: see "Effects on the tick". Then run the verification greps on a fresh `git clone --mirror` of the public repo.

**B9 GitHub Support**: send the request below only after B8.

**B10 discard** every old clone and worktree (the old history sits in them) except the private backup.

## Signatures

`git filter-repo` rewrites each commit, so the `gpgsig` header (GPG or SSH signature) of a rewritten commit is dropped, and the signature of an annotated tag that points at a rewritten commit no longer verifies. After the push, every commit and signed tag shows as unverified on GitHub. Re-signing is possible only with the owner's key (`git rebase --exec` or re-creating the tags with `git tag -s`), and it would change the SHAs again; the default is to accept unverified history. If a ruleset requires signed commits, the owner lifts that rule in B3 and restores it in B7 (new commits after the rewrite are signed as before).

## Effects

- Every SHA in the repo changes. Tags `rust-ci-runner-v*` move to new commits; the release objects stay and point at the tags. Container image tags and digests are untouched, and the `RUNNER_IMAGE_DIGEST` and `RUNNER_IMAGE` pins stay valid because they name image digests, not git SHAs.
- Open pull requests and branches break (none may exist at the go). Every clone and fork has an unrelated history.
- GitHub keeps the old commits reachable by SHA, and through `refs/pull/*` and cached views, until GitHub Support purges them. Public forks keep their own copy; only their owners can delete them (`gh api repos/<owner>/<repo>/forks` lists them).
- The commit statuses the tick walk relies on vanish with the old SHAs: the new tip has none.
- The old workflow runs and their logs stay until deleted (T031). Workflow run history is keyed by workflow file and name, so deleting the runs is how the old workflow name leaves the Actions list.

## Effects on the tick (`walk.rs` `decide` and `walk_one`)

For each project at a watched tip, `decide` first asks for the project's own status on the tip. The new tip has none, so the project is `open`, `needs_row` is true and `decide` lists the commits and builds the first-parent chain of at most 50 ancestors. `walk_one` asks for a status of the project's context on each ancestor; every ancestor is new and has none, so it returns `Run { prev: None }`. Result: one run per project at each watched tip of the rewritten repo, once, with no `prev`. The run posts statuses on the new tip; the next tick sees `Done`. It does not run every commit, and it does not fail, because "no status anywhere" is the normal first-run case. Other watched repos are untouched. Expect one run for each project of the infrastructure repo's `main` (the runner project, the canary project and each image folder; for an image folder the publish step runs again, which is why its idempotence is a precondition). Feature 011's schedule claims are keyed by slot and are not affected.

## Verification grep (B5 and B8)

`strings.txt` holds S1, S2, S3, each S5 the owner marked real, and the S6 variants, one per line. Run all of these on the rewritten `work.git` (B5) and on a fresh `git clone --mirror` of the public repo (B8); each must print nothing:

1. **Every object, any case**: `git cat-file --batch-all-objects --batch | grep -a -i -F -f strings.txt` (commits with messages and identities, annotated tag objects with their messages, trees with file names, blobs, note blobs; `-a` reads binary data, `-i` covers case variants). This also covers the tag messages and notes that a log-only grep misses.
2. **Commit view, as a second reading**: `git log --all --format='%H %an %ae %cn %ce %s%n%b' | grep -i -F -f strings.txt`.
3. **Annotated tag text**: `git for-each-ref --format='%(refname) %(contents)' refs/tags | grep -i -F -f strings.txt`.
4. **Notes**: `git log --all --notes='*' --format='%N' | grep -i -F -f strings.txt`.
5. **Paths**: `git rev-list --all --objects | cut -d' ' -f2- | grep -i -F -f strings.txt`.
6. **Ref names**: `git for-each-ref --format='%(refname)' | grep -i -F -f strings.txt`.
7. **GitHub side** (B8 only): `gh api 'repos/<owner>/<repo>/pulls?state=all&per_page=100' --paginate --jq '.[] | .title, .body, .head.ref'` and `gh api 'repos/<owner>/<repo>/issues?state=all&per_page=100' --paginate --jq '.[] | .title, .body'` contain no string; `gh run list` for the old workflow is empty; the repo description, topics and README render without a string.

A hit in B5 stops the run before the push. A hit in B8 means the support request must name the surviving object.

## Draft request to GitHub Support (do not send before B8)

> Subject: Remove cached views and pull request refs of rewritten history, public repository `<owner>/<repo>`
>
> I rewrote the full history of `<owner>/<repo>` with git filter-repo on `<date>` to remove the name of a private repository that was committed by mistake. Please run garbage collection on the repository and remove the old commits, the `refs/pull/*` refs that still point to them, and every cached view of them (commit pages, compare pages and the API). I could not delete the `refs/pull/*` refs myself. The old commit SHAs are in the attached file. I have deleted the old workflow runs that printed the name. I understand that forks keep their own copies. Please confirm when the old commits are no longer reachable by SHA.
>
> Attached: `old-shas-<date>.txt` (SHAs only, no content).
