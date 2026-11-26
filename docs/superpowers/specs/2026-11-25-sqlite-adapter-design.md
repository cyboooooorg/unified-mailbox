# SQLite adapter design

Issue: #11. Related: #10 (domain model), #12 (configuration), #17
(accounts), #18 (sync loop), #24 (trusted senders), #25 (tags), #26
(audit), #62 (internationalization).

## Intent

Give the server one place to store what it knows: the accounts and their
secrets, the mail index with Claude's verdicts, the tags, the trusted
senders and the audit log. This change lays the storage foundation
(connection, migrations, secret encryption, the phase 1 schema, full-text
search). The repositories come later, each with the feature that first
needs it, so that no port is designed before a use case calls it.

## Goals

- A `sqlite` adapter crate that opens the database with the right
  settings and runs versioned migrations at startup.
- The phase 1 schema in one initial migration, with constraints that keep
  invalid values out (stable codes, foreign keys).
- Full-text search over mails that ignores accents and stays in step
  with the mails table without application code.
- Account secrets (OAuth tokens, IMAP passwords) encrypted at rest, with
  a key kept outside the database file.
- Tests at the SQL level that pin each of the above.

## Non-goals

- Port traits and repository implementations: each comes with its use
  case (#17 accounts, #18 sync state, #24 trusted senders, #25 tags,
  #26 audit, and the mail repository with the sync loop).
- Tables of later phases (rules, history processing, snooze, follow-ups,
  privacy requests): each phase adds its own migration.
- Seeding the default tags: #25 owns the defaults.
- Backups, restore and key rotation: phase 6.
- Importing a Mailbot database: there is no data to migrate (#60).
- Reading environment variables: #12 does it in `app` and passes values.
- Tables owned by other phase 0 issues, each added by its own migration:
  device tokens (#13), jobs (#14). Dry-run and the other settings come
  from the configuration (#12), not from the database.

## Anti-goals

- An account secret readable from the database file or a copy of it
  alone.
- A failed decryption that stops the server: one account needs
  reconnecting, the others keep running.
- Search results that miss a mail because its subject or summary changed
  after insertion (a gap in Mailbot, whose index was synced by hand).
- User-facing text stored by the server: columns hold codes (#62).

## Constraints

- Hexagonal layering: only this crate and `app` know `sqlx`. The domain
  stays free of storage concerns.
- The repository is public: tests use invented data only.
- Mailbot's behavior is the reference (`app/db.py`); deviations are
  listed below.

## Approaches

1. **Foundation and phase 1 schema now, repositories with each feature**
   (chosen by the user). The schema is designed in one go, which matters
   most for `mails` and its search index; ports appear only when a use
   case calls them.
2. Foundation, schema and every phase 1 repository now. Rejected: the
   traits would be guessed without callers and likely reworked by #17,
   #18 and #25.
3. Foundation only, each feature adds its tables. Rejected: the hardest
   part, `mails` and full-text search, would be designed piecemeal inside
   the sync loop.

Secrets at rest (chosen by the user): an encrypted column with a key in
a separate file. Plain text with a `0600` file (Mailbot) was rejected
because a copied database or backup would expose every token; the OS
keyring was rejected because the server runs headless.

## Design

### Crate and connection

- Crate `server/crates/adapters/sqlite`, migrations in its `migrations/`
  directory, embedded with `sqlx::migrate!` and run by `open` before
  anything else.
- `open(config) -> Store`, where the config carries the database path and
  the key file path as values (wired from the environment by #12).
- Two pools on the same file, as in Mailbot:
  - a write pool of one connection, so writes are serialized and never
    fail on a busy lock between the server's own tasks;
  - a read pool with `PRAGMA query_only = ON`.
- Every connection: `journal_mode = WAL`, `synchronous = NORMAL`,
  `foreign_keys = ON`, `busy_timeout` of 5 seconds.
- The database file is created with mode `0600` (SQLite gives the `-wal`
  and `-shm` files the same mode).
- `PRAGMA optimize` when the store closes.
- Surface (builder's proposal, accepted): `Store` owns both pools and the
  secret cipher. The pools stay private to the crate, where the
  repositories will live; `Store` exposes `encrypt_secret(account_id,
  bytes)` and `decrypt_secret(account_id, bytes)` on opaque bytes, so the
  adapter knows nothing of OAuth tokens or IMAP passwords (the provider
  adapters serialize them).

### Migrations

- Numbered files (`0001_initial.sql`, `0002_…`), tracked by sqlx in
  `_sqlx_migrations`. An applied migration is never edited: a change is a
  new file. Mailbot's add-missing-columns at startup is not ported.
- A migration that is not purely additive carries its data migration in
  the same file.

### Stored values

- Timestamps: TEXT, UTC, fixed-width RFC 3339 with milliseconds
  (`2026-11-25T08:30:00.000Z`), so text order is time order.
- Booleans: INTEGER 0/1 with a `CHECK`.
- Enums: the domain's stable codes (`ProviderKind::as_code`, priority,
  location), with a `CHECK` listing the allowed codes.
- Lists (recipients): a JSON array in TEXT.
- Every timestamp is written by Rust code from the `Clock` port, never by
  SQLite (`now`, `CURRENT_TIMESTAMP`), so tests control time. This is why
  `review_at` is not set by a trigger (see `mails`).
- Code lists live in this crate: it maps domain types to codes and back,
  as `http` maps them to DTOs. `CHECK` lists cover the codes that are
  settled (provider, location, source, review, priority, billing, audit
  actor). Audit `resource` and `action` are free non-empty text until #26
  designs them; #26 adds their `CHECK` with its migration.
- Case: SQLite's `lower()` and `NOCASE` fold ASCII only, so the database
  does not fold. Values the domain lowercases (`accounts.email`,
  `trusted_senders.pattern`) are stored lowercased by the code; uniqueness
  that ignores case uses a column the code fills with the folded value.

### Tables of `0001_initial.sql`

- `accounts`: `id`, `provider` (code), `email` (lowercased),
  `credentials` BLOB, nullable (`NULL` means the account must be
  reconnected, instead of Mailbot's empty string), `created_at`,
  `last_sync_at`, `last_error` (an error code, not a sentence).
  `UNIQUE(provider, email)`.
- `tags`: `key` primary key, `name` (as typed), `name_folded` (the name
  lowercased by Rust, accents kept, `UNIQUE`: the domain's uniqueness
  rule), `description`, `color` (`#rrggbb`), `created_at`.
- `mails`:
  - identity: `id`, `account_id` (foreign key, `ON DELETE CASCADE`),
    `provider_id`, `message_key` (the Message-ID, stable across moves:
    Outlook ids change on every move; nullable, since some mails have
    none), `thread_id` (nullable);
    `UNIQUE(account_id, provider_id)`;
  - headers: `received_at`, `sender` (nullable: the From header of spam
    often holds no address that parses, and such a mail is still stored
    and sorted), `sender_name`, `recipients`, `subject`, `dmarc_passed`;
  - content: `body`, the text part cut to 50,000 characters (Mailbot's
    `MAX_STORED_BODY`);
  - state in the mailbox: `location` (code), `folder_path` (set only when
    `location` is `folder`, enforced by a `CHECK`), `source` (`inbox` or
    `junk`, where the mail was first seen), `is_read` (nullable: unknown),
    `locked`, `locked_remote` (the lock last seen in the mailbox; the
    mailbox wins when it differs), `gone_since`;
  - junk review: `review` (`rescued` or `confirmed`, nullable) and
    `review_at` (the confirmed-spam purge delay counts from it). Mailbot
    set `review_at` with a trigger; here the code writes it from the
    `Clock` in the same statement as `review`, so the purge delay is
    testable. Rule, enforced by a `CHECK`: `review_at` is set if and only
    if `review` is; setting the same review again keeps its time;
  - classification: `category` (a tag key, `unknown` for
    `Category::Unknown`, `NULL` when not classified), `priority`,
    `needs_reply`, `is_phishing`, `spam_confidence` (0 to 100),
    `summary`, `reason`, `classified_at`, `model`, `input_tokens`,
    `output_tokens`, `billing` (`api`, `batch` or `subscription`, which
    replaces Mailbot's `batched` and `subscription` flags).
  - `category` has no foreign key to `tags`, as in Mailbot: deleting a tag
    re-files its mails in the use case (#25), not by cascade.
  - Indexes: `(account_id, message_key)` where `message_key` is not
    null, `(account_id, thread_id,
    received_at)`, `(location, received_at)`, `(received_at)`,
    `(sender COLLATE NOCASE)`, `(category)`.
- `mails_fts`: FTS5, external content on `mails` (`content_rowid = id`),
  columns `subject`, `sender`, `sender_name`, `body`, `summary`,
  `tokenize = 'unicode61 remove_diacritics 2'`. Kept in step by triggers
  on insert, delete and update of those columns, so no code path can
  forget it.
- `trusted_senders`: `pattern` primary key, `created_at`.
- `muted_threads`: `(account_id, thread_id)` primary key, `created_at`,
  with a foreign key to `accounts` (`ON DELETE CASCADE`). Mailbot left
  these rows behind when an account was removed.
- `sync_state`: `(account_id, location, folder_path)` key, `days`
  (the window already fully indexed), foreign key with cascade. #18 may
  extend it with its own migration.
- `audit`: `id` (autoincrement), `at`, `actor` (code: `user`, `device`,
  `system`, `claude`, `rule`), `actor_id`, `resource` (code),
  `resource_id`, `account_id` (no foreign key: the log outlives the
  account), `action` (code), `changes` (JSON `{field: [before, after]}`),
  `note`. Indexes on `at`, `(resource, resource_id)`, `(actor,
  actor_id)`, `action`. Mailbot's French labels and its `search` column
  are not ported: the client translates codes (#62), and #26 decides how
  the log is searched.

### Secret encryption

- XChaCha20-Poly1305 (RustCrypto `chacha20poly1305`), key of 32 bytes
  held in a `zeroize`d type.
- Stored value: a format version byte, the 24-byte random nonce, then
  the ciphertext. The account id is the associated data, so a secret
  copied onto another account's row fails to decrypt. An account row is
  inserted first, then its secret written in the same transaction.
- The key file:
  - path given by the config;
  - missing at startup: generated from the OS random source, written with
    mode `0600`, and its creation logged (never its content);
  - present but not exactly 32 bytes, or readable by group or others:
    the server refuses to start, with an error naming the path.
- A secret that fails to decrypt (key changed, row tampered with) gives
  an error for that account only; the caller treats the account as to
  reconnect.
- The README documents the key file: where it is, that it must be backed
  up apart from the database, and that losing it means reconnecting every
  account.

### Tests

- Migrations run on an empty database; running `open` twice is a no-op.
- Foreign keys enforced; deleting an account removes its mails, muted
  threads and sync state, and their search rows.
- Search: `equipe` finds a mail whose subject is `Équipe`; updating a
  summary updates the matches; deleting a mail removes it from results.
- `CHECK` rejects a `review` without `review_at` and the reverse.
- `CHECK` constraints reject an unknown code and a `folder_path` on a
  non-folder location.
- The read pool cannot write.
- Encryption: round trip; wrong account id fails; wrong key fails;
  missing key file is generated with mode `0600`; a short key file and a
  group-readable one are refused; the database file is `0600`.

### Documentation

- `server/crates/adapters/sqlite/README.md`: purpose, the migration
  rules, the stored value formats, the key file.
- Root README: the key file and its backup, with the database location.
  The environment variables themselves are documented by #12.

## Deviations from Mailbot

- Versioned migrations instead of adding missing columns at startup.
- Search index kept by triggers instead of by hand.
- Secrets encrypted instead of in clear text; `NULL` instead of an empty
  string for "reconnect".
- Codes instead of French labels in the audit log; no `search` column.
- `muted_threads` cascades with its account.
- `billing` replaces the `batched` and `subscription` flags.
- `review_at` written by the code from the `Clock`, not by a trigger.
- Tag name uniqueness through a folded column, not `NOCASE`, which
  ignores accented capitals.

## Left to the builder

- Names of Rust types and functions, module layout, error enum variants.
- Exact SQL of indexes and triggers, as long as the tests above hold.
- Whether the two pools sit behind one `Store` type or two.
- The test layout (in-crate tests on a temporary directory).
