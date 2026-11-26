# sqlite

The SQLite adapter of the server: it opens the database, runs the
migrations and encrypts the account secrets. It implements the
repositories of `ports`, added with the use cases that need them (so far
`DeviceRepository`; accounts, mails, tags, trusted senders and audit
follow). Only this crate and `app` know `sqlx`.

## Usage

```rust
use sqlite::{Store, StoreConfig};

let store = Store::open(StoreConfig {
    database: data_dir.join("mailbox.db"),
    key_file: data_dir.join("secret.key"),
})
.await?;
// …
store.close().await;
```

`app` builds `StoreConfig` from its configuration; the crate reads no
environment variable.

## Connection

- One file. Two pools on it: a write pool of a single connection, so
  the server's own writes never compete for the lock, and a read pool
  with `PRAGMA query_only = ON`.
- Every connection: `journal_mode = WAL`, `synchronous = NORMAL`,
  `foreign_keys = ON`, `recursive_triggers = ON`, `busy_timeout` of 5
  seconds.
- Never `INSERT OR REPLACE` into `accounts`: the replaced row gets a new
  id and its mails, muted threads and sync state go with it by cascade.
  Use `INSERT … ON CONFLICT … DO UPDATE` instead, everywhere.
- The database file is created with mode `0600`; SQLite gives the
  `-wal` and `-shm` files the same mode. An existing file keeps its mode.
- `Store::close(&self)` runs `PRAGMA optimize`, then closes both pools.

## Migrations

- `migrations/NNNN_<name>.sql`, embedded in the binary and run by
  `Store::open` before anything else. sqlx records them in
  `_sqlx_migrations`.
- A migration that has reached `main` is never edited: a change is a new
  file. A migration that is not purely additive carries its data
  migration in the same file.
- Each phase adds the tables it needs with its own migration.
- `build.rs` makes cargo rebuild the crate when a migration file is
  added, since `sqlx::migrate!` does not watch the directory on stable.

## Stored values

- Timestamps: TEXT, UTC, fixed-width RFC 3339 with milliseconds
  (`2026-11-25T08:30:00.000Z`, see `sqlite::time`), so text order is
  time order. The code writes every timestamp from its clock; SQL never
  calls `now`.
- Booleans: INTEGER 0 or 1. Enums: the domain's stable codes, checked by
  `CHECK` constraints. Lists: JSON arrays in TEXT.
- No user-facing text: errors and audit entries hold codes.

## Devices

- `impl DeviceRepository for Store`: tables `devices` and
  `pairing_codes`, which hold only hashes (token, code), never a secret.
- `redeem_pairing_code` is one transaction on the single-connection
  write pool: the `UPDATE pairing_codes SET used_at …` guarded by
  `used_at IS NULL AND expires_at > now` gates the device insert, so two
  concurrent redemptions yield one device and a failed insert leaves the
  code unused.
- `insert_pairing_code` first deletes the codes that are used or expired.
- A stored name or date that no longer parses is a `RepositoryError`.

## Search

- `mails_fts` is an FTS5 index over the `subject`, `sender`,
  `sender_name`, `body` and `summary` of `mails`, with the `unicode61`
  tokenizer and `remove_diacritics 2`: `equipe` finds `Équipe` and the
  reverse.
- External content: the text is stored once, in `mails`. Triggers on
  insert, delete and update of those columns keep the index in step, so
  no code path has to. A cascade from `accounts` cleans it too, and so
  does a row replaced by `OR REPLACE` (through `recursive_triggers`).
- Query it with `SELECT rowid FROM mails_fts WHERE mails_fts MATCH ?`;
  the rowid is `mails.id`.

## Account secrets

- Each secret is stored as `[version 1][24-byte nonce][ciphertext]`,
  encrypted with XChaCha20-Poly1305 and the account id as associated
  data: a secret copied onto another account's row does not decrypt.
- The 32-byte key lives in its own file (`StoreConfig::key_file`). It is
  created with mode `0600` when missing; `Store::open` refuses a file of
  another length or readable by group or others.
- `Store::encrypt_secret` and `Store::decrypt_secret` take opaque bytes:
  the provider adapters serialize their own tokens or passwords. A
  secret that does not decrypt means that account must be reconnected;
  the others keep working.
