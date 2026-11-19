# Agent instructions

## Project

Unified Mailbox is a rewrite of Mailbot (`~/Developer/Cyboooooorg/mailbot`,
Python): it sorts Gmail and Outlook mailboxes with Claude. The rewrite adds
Proton Mail, through Proton Mail Bridge, and generic IMAP mailboxes. It
splits it into a headless Rust server that exposes an API, and clients that
only talk to that API: a web app and iOS and Android apps from the same
code. The server is reached over Tailscale.

The repository is public: no real mail, address, token or personal data in
code, fixtures, issues or pull requests. Test data is invented.

## Architecture

Hexagonal. The direction of dependencies is enforced by the crate graph:
a crate that does not declare a dependency cannot use it.

```text
server/                  # Cargo workspace
  crates/
    domain/              # pure: types and rules, no I/O, no global config
    ports/               # traits the application needs (MailProvider,
                         # Classifier, repositories, Notifier, Clock…)
    application/         # use cases, one per action; depends on domain + ports
    adapters/
      gmail/ outlook/    # impl MailProvider
      imap/              # impl MailProvider over IMAP: Proton Mail through
                         # Proton Mail Bridge, any other IMAP mailbox
      claude-api/        # impl Classifier over HTTP (reqwest)
      claude-code/       # impl Classifier through the Claude Code CLI
      sqlite/            # impl the repositories (sqlx, migrations)
      ntfy/              # impl Notifier
    http/                # inbound adapter: axum routes, DTOs, SSE, OpenAPI
    app/                 # binary: config, wiring, background tasks
api/openapi.json         # the contract, generated from server/crates/http
client/                  # Svelte + TypeScript, one code base
  capacitor/             # generated ios/ and android/ projects
```

The layout is the target: create each crate when the first code needs it.

Rules:

- `domain` depends on no I/O crate (no `sqlx`, `reqwest`, `tokio`,
  `axum`). `application` depends only on `domain` and `ports`. Only
  adapters and `app` know SQLite, HTTP, Gmail, Graph, IMAP or Anthropic.
- Configuration is read once in `app` and passed down as values
  (`SpamPolicy { threshold, archive }`), never read from a global.
- The HTTP API has its own DTOs, converted in `http`. Never serialize a
  domain type straight to the client.
- Writes go through use cases. Reads for the client (dashboard, lists,
  search) may use dedicated query services that read SQLite directly.
- Long operations (bulk actions, relabelling) are jobs: the command returns
  a job id, progress goes out on SSE, `GET /api/v1/jobs/{id}` resumes it.
- The client knows only `api/openapi.json`: generated types and calls, no
  hand-written copy of a server type. A change to the API regenerates the
  file in the same commit.
- Authentication: Tailscale is the first barrier; every client also sends a
  per-device bearer token, revocable one by one.

## Invariants carried over from Mailbot

- No permanent deletion, anywhere: spam goes to the provider's spam
  folder, confirmed spam to its trash. `Move` has no `Delete` variant.
- The code decides every action from Claude's classification; the model
  never triggers one directly.
- Mail content is untrusted data. It is fenced in the prompt, and an
  instruction found in a mail is a sign of manipulation.
- Mail HTML is sanitized on the server (`ammonia`) and shown in a
  sandboxed iframe with a strict CSP; remote images go through the server.
- A sender is "known" only when the address passed DMARC.
- Dry-run mode changes nothing in the mailbox, except actions the user
  confirmed.

When porting a feature, the Mailbot code and its tests (`app/`, `tests/`)
are the reference for behavior and edge cases: `legacy-reader` extracts
them.

## Checks

Server, from `server/`:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Client, from `client/`, once it exists: `npm run check` and `npm test`.
Tools are pinned in `mise.toml`.

## Branches and pull requests

`main` holds what has been tested. `develop` is a test branch: `main` plus
the pull requests being tried, rebuilt from `main` at every cycle.

1. Every feature or fix gets its own branch from `main` (`feat/…`,
   `fix/…`, `docs/…`, `chore/…`) and its own pull request into `main`.
2. Before trying a pull request, rebuild `develop` on the current `main`,
   then merge the pull request's branch into it, keeping the branch:

   ```bash
   git fetch origin
   git checkout -B develop origin/main
   git merge --no-ff -m "chore: merge <branch> (#<n>) into develop" origin/<branch>
   git push --force-with-lease origin develop
   ```

