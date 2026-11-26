//! `DeviceRepository` over SQLite: paired devices and pairing codes.

use chrono::{DateTime, Utc};
use domain::{Device, DeviceId, DeviceName};
use ports::{DeviceRepository, NewDevice, NewPairingCode, RepositoryError};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::store::Store;
use crate::time::{from_db, to_db};

const DEVICE_COLUMNS: &str = "id, name, created_at, last_seen_at, revoked_at";

fn fail(err: impl std::error::Error + Send + Sync + 'static) -> RepositoryError {
    RepositoryError(Box::new(err))
}

fn corrupt(what: &str) -> RepositoryError {
    RepositoryError(format!("stored {what} is invalid").into())
}

fn time(value: &str, what: &str) -> Result<DateTime<Utc>, RepositoryError> {
    from_db(value).ok_or_else(|| corrupt(what))
}

fn optional_time(
    value: Option<String>,
    what: &str,
) -> Result<Option<DateTime<Utc>>, RepositoryError> {
    value.map(|v| time(&v, what)).transpose()
}

fn device(row: &SqliteRow) -> Result<Device, RepositoryError> {
    let name: String = row.try_get("name").map_err(fail)?;
    Ok(Device {
        id: DeviceId::new(row.try_get("id").map_err(fail)?),
        name: DeviceName::new(&name).map_err(|_| corrupt("device name"))?,
        created_at: time(
            &row.try_get::<String, _>("created_at").map_err(fail)?,
            "created_at",
        )?,
        last_seen_at: optional_time(row.try_get("last_seen_at").map_err(fail)?, "last_seen_at")?,
        revoked_at: optional_time(row.try_get("revoked_at").map_err(fail)?, "revoked_at")?,
    })
}

