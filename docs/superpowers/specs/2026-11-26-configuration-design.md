# Configuration design

Issue: #12. Related: #11 (sqlite adapter), #13 (HTTP skeleton), #54
(settings from the client), #55 (backups), #57 (deployment).

## Intent

The server reads its configuration once, at startup, in `app`, and passes
typed values down to the crates that need them. No crate reads the
environment or a global. A bad value stops the server at startup with
every error listed, instead of failing later inside a background task.

## Goals

- A `config` module in `app` that turns a map of variables into typed
  sections, one per consumer, and reports every invalid variable at once.
- The variables that already have a consumer: the store (`StoreConfig`,
  #11), the spam policy (`SpamPolicy`, `domain`), dry-run, the log level.
- `main` reads the configuration, sets up logging and opens the store.
- The full table of Mailbot's variables, with what becomes of each, so
  that the issue that ports a feature knows its variables.
- Every variable of this change documented in the README and in a
  `.env.example`.

## Non-goals

- Variables whose consumer does not exist yet. Each comes with the issue
  that adds its feature, following the table below (approach A, chosen
  over typing every variable now).
- Settings edited from the client and stored in the database (#54).
- A secret wrapper type: no variable of this change is a secret. The
  first issue that adds one (#16 or #19) adds the type (no `Debug`
  output of the value, `zeroize` on drop).
- The listen address: #13 adds `LISTEN_ADDR` with the HTTP server.

## Anti-goals

- A crate other than `app` reading `std::env`, or a global config.
- A value accepted at startup that a later check would reject (Mailbot's
  `CONCURRENCY=0` blocks its worker).
- A secret printed in an error or a log.

## Approaches

- **A, chosen.** Type only the variables that have a consumer today;
  this spec holds the table of all the others. It follows the rule of the
  project (code arrives with its first use), and bounds are fixed when
  the feature that uses them is designed. #12 does not cover every
  variable its text lists: the others are left to their issues.
- **B.** Type the ~35 variables kept from Mailbot now. #12 complete in
  one go, but most fields have no reader for weeks, and their bounds may
  change when their feature is designed.

Loading: a small hand-written parser over a `HashMap<String, String>`,
rather than a configuration crate (`figment`, `config`), because the
rules are few and the map makes tests trivial.

## Loading

1. `dotenvy` reads `.env` from the working directory into a map, if the
   file exists. A missing file is fine; an unreadable or malformed one is
   an error, reported with its line number and never the line's content,
   which may hold a secret. `dotenvy` expands `$NAME` outside single
   quotes (python-dotenv only `${NAME}`): the README says so.
2. The process environment is laid over it: a real variable beats
   `.env`, as in Mailbot (pydantic-settings).
3. `Config::from_vars(&HashMap<String, String>)` parses the map and
   returns `Config` or the list of all errors.

Only `main` touches the process environment; tests call `from_vars`.

Names are Mailbot's, upper case and without prefix, so that a Mailbot
`.env` mostly carries over. Lookup is exact (upper case); Mailbot's
case-insensitive lookup is dropped.

## Parsing rules

- An unset variable, or one set to the empty string (after trimming),
  takes its default. No variable of this change is required.
- Booleans accept, case-insensitively: `true`, `t`, `false`, `f`, `1`,
  `0`, `yes`, `y`, `no`, `n`, `on`, `off` (pydantic's set). Anything else
  is an error.
- Integers must parse and be within the variable's bounds, inclusive.
  **Deviation from Mailbot:** Mailbot checks bounds only for values set
  from its settings page and accepts any integer from the environment.
  Here the bounds apply to the environment too.
- Lists are comma-separated: each item is trimmed and lower-cased, empty
  items are dropped (Mailbot's `_split`). Duplicates collapse.
- Every error names the variable, the value and what was expected. The
  server prints all of them and exits with a non-zero status.

## Variables of this change

| Variable | Type | Default | Rule | Goes to |
|---|---|---|---|---|
| `DATA_DIR` | path | `./data` | Created at startup if missing (`0700`) | `StoreConfig::database` = `DATA_DIR/unified-mailbox.db` |
| `KEY_FILE` | path | `DATA_DIR/secret.key` | Its parent must exist | `StoreConfig::key_file` |
| `DRY_RUN` | bool | `true` | | `Config::dry_run` |
| `SPAM_THRESHOLD` | int | `90` | 0..100 | `SpamPolicy::threshold` |
| `PROTECT_KNOWN_SENDERS` | bool | `true` | | `SpamPolicy::protect_known_senders` |
| `ARCHIVE_CATEGORIES` | list | empty | Each item a well-formed tag key (`TagKey::from_stored`) | `SpamPolicy::archive` |
| `RUST_LOG` | filter | `info` | `tracing-subscriber` `EnvFilter` syntax | logging |

`KEY_FILE` is separate so the key can live outside the data directory
and be backed up apart from the database (#55). The store itself creates
both files with mode `0600` and checks the key (#11).

`ARCHIVE_CATEGORIES` is checked for form only: tags live in the database
and their defaults come with #25, so whether each key exists is checked
by #25, not at configuration time.

## Mailbot variables and where they go

| Mailbot variable | Fate | Issue |
|---|---|---|
| `DATA_DIR` | Kept (this change) | #12 |
| `BACKUP_DIR`, `AUTO_BACKUP_DAYS`, `AUTO_BACKUP_KEEP` | Kept | #55 |
| `DRY_RUN` | Kept (this change) | #12, #23 |
| `SPAM_THRESHOLD`, `PROTECT_KNOWN_SENDERS`, `ARCHIVE_CATEGORIES` | Kept (this change) | #12, #24 |
| `TRUSTED_SENDERS` | Kept | #24 |
| `SPAM_PURGE_HOURS` | Kept | #48 |
| `AUDIT_RETENTION_DAYS` | Kept | #26 |
| `FOLLOWUP_DAYS`, `REPLY_DAYS` | Kept | #39 |
| `POLL_INTERVAL`, `BACKFILL_DAYS`, `JUNK_DAYS`, `INDEX_DAYS`, `INDEX_PER_CYCLE` | Kept | #18, #31 |
| `CONCURRENCY` | Kept, minimum 1 | #19 |
| `CLAUDE_BACKEND` | Kept as the classifier choice, under a provider-neutral name and values decided by #19, so that #71 (OpenAI-compatible servers, local models) adds a value | #19, #20, #71 |
| `CLAUDE_MODEL`, `CLAUDE_EFFORT`, `MAX_BODY_CHARS` | Kept; `CLAUDE_MODEL` checked against the list at startup too | #19, #20 |
| `ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN` | Kept, secrets | #19, #20 |
| `CLAUDE_BUDGET` | Kept | #21 |
| `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` | Kept, secret | #16 |
| `MS_CLIENT_ID`, `MS_CLIENT_SECRET` | Kept, secret | #27 |
| `BASE_URL` | Kept: OAuth redirect URIs and ntfy links (the server's tailnet URL) | #16, #53 |
| `NTFY_URL`, `NTFY_TOKEN`, `ALERT_*` | Kept, `NTFY_*` secrets | #53 |
| `PRIVACY_FOLLOWUPS`, `PRIVACY_RENEWAL_MONTHS` | Deferred with the module | #59 |
| `ALLOWED_HOSTS` | Dropped: Tailscale and per-device tokens replace the Host check | #13 |
| `MAX_PER_CYCLE` | Dropped: Mailbot never reads it | — |
| (new) `LISTEN_ADDR` | Added with the HTTP server | #13 |
| (new) `KEY_FILE`, `RUST_LOG` | Added (this change) | #12 |

Bounds and defaults of the kept variables are Mailbot's
(`app/config.py`) unless the issue that adds them decides otherwise in
its own spec. The variables Mailbot lets the user edit at runtime are
marked as such by #54; in this design they are defaults that #54 will
let the database override.

## Shape of the code

- `app/src/config.rs`: `Config { store: StoreConfig, spam: SpamPolicy,
  dry_run: bool, log_filter: String }`, `Config::from_vars`, a
  `ConfigError { variable, message }` and `ConfigErrors(Vec<ConfigError>)`
  that displays one line per error.
- `app/src/main.rs`: load `.env` and the environment into a map, parse,
  print the errors and exit 1 on failure; otherwise set up
  `tracing-subscriber`, create `DATA_DIR`, open the store, log that it is
  ready and return. #13 turns this into a running server.
- `app` gains dependencies on `domain`, `sqlite`, `tokio`, `dotenvy`,
  `tracing`, `tracing-subscriber`.

## Tests

In `config.rs`, against maps built in the test, never the process
environment:

- every default when the map is empty, and when every variable is the
  empty string;
- each bool form accepted, an unknown one rejected;
- `SPAM_THRESHOLD` at 0, 100 accepted; -1, 101, `abc` rejected;
- `ARCHIVE_CATEGORIES` trimmed, lower-cased, empties dropped,
  duplicates collapsed; a malformed key rejected;
- `KEY_FILE` defaulting under `DATA_DIR`, and overridden;
- several invalid variables reported together;
- a test for the overlay: a variable from the environment map beats the
  same one from the `.env` map.

## Documentation

- README: a Configuration section with the table of this change's
  variables, how `.env` and the environment combine, and the startup
  error behavior; the storage and key sections point to it instead of
  to #12.
- `.env.example` at the repository root, in English, listing this
  change's variables with their defaults.
- Each later issue adds its variables to both.

## Left to the builder

- Whether the `.env` overlay is a function tested on its own or folded
  into `main`.
- The exact wording of error messages.
