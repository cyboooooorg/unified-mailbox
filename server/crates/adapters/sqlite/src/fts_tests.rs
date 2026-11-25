//! Full-text search over mails, kept in step by triggers.

use sqlx::SqlitePool;

use crate::schema_tests::mail;
use crate::store::test_store;

async fn search(pool: &SqlitePool, query: &str) -> Vec<i64> {
    sqlx::query_scalar("SELECT rowid FROM mails_fts WHERE mails_fts MATCH ? ORDER BY rowid")
        .bind(query)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn exec(pool: &SqlitePool, sql: &str) {
    sqlx::query(sql).execute(pool).await.unwrap();
}

async fn account(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO accounts (provider, email, created_at) \
         VALUES ('gmail', 'alice@example.com', '2026-11-25T08:30:00.000Z') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn integrity_check(pool: &SqlitePool) {
    exec(
        pool,
        "INSERT INTO mails_fts (mails_fts, rank) VALUES ('integrity-check', 1)",
    )
    .await;
}

#[tokio::test]
async fn accents_are_ignored() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let a = account(w).await;
    let accented = mail(w, a, &[("subject", "'Équipe projet'")]).await.unwrap();
    let plain = mail(w, a, &[("subject", "'equipe'")]).await.unwrap();
    assert_eq!(search(w, "equipe").await, vec![accented, plain]);
    assert_eq!(search(w, "équipe").await, vec![accented, plain]);
}

#[tokio::test]
async fn summary_update_is_indexed() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let a = account(w).await;
    let id = mail(w, a, &[("summary", "'alpha'")]).await.unwrap();
    exec(
        w,
        &format!("UPDATE mails SET summary = 'bravo' WHERE id = {id}"),
    )
    .await;
    assert_eq!(search(w, "bravo").await, vec![id]);
    assert!(search(w, "alpha").await.is_empty());
    integrity_check(w).await;
}

#[tokio::test]
async fn non_indexed_update_keeps_matches() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let a = account(w).await;
    let id = mail(w, a, &[("subject", "'Facture'")]).await.unwrap();
    exec(w, &format!("UPDATE mails SET is_read = 1 WHERE id = {id}")).await;
    assert_eq!(search(w, "facture").await, vec![id]);
}

#[tokio::test]
async fn deleted_mail_leaves_the_index() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let a = account(w).await;
    let id = mail(w, a, &[("subject", "'Facture'")]).await.unwrap();
    exec(w, &format!("DELETE FROM mails WHERE id = {id}")).await;
    assert!(search(w, "facture").await.is_empty());
    integrity_check(w).await;
}

#[tokio::test]
async fn cascade_leaves_the_index() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let a = account(w).await;
    mail(w, a, &[("subject", "'Facture'")]).await.unwrap();
    exec(w, &format!("DELETE FROM accounts WHERE id = {a}")).await;
    assert!(search(w, "facture").await.is_empty());
    integrity_check(w).await;
}

#[tokio::test]
async fn null_columns_are_indexed_safely() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let a = account(w).await;
    let id = mail(
        w,
        a,
        &[
            ("subject", "'Gagnez'"),
            ("sender", "NULL"),
            ("sender_name", "NULL"),
            ("summary", "NULL"),
            ("message_key", "NULL"),
        ],
    )
    .await
    .unwrap();
    assert_eq!(search(w, "gagnez").await, vec![id]);
    exec(w, &format!("DELETE FROM mails WHERE id = {id}")).await;
    assert!(search(w, "gagnez").await.is_empty());
    integrity_check(w).await;
}

#[tokio::test]
async fn every_indexed_column_is_searchable() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let a = account(w).await;
    let id = mail(
        w,
        a,
        &[
            ("subject", "'one'"),
            ("sender", "'bob@example.org'"),
            ("sender_name", "'Robert'"),
            ("body", "'three'"),
            ("summary", "'four'"),
        ],
    )
    .await
    .unwrap();
    for word in ["one", "example", "robert", "three", "four"] {
        assert_eq!(search(w, word).await, vec![id], "{word}");
    }
    exec(
        w,
        &format!(
            "UPDATE mails SET subject = 'uno', sender = 'carol@example.net', sender_name = 'Carol', \
             body = 'tres', summary = 'cuatro' WHERE id = {id}"
        ),
    )
    .await;
    for word in ["one", "robert", "three", "four"] {
        assert!(search(w, word).await.is_empty(), "{word}");
    }
    for word in ["uno", "carol", "tres", "cuatro"] {
        assert_eq!(search(w, word).await, vec![id], "{word}");
    }
    integrity_check(w).await;
}

#[tokio::test]
async fn insert_or_replace_keeps_the_index() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let a = account(w).await;
    mail(w, a, &[("provider_id", "'same'"), ("subject", "'Facture'")])
        .await
        .unwrap();
    exec(
        w,
        &format!(
            "INSERT OR REPLACE INTO mails (account_id, provider_id, received_at, location, source, subject) \
             VALUES ({a}, 'same', '2026-11-25T08:30:00.000Z', 'inbox', 'inbox', 'Relance')"
        ),
    )
    .await;
    assert!(search(w, "facture").await.is_empty());
    assert_eq!(search(w, "relance").await.len(), 1);
    integrity_check(w).await;
}