#[async_trait::async_trait]
impl DeviceRepository for Store {
    async fn insert_pairing_code(&self, code: NewPairingCode) -> Result<(), RepositoryError> {
        let now = to_db(code.created_at);
        let mut tx = self.write().begin().await.map_err(fail)?;
        sqlx::query("DELETE FROM pairing_codes WHERE used_at IS NOT NULL OR expires_at <= ?")
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(fail)?;
        sqlx::query(
            "INSERT INTO pairing_codes (code_hash, created_at, expires_at, created_by) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(&code.code_hash)
        .bind(&now)
        .bind(to_db(code.expires_at))
        .bind(code.created_by.map(DeviceId::get))
        .execute(&mut *tx)
        .await
        .map_err(fail)?;
        tx.commit().await.map_err(fail)
    }

    async fn redeem_pairing_code(
        &self,
        code_hash: &str,
        now: DateTime<Utc>,
        new: NewDevice,
    ) -> Result<Option<Device>, RepositoryError> {
        let now = to_db(now);
        let mut tx = self.write().begin().await.map_err(fail)?;
        let used = sqlx::query(
            "UPDATE pairing_codes SET used_at = ? \
             WHERE code_hash = ? AND used_at IS NULL AND expires_at > ?",
        )
        .bind(&now)
        .bind(code_hash)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(fail)?;
        if used.rows_affected() != 1 {
            return Ok(None);
        }
        let row = sqlx::query(&format!(
            "INSERT INTO devices (name, token_hash, created_at) VALUES (?, ?, ?) \
             RETURNING {DEVICE_COLUMNS}"
        ))
        .bind(new.name.as_str())
        .bind(&new.token_hash)
        .bind(to_db(new.created_at))
        .fetch_one(&mut *tx)
        .await
        .map_err(fail)?;
        let created = device(&row)?;
        tx.commit().await.map_err(fail)?;
        Ok(Some(created))
    }

    async fn find_active_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<Device>, RepositoryError> {
        let row = sqlx::query(&format!(
            "SELECT {DEVICE_COLUMNS} FROM devices WHERE token_hash = ? AND revoked_at IS NULL"
        ))
        .bind(token_hash)
        .fetch_optional(self.read())
        .await
        .map_err(fail)?;
        row.as_ref().map(device).transpose()
    }

    async fn touch_last_seen(
        &self,
        id: DeviceId,
        at: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE devices SET last_seen_at = ? WHERE id = ?")
            .bind(to_db(at))
            .bind(id.get())
            .execute(self.write())
            .await
            .map_err(fail)?;
        Ok(())
    }

    async fn list_active(&self) -> Result<Vec<Device>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "SELECT {DEVICE_COLUMNS} FROM devices WHERE revoked_at IS NULL \
             ORDER BY created_at, id"
        ))
        .fetch_all(self.read())
        .await
        .map_err(fail)?;
        rows.iter().map(device).collect()
    }

    async fn revoke(&self, id: DeviceId, at: DateTime<Utc>) -> Result<bool, RepositoryError> {
        let done =
            sqlx::query("UPDATE devices SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL")
                .bind(to_db(at))
                .bind(id.get())
                .execute(self.write())
                .await
                .map_err(fail)?;
        Ok(done.rows_affected() == 1)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};

    use super::*;
    use crate::store::test_store;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 26, 9, 0, 0).unwrap()
    }

    fn hash(c: char) -> String {
        c.to_string().repeat(64)
    }

    fn code(c: char, created: DateTime<Utc>, by: Option<DeviceId>) -> NewPairingCode {
        NewPairingCode {
            code_hash: hash(c),
            created_at: created,
            expires_at: created + Duration::minutes(10),
            created_by: by,
        }
    }

    fn new_device(name: &str, token: char, created: DateTime<Utc>) -> NewDevice {
        NewDevice {
            name: DeviceName::new(name).unwrap(),
            token_hash: hash(token),
            created_at: created,
        }
    }

    async fn pair(store: &Store, name: &str, token: char, at: DateTime<Utc>) -> Device {
        let c = code(token.to_ascii_uppercase(), at, None);
        store.insert_pairing_code(c.clone()).await.unwrap();
        store
            .redeem_pairing_code(&c.code_hash, at, new_device(name, token, at))
            .await
            .unwrap()
            .unwrap()
    }

    async fn count(store: &Store, table: &str) -> i64 {
        sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(store.read())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn migration_creates_tables() {
        let (store, _dir) = test_store().await;
        for table in ["devices", "pairing_codes"] {
            assert_eq!(count(&store, table).await, 0, "{table}");
        }
    }

    #[tokio::test]
    async fn token_hash_unique() {
        let (store, _dir) = test_store().await;
        pair(&store, "Phone", 'a', t0()).await;
        let c = code('B', t0(), None);
        store.insert_pairing_code(c.clone()).await.unwrap();
        let second = store
            .redeem_pairing_code(&c.code_hash, t0(), new_device("Tablet", 'a', t0()))
            .await;
        assert!(second.is_err());
        // The failed redemption rolled back: the code is still usable.
        let retry = store
            .redeem_pairing_code(&c.code_hash, t0(), new_device("Tablet", 'c', t0()))
            .await
            .unwrap();
        assert!(retry.is_some());
    }

    #[tokio::test]
    async fn redeem_once() {
        let (store, _dir) = test_store().await;
        store
            .insert_pairing_code(code('A', t0(), None))
            .await
            .unwrap();
        let first = store
            .redeem_pairing_code(&hash('A'), t0(), new_device("Phone", 'a', t0()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.name.as_str(), "Phone");
        assert_eq!(first.created_at, t0());
        assert_eq!(first.last_seen_at, None);
        assert_eq!(first.revoked_at, None);
        let again = store
            .redeem_pairing_code(&hash('A'), t0(), new_device("Phone", 'b', t0()))
            .await
            .unwrap();
        assert_eq!(again, None);
        assert_eq!(count(&store, "devices").await, 1);
    }

    #[tokio::test]
    async fn redeem_expired_or_unknown() {
        let (store, _dir) = test_store().await;
        store
            .insert_pairing_code(code('A', t0(), None))
            .await
            .unwrap();
        let at_expiry = t0() + Duration::minutes(10);
        let expired = store
            .redeem_pairing_code(&hash('A'), at_expiry, new_device("Phone", 'a', at_expiry))
            .await
            .unwrap();
        assert_eq!(expired, None);
        let unknown = store
            .redeem_pairing_code(&hash('Z'), t0(), new_device("Phone", 'a', t0()))
            .await
            .unwrap();
        assert_eq!(unknown, None);
        assert_eq!(count(&store, "devices").await, 0);
        // One millisecond earlier still works.
        let before = at_expiry - Duration::milliseconds(1);
        let ok = store
            .redeem_pairing_code(&hash('A'), before, new_device("Phone", 'a', before))
            .await
            .unwrap();
        assert!(ok.is_some());
    }

    #[tokio::test]
    async fn concurrent_redeem_yields_one_device() {
        let (store, _dir) = test_store().await;
        store
            .insert_pairing_code(code('A', t0(), None))
            .await
            .unwrap();
        let code_hash = hash('A');
        let (a, b) = tokio::join!(
            store.redeem_pairing_code(&code_hash, t0(), new_device("One", 'a', t0())),
            store.redeem_pairing_code(&code_hash, t0(), new_device("Two", 'b', t0())),
        );
        let results = [a.unwrap(), b.unwrap()];
        assert_eq!(results.iter().filter(|r| r.is_some()).count(), 1);
        assert_eq!(store.list_active().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn insert_purges_used_and_expired() {
        let (store, _dir) = test_store().await;
        let later = t0() + Duration::minutes(10);
        let mid = t0() + Duration::minutes(9);
        // Expired at `later`, unused.
        store
            .insert_pairing_code(code('E', t0(), None))
            .await
            .unwrap();
        // Valid at `later`, unused.
        store
            .insert_pairing_code(code('V', mid, None))
            .await
            .unwrap();
        // Used.
        pair(&store, "Phone", 'a', mid).await;
        assert_eq!(count(&store, "pairing_codes").await, 3);
        store
            .insert_pairing_code(code('N', later, None))
            .await
            .unwrap();
        let left: Vec<String> =
            sqlx::query_scalar("SELECT code_hash FROM pairing_codes ORDER BY code_hash")
                .fetch_all(store.read())
                .await
                .unwrap();
        assert_eq!(left, vec![hash('N'), hash('V')]);
    }

    #[tokio::test]
    async fn find_skips_revoked() {
        let (store, _dir) = test_store().await;
        let d = pair(&store, "Phone", 'a', t0()).await;
        let found = store.find_active_by_token_hash(&hash('a')).await.unwrap();
        assert_eq!(found, Some(d.clone()));
        assert_eq!(
            store.find_active_by_token_hash(&hash('z')).await.unwrap(),
            None
        );
        assert!(store.revoke(d.id, t0()).await.unwrap());
        assert_eq!(
            store.find_active_by_token_hash(&hash('a')).await.unwrap(),
            None
        );
        assert!(!store.revoke(d.id, t0()).await.unwrap());
        assert!(!store.revoke(DeviceId::new(9999), t0()).await.unwrap());
        assert!(store.list_active().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn touch_and_list_order() {
        let (store, _dir) = test_store().await;
        let t1 = t0();
        let t2 = t0() + Duration::minutes(5);
        // Created out of order: ids ascend, created_at does not.
        let late = pair(&store, "Late", 'b', t2).await;
        let early = pair(&store, "Early", 'a', t1).await;
        let seen = t2 + Duration::minutes(1);
        store.touch_last_seen(early.id, seen).await.unwrap();
        let list = store.list_active().await.unwrap();
        let names: Vec<&str> = list.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["Early", "Late"]);
        assert_eq!(list[0].last_seen_at, Some(seen));
        assert_eq!(list[1].last_seen_at, None);
        assert_eq!(list[1].id, late.id);
    }

    #[tokio::test]
    async fn list_breaks_ties_by_id() {
        let (store, _dir) = test_store().await;
        let first = pair(&store, "First", 'a', t0()).await;
        let second = pair(&store, "Second", 'b', t0()).await;
        let ids: Vec<DeviceId> = store
            .list_active()
            .await
            .unwrap()
            .iter()
            .map(|d| d.id)
            .collect();
        assert_eq!(ids, [first.id, second.id]);
    }

    #[tokio::test]
    async fn created_by_kept() {
        let (store, _dir) = test_store().await;
        let d = pair(&store, "Phone", 'a', t0()).await;
        store
            .insert_pairing_code(code('B', t0(), Some(d.id)))
            .await
            .unwrap();
        let by: Option<i64> =
            sqlx::query_scalar("SELECT created_by FROM pairing_codes WHERE code_hash = ?")
                .bind(hash('B'))
                .fetch_one(store.read())
                .await
                .unwrap();
        assert_eq!(by, Some(d.id.get()));
    }

    #[tokio::test]
    async fn invalid_stored_rows_are_errors() {
        let (store, _dir) = test_store().await;
        sqlx::query("INSERT INTO devices (name, token_hash, created_at) VALUES ('  ', 'x', '2026-11-26T09:00:00.000Z')")
            .execute(store.write())
            .await
            .unwrap();
        assert!(store.find_active_by_token_hash("x").await.is_err());
        sqlx::query("UPDATE devices SET name = 'Phone', created_at = 'yesterday'")
            .execute(store.write())
            .await
            .unwrap();
        assert!(store.list_active().await.is_err());
    }
}