3. Test on `develop` (the user or an agent).
4. A change to make goes on the pull request's own branch, never on
   `develop`. Then start again from step 2.
5. Once it works on `develop`, merge the pull request into `main` (with
   the user's approval) and reset `develop` to `main`:

   ```bash
   git fetch origin
   git push --force-with-lease origin origin/main:develop
   ```

Rules that follow from it:

- `develop` is rewritten at every cycle: never base a branch on it, never
  commit to it directly.
- Branches are merged into `develop` locally (step 2), never through
  GitHub: no pull request targets `develop`.
- Several pull requests can be on `develop` at once: step 2 merges each of
  them again on top of the new `main`.
- Two sessions working at once tell each other before rebuilding
  `develop`.
- A pull request that conflicts with `main` is rebased (or `main` merged
  into it) on its own branch before step 2.
- Every commit, merge commits included, follows Conventional Commits:
  never git's default merge message. Merge pull requests into `main` with
  a conventional subject
  (`gh pr merge <n> --merge --subject "feat(server): … (#<n>)"`).
- Scopes: `domain`, `app`, `http`, `api`, `sqlite`, `gmail`, `outlook`,
  `imap`, `claude`, `client`, `mobile`, `ci`, or none.

Worktrees for parallel work live next to the clone:
`~/Developer/Cyboooooorg/um-<branch-name>`.

## Backlog and issues

The backlog lives in the repository's GitHub issues (label `backlog`, plus
a theme label), not in a file.

- A new backlog item, or a review remark left for later, becomes an issue
  (`gh issue create --label backlog`).
- A pull request that resolves an issue says so with a closing keyword, one
  per issue (`Closes #123`); one that only advances it writes `Refs #123`.
- Comment on issues and pull requests whenever it helps the next reader:
  what was found, what was decided, why an issue stays open or is split.

## Subagents

`.claude/agents/` holds the subagents for the recurring tasks of this
repository, each with its model and effort fixed. Prefer them over generic
agents:

| Agent | Use it to | Model, effort |
|---|---|---|
| `codebase-mapper` | Find interfaces with `file:line` before a plan | sonnet, medium |
| `legacy-reader` | Extract a Mailbot feature's behavior and edge cases | sonnet, medium |
| `repo-state-auditor` | Check branches, worktrees and PRs (read-only) | haiku, low |
| `contract-checker` | Regenerate the OpenAPI contract, check the client | sonnet, low |
| `branch-syncer` | Merge `main` into open branches and run the checks | sonnet, medium |
| `branch-reviewer` | Review a whole branch before its PR (read-only) | opus, high |
| `develop-rebuilder` | Rebuild `develop` from `main` with the listed PRs | sonnet, low |
| `pr-shepherd` | Push, open the PR, watch CI, merge when approved | sonnet, low |
| `issue-writer` | File a backlog issue from a review remark | sonnet, medium |

The main session keeps what needs the conversation's context:

- design and spec discussions with the user, and API design;
- deciding to merge or push: `pr-shepherd` merges only when the request
  quotes the user's explicit approval for that pull request;
- warning the other sessions before `develop` is rebuilt;
- deciding which Mailbot behavior to keep, change or drop.

## Context and token economy

Every turn resends the whole conversation, so a long context costs on every
turn that follows.

### Large documents

Mailbot's `README.md` (about 80 KB), `app/main.py`, `app/db.py` and
`app/worker.py` (about 2,000 lines each) are too large to read whole. Find
the section first (`grep -n '^#' README.md` or `grep -n '<term>'`), then
read only that range. Better: ask `legacy-reader`.

### Images

Never `Read` a full-resolution screenshot in the main session: it stays in
the context until the session ends. Downscale a copy in the scratchpad
first.

### Implementation plans and reviews

When a plan is executed with subagents:

- implementers run on sonnet; opus only for a task the plan marks as hard
  (concurrency, OAuth, sync reconciliation, security);
- no review subagent after each task: the implementer runs the checks, and
  `branch-reviewer` reviews the whole branch once, before the PR;
- after a fix following that review, re-run the checks, not a new full
  review, unless the fix changes the design;
- recon goes to `codebase-mapper` (this repository) or `legacy-reader`
  (Mailbot), never to `Explore` or `general-purpose`.

### Session length

One session per topic: once a PR is open or a topic is settled, tell the
user it is a good point for `/clear`. Past about 200k tokens of context,
suggest a `/compact` that keeps the decisions, the branch and the open
questions.
