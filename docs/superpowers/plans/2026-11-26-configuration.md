# Configuration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps
> use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `app` reads `.env` and the environment once, parses them into
typed values (`StoreConfig`, `SpamPolicy`, dry-run, log filter), reports
every invalid variable at once, then sets up logging and opens the store.

**Architecture:** A pure `config` module in `app` (`Config::from_vars`
over a `HashMap<String, String>`), a small `env` module that builds that
map from `.env` and the process environment, and `main` wiring both to
`tracing-subscriber` and `sqlite::Store::open`. No other crate changes.

**Tech Stack:** Rust 1.99 (edition 2024), `dotenvy` 0.15,
`tracing-subscriber` 0.3 (feature `env-filter`), `tokio`, `tracing`,
`thiserror`, `tempfile` (dev). Use the latest compatible patch versions,
declared in `[workspace.dependencies]`.

**Spec:** `docs/superpowers/specs/2026-11-26-configuration-design.md`

## Global Constraints

- Only `app` reads `std::env`; no global config. `cargo test -p app
  --test layering` stays green.
- Variable names are exact, upper case, no prefix: `DATA_DIR`,
  `KEY_FILE`, `DRY_RUN`, `SPAM_THRESHOLD`, `PROTECT_KNOWN_SENDERS`,
  `ARCHIVE_CATEGORIES`, `RUST_LOG`.
- An unset variable or one that is empty after trimming takes its
  default; values are trimmed before parsing.
- Defaults: `DATA_DIR=./data`, `KEY_FILE=DATA_DIR/secret.key`,
  database `DATA_DIR/unified-mailbox.db`, `DRY_RUN=true`,
  `SPAM_THRESHOLD=90` (0..=100), `PROTECT_KNOWN_SENDERS=true`,
  `ARCHIVE_CATEGORIES` empty, `RUST_LOG=info`.
- Booleans: `true false 1 0 yes no on off`, case-insensitive.
- Every error names the variable, the value and what was expected; all
  errors are reported together; exit status 1.
- English for code, comments, logs, docs. Invented test data only.
- `mise run ci` passes at the end of each task.

## Review Focus

1. The whole process environment is passed in: unknown variables
   (`PATH`, `HOME`, lower-case `dry_run`) are ignored, never an error.
2. A process variable that is not valid UTF-8 must not panic the server
   (`std::env::vars` panics): skip it.
3. `RUST_LOG` with an invalid filter is a config error listed with the
   others, not a panic in `tracing-subscriber`.
4. `DATA_DIR` that exists as a regular file, or `KEY_FILE` whose parent
   directory is missing, stops startup with a message naming the
   variable, before `Store::open`.
5. Values with surrounding spaces (` 95 `, ` newsletter , promotion `)
   parse as their trimmed form.

Each line has its test in Task 1 (1, 3, 5) or Task 2 (2, 4).

---

### Task 1: Parse variables into `Config`

**Files:**

- Modify: `server/Cargo.toml` (workspace dep `tracing-subscriber`),
  `server/crates/app/Cargo.toml` (deps `domain`, `sqlite`, `thiserror`,
  `tracing-subscriber`)
- Create: `server/crates/app/src/config.rs` (tests in-module)
- Modify: `server/crates/app/src/main.rs` (`mod config;` only)

**Interfaces:**

- Produces:

  ```rust
  pub struct Config {
      pub data_dir: PathBuf,
      pub store: sqlite::StoreConfig,
      pub spam: domain::SpamPolicy,
      pub dry_run: bool,
      pub log_filter: String,
  }
  impl Config {
      pub fn from_vars(vars: &HashMap<String, String>) -> Result<Config, ConfigErrors>;
  }
  pub struct ConfigError { pub variable: &'static str, pub message: String }
  pub struct ConfigErrors(pub Vec<ConfigError>); // Display: one line per error; impl Error
  ```

  `ConfigError` displays as `{variable}: {message}`; the message quotes
  the value and says what was expected (`"101" is not an integer from 0
  to 100`). `SpamPolicy`, `SpamConfidence` and `TagKey` are re-exported
  at the root of `domain`.

