# SQLite Adapter Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps
> use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A `sqlite` adapter crate that opens the database with its
settings, runs the phase 1 migration (schema, constraints, full-text
search kept by triggers) and encrypts account secrets with a key kept
outside the database.

**Architecture:** `server/crates/adapters/sqlite`, a library crate. A
`secret` module (key file, XChaCha20-Poly1305), a `store` module (`Store`
with a one-connection write pool and a `query_only` read pool, migrations
embedded with `sqlx::migrate!`), a `time` module (stored timestamp
format). SQL lives in `migrations/0001_initial.sql`. No port trait and no
repository yet.

**Tech Stack:** Rust 1.99 (edition 2024), `sqlx` 0.8 (features
`runtime-tokio`, `sqlite`, `migrate`, `macros`), `tokio`,
`chacha20poly1305` 0.10, `zeroize` 1, `getrandom` 0.3, `chrono`,
`thiserror`, `tracing` 0.1. Use the latest compatible patch versions.

**Spec:** `docs/superpowers/specs/2026-11-25-sqlite-adapter-design.md`

## Global Constraints

- Only this crate (and later `app`) depends on `sqlx`: `cargo test -p app
  --test layering` stays green.
- Timestamps: TEXT, UTC, `2026-11-25T08:30:00.000Z` (fixed width, three
  fractional digits, `Z`). Never `now`, `CURRENT_TIMESTAMP` or a
  `DEFAULT` time in SQL: the code writes every timestamp.
- Codes, never sentences: provider `gmail|outlook|imap`
  (`ProviderKind::as_code`), location
  `inbox|archive|junk|trash|sent|folder`, source `inbox|junk`, review
  `rescued|confirmed`, priority `high|normal|low`
  (`Priority::as_code`), billing `api|batch|subscription`, audit actor
  `user|device|system|claude|rule`.
- Body cut: 50,000 characters (`MAX_STORED_BODY`), enforced by a `CHECK`
  on `length(body)`.
- Key: exactly 32 bytes; file mode `0600`; refused if group or others
  have any permission bit.
- Stored secret: `[version = 1][24-byte nonce][ciphertext]`, associated
  data = the account id as 8 little-endian bytes.
- Test data is invented (`alice@example.com`), databases live in a
  `tempfile::TempDir`.
- Error variants are codes with fields, no user-facing sentence.
- `mise run ci` passes after every task. Commits: Conventional Commits,
  scope `sqlite`, footer `Refs: #11` (final task `Closes: #11`), then
  the `Co-Authored-By:` trailer the session requires.
- Unix only (the server runs on Linux): file modes use
  `std::os::unix::fs::{OpenOptionsExt, PermissionsExt}`, and every mode
  compared or reported is `mode & 0o777` (metadata also carries the file
  type bits).

## Review Focus

- A key file present but empty or 31/33 bytes long (a truncated copy):
  must refuse to start, never regenerate over it. Pinned in Task 1.
- A key file that does not exist but whose directory does not either:
  error naming the path, no panic. Pinned in Task 1.
- A stored secret with an unknown version byte or shorter than 25 bytes
  (corrupted row): `SecretError::Undecryptable`, no panic on slicing.
  Pinned in Task 1.
- Opening a database file that already exists with mode `0644`
  (created by hand): opens; mode is left as is (the spec only covers
  creation). Pinned in Task 2 as "existing file opens".
- A mail whose `sender` and `message_key` are `NULL` (spam without
  parseable From or Message-ID): insert succeeds and it is searchable by
  subject. Pinned in Task 4.

---

### Task 1: Secret key file and cipher

**Files:**

- Modify: `server/Cargo.toml` (members add `"crates/adapters/*"`;
  workspace dependencies for the crates of the Tech Stack;
  `sqlite = { path = "crates/adapters/sqlite" }`)
- Create: `server/crates/adapters/sqlite/Cargo.toml` (package `sqlite`,
  workspace lints; dev-dependencies `tempfile`, `tokio` with `macros`,
  `rt-multi-thread`)
