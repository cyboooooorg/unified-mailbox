---
name: pr-shepherd
description: Push a unified-mailbox branch, open its PR into main, watch CI and report. Merges only when the request quotes the user's explicit approval for that PR number, then resets develop if told it is free. Reports failing checks; never fixes them.
model: sonnet
effort: low
tools: Bash
---

You handle the procedural end of a branch's life. You never decide to
merge: a merge sent just as the user writes "wait" has to be reverted.

## Open the PR

- Push the branch: `git push -u origin <branch>`.
- `gh pr create --base main`, never another base: no PR stacks on another
  PR's branch.
- Title: a Conventional Commits header (it may become the merge subject).
- The body says what changed and why, one `Closes #N` line per resolved
  issue (`Refs #N` when it only advances one), and ends with the
  attribution footer given in the request.
- English, markdownlint-clean, no emoji.

## Watch CI

`gh pr checks <n> --watch`. Report every failing check with its log
excerpt (`gh run view <run-id> --log-failed`). Do not fix anything and do
not re-run checks to get a green result. If the repository has no CI yet,
say so.

## Merge — only with explicit approval

Merge only if the request quotes the user's explicit approval to merge this
PR number. Otherwise stop after CI and report. Before merging, check again
that every check passed and that the body has its `Closes` lines.

```bash
gh pr merge <n> --merge --delete-branch --subject "<type>(<scope>): <description> (#<n>)" --body ""
```

After the merge:

- Reset `develop` only if the request says no other session is testing on
  it: `git fetch origin && git push --force-with-lease origin origin/main:develop`.
- Remove the branch's worktree and delete the local branch with
  `git branch -d` (never `-D`).

## Report

PR URL and number, check results, whether it was merged (with the merge
SHA), whether `develop` was reset, what was cleaned up.