- [ ] **Step 1: Write the failing tests** in `config.rs`, with a helper
  `vars(&[(&str, &str)]) -> HashMap<String, String>`:
  - `defaults_when_unset`: empty map → `data_dir == "./data"`,
    `store.database == "./data/unified-mailbox.db"`,
    `store.key_file == "./data/secret.key"`, `dry_run`,
    `spam.threshold.get() == 90`, `spam.protect_known_senders`,
    `spam.archive.is_empty()`, `log_filter == "info"`.
  - `defaults_when_empty`: every variable set to `"  "` → same values.
  - `unknown_variables_ignored`: `PATH`, `HOME`, `dry_run=false` →
    `Ok`, `dry_run` still `true`.
  - `bool_forms`: each of `true TRUE 1 yes On` → `true`, each of
    `false 0 no OFF` → `false` (for `DRY_RUN`); `maybe` → error on
    `DRY_RUN`.
  - `threshold_bounds`: `0`, `100`, `" 95 "` accepted (0, 100, 95);
    `-1`, `101`, `abc`, `9.5` each → one error on `SPAM_THRESHOLD`
    whose display contains the value.
  - `archive_categories_normalised`:
    `" Newsletter , promotion,,newsletter "` → `{newsletter,
    promotion}`; `"news letter"` and `"news_letter"` → error on
    `ARCHIVE_CATEGORIES`.
  - `key_file`: `DATA_DIR=/srv/um` → key `/srv/um/secret.key`, database
    `/srv/um/unified-mailbox.db`; adding `KEY_FILE=/keys/um.key` → key
    `/keys/um.key`, database unchanged.
  - `invalid_log_filter`: `RUST_LOG="sqlite=loud"` (a level that does
    not exist; confirm `EnvFilter::try_new` rejects it, else pick a
    value it does) → error on `RUST_LOG`;
    `RUST_LOG="sqlite=debug"` → `log_filter == "sqlite=debug"`.
  - `all_errors_reported`: `DRY_RUN=maybe`, `SPAM_THRESHOLD=101`,
    `ARCHIVE_CATEGORIES="a b"` → exactly 3 errors, variables in that
    order; `ConfigErrors` display has 3 lines.

- [ ] **Step 2: Run** `cargo test -p app config` from `server/`.
  Expected: compile failure (`Config` undefined).

- [ ] **Step 3: Implement `Config::from_vars`.** One private helper per
  kind (`get(vars, name) -> Option<&str>` trimming and treating empty
  as unset, `parse_bool`, `parse_threshold`, `parse_tag_list`) that
  push into a shared `Vec<ConfigError>` and return a fallback so
  parsing continues; return `Err(ConfigErrors)` if the vec is non-empty.
  Threshold: parse `i64`, check `0..=100`, then `SpamConfidence::new`.
  Tags: `TagKey::from_stored` on each normalised item, into a
  `BTreeSet`. `RUST_LOG`: validate with
  `tracing_subscriber::EnvFilter::try_new`, keep the string. `main`
  uses `Config` only in Task 2: if clippy flags dead code in this task,
  put `#[cfg_attr(not(test), allow(dead_code))]` on `mod config;` and
  remove it in Task 2.

- [ ] **Step 4: Run** `cargo test -p app config`, then `mise run ci`.
  Expected: all pass.

- [ ] **Step 5: Commit**

  ```bash
  git add server/Cargo.toml server/Cargo.lock server/crates/app
  git commit -m "feat(app): parse the configuration into typed values"
  ```

### Task 2: Load the environment, wire `main`, document

**Files:**

- Create: `server/crates/app/src/env.rs` (tests in-module)
- Modify: `server/crates/app/src/main.rs`, `server/Cargo.toml`
  (workspace dep `dotenvy`), `server/crates/app/Cargo.toml` (deps
  `dotenvy`, `tokio`, `tracing`; dev-dep `tempfile`)
- Create: `.env.example` (repository root)
- Modify: `README.md` (new Configuration section; storage and key
  sections point to it instead of `#12`), `.gitignore` (`.env`,
  `/data/`, `server/data/`)

**Interfaces:**

