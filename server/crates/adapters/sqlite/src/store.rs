//! The store: one SQLite file, a single-connection write pool, a
//! `query_only` read pool, migrations run at open, and the secret key.

use std::fs::OpenOptions;
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use thiserror::Error;

use crate::secret::{self, KeyFileError, SecretError, SecretKey};

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Where the store keeps its files, as values read by `app`.
#[derive(Clone, Debug)]
pub struct StoreConfig {
    pub database: PathBuf,
    pub key_file: PathBuf,
}

pub struct Store {
    write: SqlitePool,
    read: SqlitePool,
    key: SecretKey,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Key(#[from] KeyFileError),
    #[error("database_io: {path}")]
    Io { path: PathBuf, source: io::Error },
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

impl Store {
    /// Loads (or creates) the key, creates the database file with mode
    /// `0600` when missing, runs the pending migrations and opens both
    /// pools.
    pub async fn open(config: StoreConfig) -> Result<Store, StoreError> {
        let key = secret::load_or_create_key(&config.key_file)?;
        create_private_file(&config.database).map_err(|source| StoreError::Io {
            path: config.database.clone(),
            source,
        })?;
        let options = SqliteConnectOptions::new()
            .filename(&config.database)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(BUSY_TIMEOUT)
            // A row deleted by an `OR REPLACE` conflict fires the delete
            // triggers only with this on: without it the search index
            // keeps the replaced mail.
            .pragma("recursive_triggers", "ON");
        let write = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options.clone())
            .await?;
        sqlx::migrate!("./migrations").run(&write).await?;
        let read = SqlitePoolOptions::new()
            .connect_with(options.pragma("query_only", "ON"))
            .await?;
        Ok(Store { write, read, key })
    }

    /// Lets SQLite refresh its statistics, then closes both pools.
    pub async fn close(&self) {
        if let Err(err) = sqlx::query("PRAGMA optimize").execute(&self.write).await {
            tracing::warn!(error = %err, "PRAGMA optimize failed");
        }
        self.read.close().await;
        self.write.close().await;
    }

    /// Encrypts an account secret (opaque bytes) for storage.
    pub fn encrypt_secret(&self, account_id: i64, plain: &[u8]) -> Vec<u8> {
        secret::encrypt(&self.key, account_id, plain)
    }

    /// Decrypts a stored account secret; an error means the account must
    /// be reconnected.
    pub fn decrypt_secret(&self, account_id: i64, stored: &[u8]) -> Result<Vec<u8>, SecretError> {
        secret::decrypt(&self.key, account_id, stored)
    }

    #[allow(dead_code)]
    pub(crate) fn write(&self) -> &SqlitePool {
        &self.write
    }

    #[allow(dead_code)]
    pub(crate) fn read(&self) -> &SqlitePool {
        &self.read
    }
}

/// Creates `path` with mode `0600` if it does not exist; an existing file
/// is left as it is.
fn create_private_file(path: &Path) -> io::Result<()> {
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
pub(crate) async fn test_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().unwrap();
    let store = Store::open(config(&dir)).await.unwrap();
    (store, dir)
}

#[cfg(test)]
fn config(dir: &tempfile::TempDir) -> StoreConfig {
    StoreConfig {
        database: dir.path().join("mailbox.db"),
        key_file: dir.path().join("secret.key"),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;

    const ACCOUNT: &str = "INSERT INTO accounts (provider, email, created_at) \
                           VALUES ('gmail', 'alice@example.com', '2026-11-25T08:30:00.000Z')";

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[tokio::test]
    async fn open_creates_database_0600() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);
        let store = Store::open(cfg.clone()).await.unwrap();
        assert_eq!(mode(&cfg.database), 0o600);
        sqlx::query(ACCOUNT).execute(store.write()).await.unwrap();
        let wal = dir.path().join("mailbox.db-wal");
        assert_eq!(mode(&wal), 0o600);
        store.close().await;
    }

    #[tokio::test]
    async fn existing_file_opens() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);
        fs::write(&cfg.database, b"").unwrap();
        fs::set_permissions(&cfg.database, fs::Permissions::from_mode(0o644)).unwrap();
        let store = Store::open(cfg).await.unwrap();
        sqlx::query(ACCOUNT).execute(store.write()).await.unwrap();
        store.close().await;
    }

    #[tokio::test]
    async fn open_twice_is_a_no_op() {
        let dir = TempDir::new().unwrap();
        Store::open(config(&dir)).await.unwrap().close().await;
        let store = Store::open(config(&dir)).await.unwrap();
        let applied: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(store.read())
            .await
            .unwrap();
        let embedded = sqlx::migrate!("./migrations").iter().count() as i64;
        assert_eq!(applied, embedded);
        store.close().await;
    }

    #[tokio::test]
    async fn pragmas() {
        let (store, _dir) = test_store().await;
        assert_eq!(store.write().options().get_max_connections(), 1);
        for pool in [store.write(), store.read()] {
            let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
                .fetch_one(pool)
                .await
                .unwrap();
            assert_eq!(journal, "wal");
            for (pragma, expected) in [
                ("foreign_keys", 1),
                ("busy_timeout", 5000),
                ("synchronous", 1),
            ] {
                let value: i64 = sqlx::query_scalar(&format!("PRAGMA {pragma}"))
                    .fetch_one(pool)
                    .await
                    .unwrap();
                assert_eq!(value, expected, "{pragma}");
            }
        }
        store.close().await;
    }

    #[tokio::test]
    async fn read_pool_cannot_write() {
        let (store, _dir) = test_store().await;
        assert!(sqlx::query(ACCOUNT).execute(store.read()).await.is_err());
        sqlx::query(ACCOUNT).execute(store.write()).await.unwrap();
        store.close().await;
    }

    #[tokio::test]
    async fn secret_round_trip_through_store() {
        let (store, _dir) = test_store().await;
        let stored = store.encrypt_secret(3, b"refresh-token");
        assert_eq!(store.decrypt_secret(3, &stored).unwrap(), b"refresh-token");
        assert_eq!(
            store.decrypt_secret(4, &stored),
            Err(SecretError::Undecryptable)
        );
        store.close().await;
    }

    #[tokio::test]
    async fn bad_key_file_stops_open() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);
        fs::write(&cfg.key_file, [0u8; 31]).unwrap();
        fs::set_permissions(&cfg.key_file, fs::Permissions::from_mode(0o600)).unwrap();
        match Store::open(cfg).await {
            Err(StoreError::Key(KeyFileError::WrongLength { len: 31, .. })) => {}
            Err(other) => panic!("{other:?}"),
            Ok(_) => panic!("opened"),
        }
    }
}
