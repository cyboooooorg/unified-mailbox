//! `SettingsRepository` over SQLite: key/value settings.

use chrono::{DateTime, Utc};
use domain::Language;
use ports::{RepositoryError, SettingsRepository};

use crate::store::Store;
use crate::time::to_db;

fn fail(err: impl std::error::Error + Send + Sync + 'static) -> RepositoryError {
    RepositoryError(Box::new(err))
}

fn corrupt(what: &str) -> RepositoryError {
    RepositoryError(format!("stored {what} is invalid").into())
}

#[async_trait::async_trait]
impl SettingsRepository for Store {
    async fn language(&self) -> Result<Option<Language>, RepositoryError> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT value FROM settings WHERE key = 'language'")
                .fetch_optional(self.read())
                .await
                .map_err(fail)?;
        value
            .map(|code| Language::from_code(&code).ok_or_else(|| corrupt("language")))
            .transpose()
    }

    async fn set_language(
        &self,
        language: Language,
        at: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ('language', ?, ?) \
             ON CONFLICT (key) DO UPDATE SET value = excluded.value, \
             updated_at = excluded.updated_at",
        )
        .bind(language.as_code())
        .bind(to_db(at))
        .execute(self.write())
        .await
        .map_err(fail)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};

    use super::*;
    use crate::store::test_store;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 27, 9, 0, 0).unwrap()
    }

    #[tokio::test]
    async fn language_is_none_until_set() {
        let (store, _dir) = test_store().await;
        assert_eq!(store.language().await.unwrap(), None);
    }

    #[tokio::test]
    async fn set_then_get() {
        let (store, _dir) = test_store().await;
        store.set_language(Language::Fr, t0()).await.unwrap();
        assert_eq!(store.language().await.unwrap(), Some(Language::Fr));
    }

    #[tokio::test]
    async fn set_twice_keeps_the_last() {
        let (store, _dir) = test_store().await;
        store.set_language(Language::Fr, t0()).await.unwrap();
        store.set_language(Language::En, t0()).await.unwrap();
        assert_eq!(store.language().await.unwrap(), Some(Language::En));
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings WHERE key = 'language'")
            .fetch_one(store.read())
            .await
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[tokio::test]
    async fn set_stores_the_time() {
        let (store, _dir) = test_store().await;
        let later = t0() + Duration::minutes(5);
        store.set_language(Language::Fr, t0()).await.unwrap();
        store.set_language(Language::En, later).await.unwrap();
        let stored: String =
            sqlx::query_scalar("SELECT updated_at FROM settings WHERE key = 'language'")
                .fetch_one(store.read())
                .await
                .unwrap();
        assert_eq!(stored, to_db(later));
    }

    #[tokio::test]
    async fn invalid_stored_language_is_an_error() {
        let (store, _dir) = test_store().await;
        sqlx::query("INSERT INTO settings (key, value, updated_at) VALUES ('language', 'de', ?)")
            .bind(to_db(t0()))
            .execute(store.write())
            .await
            .unwrap();
        assert!(store.language().await.is_err());
    }
}