- Consumes: `Config::from_vars`, `ConfigErrors` (Task 1).
- Produces:

  ```rust
  // env.rs
  pub fn read_dotenv(path: &Path) -> Result<Vec<(String, String)>, dotenvy::Error>; // missing file → Ok(vec![])
  pub fn overlay(
      file: impl IntoIterator<Item = (String, String)>,
      process: impl IntoIterator<Item = (OsString, OsString)>,
  ) -> HashMap<String, String>; // process wins; non-UTF-8 entries skipped
  // main.rs
  fn prepare_dirs(config: &Config) -> Result<(), String>; // message names DATA_DIR or KEY_FILE
  ```

- [ ] **Step 1: Write the failing tests.**
  - `env.rs`: `process_beats_file` (`DRY_RUN` in both → process value);
    `file_only_kept`; `non_utf8_skipped` (on unix, an
    `OsString::from_vec(vec![0xff])` value → key absent, no panic);
    `missing_dotenv_is_empty` (path in a `tempfile::TempDir` that does
    not exist → `Ok(vec![])`); `malformed_dotenv_is_error` (a file
    `dotenvy` rejects, such as a line `NOT VALID`; confirm it does, else
    pick one it rejects → `Err`); `dotenv_read` (`DRY_RUN=false` →
    `[("DRY_RUN","false")]`).
  - `main.rs`: `prepare_dirs_creates_data_dir` (missing `DATA_DIR` in a
    temp dir → created, mode `0700` on unix);
    `prepare_dirs_rejects_file` (`DATA_DIR` is a regular file → `Err`
    containing `DATA_DIR`); `prepare_dirs_rejects_missing_key_parent`
    (`KEY_FILE` under a missing directory → `Err` containing
    `KEY_FILE`); `prepare_dirs_accepts_bare_key_file` (`KEY_FILE` set to
    a bare file name: its parent is the empty path, meaning the working
    directory → `Ok`).

- [ ] **Step 2: Run** `cargo test -p app`. Expected: compile failure.

- [ ] **Step 3: Implement** `read_dotenv` with
  `dotenvy::from_path_iter` (map `Error::Io` of kind `NotFound` to an
  empty vec; `dotenvy::Error::not_found()` tells it), `overlay` (file
  entries first, then process entries converted with `into_string`,
  skipping failures), and `prepare_dirs` (`DirBuilder` with
  `recursive(true)` and mode `0o700` on unix; then check
  `data_dir.is_dir()`, and that `key_file.parent()` is a directory,
  an empty parent counting as the working directory). An existing
  `DATA_DIR` keeps its permissions.

- [ ] **Step 4: Wire `main`** as `#[tokio::main] async fn main() ->
  ExitCode`:
  1. `read_dotenv(Path::new(".env"))`, on error print to stderr and
     return `ExitCode::FAILURE`;
  2. `overlay(file, std::env::vars_os())`, `Config::from_vars`; on
     error print `configuration errors:` then the `ConfigErrors`
     display to stderr and return `ExitCode::FAILURE`;
  3. init `tracing_subscriber::fmt()` with
     `EnvFilter::new(&config.log_filter)`;
  4. `prepare_dirs`, then `sqlite::Store::open(config.store.clone())`;
     on error `tracing::error!` and return `ExitCode::FAILURE`;
  5. `tracing::info!(dry_run = config.dry_run, "store ready")`, return
     success.

- [ ] **Step 5: Run** `cargo test -p app`, then by hand from an empty
  scratch directory (so no `.env` is picked up), with
  `M=<repo>/server/Cargo.toml`:
  `SPAM_THRESHOLD=101 DRY_RUN=maybe cargo run -q --manifest-path $M -p app`
  prints both errors and exits 1;
  `DATA_DIR=./d cargo run -q --manifest-path $M -p app` creates
  `./d/unified-mailbox.db` and `./d/secret.key` and exits 0.

- [ ] **Step 6: Document.** `.env.example`: each variable of this
  change, commented out with its default and one English line on what
  it does. README Configuration section: the table of the spec's
  "Variables of this change", that a real variable beats `.env`, that
  names are exact upper case, that the server lists all invalid values
  and exits at startup, and that later features add their variables
  here. Markdown follows `.markdownlint.jsonc`.

- [ ] **Step 7: Run** `mise run ci`. Expected: pass.

- [ ] **Step 8: Commit**

  ```bash
  git add server .env.example README.md .gitignore
  git commit -m "feat(app): load the configuration at startup and open the store"
  ```

  Footer: `Closes: #12`, then the session's attribution trailers (both
  commits carry them).