- Create: `server/crates/adapters/sqlite/src/lib.rs`
- Create: `server/crates/adapters/sqlite/src/secret.rs`
- Modify: `README.md` (a "Secret key" section: the key file's path comes
  from the configuration (#12), it is generated at first start, it must
  be backed up apart from the database, losing it means reconnecting
  every account)

**Interfaces:**

- Produces:
  - `pub struct SecretKey` (32 bytes, `Zeroize` on drop, no `Debug` that
    prints bytes).
  - The functions and errors:

    ```rust
    pub fn load_or_create_key(path: &Path) -> Result<SecretKey, KeyFileError>;
    pub enum KeyFileError {
        Io { path: PathBuf, source: io::Error },
        WrongLength { path: PathBuf, len: u64 },
        TooOpen { path: PathBuf, mode: u32 },
    }
    pub fn encrypt(key: &SecretKey, account_id: i64, plain: &[u8]) -> Vec<u8>;
    pub fn decrypt(key: &SecretKey, account_id: i64, stored: &[u8])
        -> Result<Vec<u8>, SecretError>;
    pub enum SecretError { Undecryptable }
    ```

- [ ] **Step 1: Write the failing tests** in `secret.rs`
  (`#[cfg(test)] mod tests`):
  - `missing_key_file_is_created_0600`: path in a temp dir; after the
    call the file exists, is 32 bytes, `mode & 0o777 == 0o600`.
  - `existing_key_is_reused`: two calls on one path; a secret encrypted
    with the first key decrypts with the second.
  - `short_key_is_refused`: file of 31 bytes, mode `0600` →
    `WrongLength { len: 31, .. }`; also 0 and 33 bytes; file unchanged
    afterwards.
  - `group_readable_key_is_refused`: 32 bytes, mode `0640` →
    `TooOpen { mode: 0o640, .. }`.
  - `missing_directory_is_an_io_error`: path `<tmp>/nope/key` →
    `Io { path, .. }` with that path.
  - `round_trip`: `decrypt(k, 7, &encrypt(k, 7, b"token")) == b"token"`.
  - `other_account_fails`: encrypt for 7, decrypt for 8 →
    `Undecryptable`.
  - `other_key_fails`: two keys from two files → `Undecryptable`.
  - `nonce_is_random`: two encryptions of the same input differ.
  - `corrupt_stored_values_fail`: empty, 24 bytes, version byte `2`,
    one flipped ciphertext byte → each `Undecryptable`.

- [ ] **Step 2: Run them to verify they fail**

  Run: `cd server && cargo test -p sqlite secret`
  Expected: compile errors (functions not defined).

- [ ] **Step 3: Implement** in `secret.rs`. Creation uses
  `OpenOptions::new().write(true).create_new(true).mode(0o600)`, fills
  32 bytes with `getrandom::fill`, `sync_all`, then logs
  `tracing::warn!(path = %path.display(), "generated a new secret key")`
  (never the bytes). Checks on an existing file: length first, then
  `mode & 0o077 != 0` → `TooOpen`. A missing file is detected by
  `NotFound` on `metadata`; any other I/O error, and a failed creation
  (missing directory), is `Io` with the path. Cipher:
  `XChaCha20Poly1305` with
  `Payload { msg, aad: &account_id.to_le_bytes() }`, nonce of 24 bytes
  from `getrandom::fill`; check length ≥ 25 and the version byte before
  slicing. Re-export the public items from
  `lib.rs`. Add the README section.

- [ ] **Step 4: Run the tests and the checks**

  Run: `cd server && cargo test -p sqlite secret && mise run ci`
  Expected: all pass.

- [ ] **Step 5: Commit**

  ```bash
  git add server README.md
  git commit -m "feat(sqlite): encrypt account secrets with a key file" -m "Refs: #11"
  ```

### Task 2: Store, connection settings and migrations

**Files:**

- Create: `server/crates/adapters/sqlite/src/store.rs`
- Create: `server/crates/adapters/sqlite/src/time.rs`
- Create: `server/crates/adapters/sqlite/migrations/0001_initial.sql`
  (this task: `accounts` only; Tasks 3 and 4 extend the same file, which
  is unapplied until the branch merges)
- Create: `server/crates/adapters/sqlite/README.md` (purpose, migration
  rules from the spec, stored value formats, key file, the two pools)

**Interfaces:**

- Consumes: `load_or_create_key`, `encrypt`, `decrypt`, `SecretError`
  (Task 1).
- Produces:
  - `pub struct StoreConfig { pub database: PathBuf, pub key_file: PathBuf }`
  - `pub struct Store { write: SqlitePool, read: SqlitePool, key: SecretKey }`
    with `pub(crate) fn write(&self) -> &SqlitePool` and
    `pub(crate) fn read(&self) -> &SqlitePool`.
  - The methods and errors:

    ```rust
    impl Store {
        pub async fn open(config: StoreConfig) -> Result<Store, StoreError>;
        pub async fn close(self);
        pub fn encrypt_secret(&self, account_id: i64, plain: &[u8]) -> Vec<u8>;
        pub fn decrypt_secret(&self, account_id: i64, stored: &[u8])
            -> Result<Vec<u8>, SecretError>;
    }
    pub enum StoreError {
        Key(KeyFileError),
        Io { path: PathBuf, source: io::Error },
        Database(sqlx::Error),
        Migrate(sqlx::migrate::MigrateError),
    }
    ```

  - `time.rs`: `pub fn to_db(t: DateTime<Utc>) -> String` and
    `pub fn from_db(s: &str) -> Option<DateTime<Utc>>`.
  - A test helper reused by Tasks 3 and 4:
    `pub(crate) async fn test_store() -> (Store, TempDir)`, under
    `#[cfg(test)]`.

- [ ] **Step 1: Write the failing tests** (`store.rs` tests,
  `#[tokio::test]`):
  - `open_creates_database_0600`: database file mode `0o600`; after a
    write, the `-wal` file exists with mode `0o600`.
  - `existing_file_opens`: a file pre-created with mode `0644` opens and
    migrates.
  - `open_twice_is_a_no_op`: open, close, open again; one row per
    migration in `_sqlx_migrations`.
  - `pragmas`: on both pools `PRAGMA journal_mode` = `wal`,
    `PRAGMA foreign_keys` = `1`, `PRAGMA busy_timeout` = `5000`,
    `PRAGMA synchronous` = `1` (NORMAL).
  - `read_pool_cannot_write`: `INSERT INTO accounts …` on `read()` fails;
    the same on `write()` succeeds.
  - `secret_round_trip_through_store`.
  - `bad_key_file_stops_open`: a 31-byte key file →
    `StoreError::Key(WrongLength { .. })`.
  - `time.rs`: `to_db` of 2026-11-25 08:30:00.123 UTC is
    `"2026-11-25T08:30:00.123Z"`; `from_db(to_db(t)) == Some(t)` at
    millisecond precision; `from_db("2026-11-25 08:30")` is `None`.

- [ ] **Step 2: Run them to verify they fail**

  Run: `cd server && cargo test -p sqlite -- store time`
  Expected: compile errors.

- [ ] **Step 3: Implement.** In `open`: load the key first; create the
  database file with `create_new(true).mode(0o600)` when absent; build
  `SqliteConnectOptions` with `journal_mode(Wal)`,
  `synchronous(Normal)`, `foreign_keys(true)`,
  `busy_timeout(5 s)`; write pool `max_connections(1)`; run
  `sqlx::migrate!("./migrations")` on it; read pool from the same
  options plus `.pragma("query_only", "ON")` (`journal_mode` is stored
  in the file: if setting it on a `query_only` connection fails, leave it
  out of the read options, the `pragmas` test still holds). `close` runs
  `PRAGMA optimize` on the write pool, then closes both. `accounts` in
  `0001_initial.sql`: columns and `UNIQUE(provider, email)` from the
  spec, `credentials BLOB` nullable, `created_at NOT NULL`, the others
  nullable, `CHECK (provider IN ('gmail','outlook','imap'))`,
  `CHECK (email = lower(email))`.

- [ ] **Step 4: Run the tests and the checks**

  Run: `cd server && cargo test -p sqlite && mise run ci`
  Expected: all pass.

- [ ] **Step 5: Commit**

  ```bash
  git add server
  git commit -m "feat(sqlite): open the store and run migrations" -m "Refs: #11"
  ```

### Task 3: Phase 1 tables and constraints

**Files:**

- Modify: `server/crates/adapters/sqlite/migrations/0001_initial.sql`
- Create: `server/crates/adapters/sqlite/src/schema_tests.rs`
  (`#[cfg(test)]` module declared in `lib.rs`)

**Interfaces:**

- Consumes: `test_store()` (Task 2).
- Produces: tables `tags`, `mails`, `trusted_senders`, `muted_threads`,
  `sync_state`, `audit`, with the columns, nullability and indexes of the
  spec. Exact choices the spec leaves open:
  - Every column the spec does not call nullable is `NOT NULL`: in
    `mails`, `account_id`, `provider_id`, `received_at`, `location`,
    `source`, the booleans and the token counts; `body` is nullable
    (not fetched yet).
  - `tags`: `name`, `name_folded`, `description`, `color`, `created_at`
    `NOT NULL`; `CHECK (color GLOB '#[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]')`.
  - `trusted_senders`: `CHECK (pattern = lower(pattern))`, as
    `accounts.email`.
  - `mails.category`, `priority`, `summary`, `reason`, `classified_at`,
    `model`, `billing`: nullable; `input_tokens`, `output_tokens`:
    `NOT NULL DEFAULT 0`; booleans `NOT NULL DEFAULT 0` with
    `CHECK (x IN (0,1))`, except `is_read` (nullable).
  - `mails.sender`, `message_key`, `thread_id`, `sender_name`: nullable;
    `subject` `NOT NULL DEFAULT ''`; `recipients` `NOT NULL DEFAULT '[]'`.
  - `sync_state`: `folder_path TEXT NOT NULL DEFAULT ''` (a `NULL` in a
    primary key would allow duplicates), primary key `(account_id,
    location, folder_path)`, `CHECK ((location = 'folder') =
    (folder_path <> ''))`.
  - `mails`: `CHECK ((location = 'folder') = (folder_path IS NOT
    NULL))`, `CHECK ((review IS NULL) = (review_at IS NULL))`,
    `CHECK (spam_confidence BETWEEN 0 AND 100)`,
    `CHECK (length(body) <= 50000)`.
  - `audit.resource`, `audit.action`: `NOT NULL CHECK (x <> '')`.

- [ ] **Step 1: Write the failing tests** in `schema_tests.rs`:
  - `every_provider_code_is_accepted`: insert one account per
    `ProviderKind` using `as_code()` (add `domain` as a dev-dependency);
    `"yahoo"` is rejected.
  - `every_priority_code_is_accepted`: same with `Priority::as_code()`.
  - `unknown_codes_are_rejected`: one bad value each for location,
    source, review, billing, audit actor; tag color `red` and `#ABCDEF`
    rejected, `#abcdef` accepted.
  - `folder_path_only_with_folder_location`: `location = 'inbox'` with a
    path rejected; `'folder'` without a path rejected; `'folder'` with
    `Clients/Acme` accepted. Same pair for `sync_state`.
  - `review_and_review_at_go_together`.
  - `spam_confidence_bounds`: 0 and 100 accepted, -1 and 101 rejected.
  - `body_cut_enforced`: 50,000 `é` accepted (characters, not bytes),
    50,001 rejected.
  - `duplicate_provider_id_rejected`: same `(account_id, provider_id)`
    twice.
  - `spam_without_sender_or_message_key`: `sender`, `message_key`,
    `thread_id` all `NULL` → inserted.
  - `tag_names_unique_when_folded`: `name_folded` `école` twice
    rejected; `ecole` accepted beside `école`.
  - `account_delete_cascades`: delete an account → its mails,
    muted threads and sync state are gone; its audit rows stay.
  - `foreign_keys_enforced`: a mail with an unknown `account_id` is
    rejected.

- [ ] **Step 2: Run them to verify they fail**

  Run: `cd server && cargo test -p sqlite schema_tests`
  Expected: FAIL (`no such table: mails`…).

- [ ] **Step 3: Write the tables** in `0001_initial.sql`, with the
  indexes of the spec (`(account_id, message_key) WHERE message_key IS
  NOT NULL`, `(account_id, thread_id, received_at)`,
  `(location, received_at)`, `(received_at)`, `(sender COLLATE NOCASE)`,
  `(category)`; audit: `at`, `(resource, resource_id)`,
  `(actor, actor_id)`, `action`).

- [ ] **Step 4: Run the tests and the checks**

  Run: `cd server && cargo test -p sqlite && mise run ci`
  Expected: all pass.

- [ ] **Step 5: Commit**

  ```bash
  git add server
  git commit -m "feat(sqlite): add the phase 1 tables" -m "Refs: #11"
  ```

### Task 4: Full-text search kept by triggers

**Files:**

- Modify: `server/crates/adapters/sqlite/migrations/0001_initial.sql`
- Create: `server/crates/adapters/sqlite/src/fts_tests.rs` (`#[cfg(test)]`)
- Modify: `server/crates/adapters/sqlite/README.md` (search section: the
  indexed columns, accent folding, triggers)

**Interfaces:**

- Consumes: `test_store()` (Task 2), `mails` (Task 3).
- Produces: `mails_fts` (FTS5, `content = 'mails'`,
  `content_rowid = 'id'`, columns `subject, sender, sender_name, body,
  summary`, `tokenize = 'unicode61 remove_diacritics 2'`) and triggers
  `mails_fts_insert`, `mails_fts_delete`, `mails_fts_update` (`AFTER
  UPDATE OF subject, sender, sender_name, body, summary`: the `'delete'`
  command with the old values, then an insert with the new ones).

- [ ] **Step 1: Write the failing tests** in `fts_tests.rs`, each
  querying `SELECT rowid FROM mails_fts WHERE mails_fts MATCH ?`:
  - `accents_are_ignored`: subject `Équipe projet` found by `equipe`,
    and subject `equipe` found by `équipe`.
  - `summary_update_is_indexed`: insert with summary `alpha`; update to
    `bravo`; `bravo` matches, `alpha` does not.
  - `non_indexed_update_keeps_matches`: update `is_read` only; the mail
    still matches its subject.
  - `deleted_mail_leaves_the_index`.
  - `cascade_leaves_the_index`: delete the account; no match remains
    for its mails.
  - `null_columns_are_indexed_safely`: a mail with `sender`,
    `sender_name`, `summary` `NULL` matches by subject; deleting it
    leaves no match (the `'delete'` command with `NULL`s).
  - `integrity_check_passes`: after the operations above,
    `INSERT INTO mails_fts(mails_fts, rank) VALUES('integrity-check', 1)`
    succeeds (`rank = 1` also compares the index with the `mails`
    rows).

- [ ] **Step 2: Run them to verify they fail**

  Run: `cd server && cargo test -p sqlite fts_tests`
  Expected: FAIL (`no such table: mails_fts`).

- [ ] **Step 3: Add the table and the triggers** to `0001_initial.sql`;
  update the crate README.

- [ ] **Step 4: Run the tests and the checks**

  Run: `cd server && cargo test -p sqlite && mise run ci`
  Expected: all pass.

- [ ] **Step 5: Commit**

  ```bash
  git add server
  git commit -m "feat(sqlite): index mails for accent-insensitive search" -m "Closes: #11"
  ```
