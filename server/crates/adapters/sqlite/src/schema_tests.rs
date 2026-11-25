//! Constraints of the phase 1 schema, checked at the SQL level.

use std::sync::atomic::{AtomicU32, Ordering};

use domain::{Priority, ProviderKind};
use sqlx::SqlitePool;

use crate::store::test_store;

const AT: &str = "'2026-11-25T08:30:00.000Z'";

async fn exec(pool: &SqlitePool, sql: &str) -> Result<(), sqlx::Error> {
    sqlx::query(sql).execute(pool).await.map(|_| ())
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn account(pool: &SqlitePool, provider: &str, email: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(&format!(
        "INSERT INTO accounts (provider, email, created_at) VALUES ('{provider}', '{email}', {AT}) RETURNING id"
    ))
    .fetch_one(pool)
    .await
}

/// Inserts a mail with valid defaults, each overridable as `(column, SQL literal)`.
pub(crate) async fn mail(
    pool: &SqlitePool,
    account_id: i64,
    overrides: &[(&str, &str)],
) -> Result<i64, sqlx::Error> {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    let provider_id = format!("'m{}'", NEXT.fetch_add(1, Ordering::Relaxed));
    let account = account_id.to_string();
    let mut columns: Vec<(&str, &str)> = vec![
        ("account_id", &account),
        ("provider_id", &provider_id),
        ("received_at", AT),
        ("location", "'inbox'"),
        ("source", "'inbox'"),
    ];
    for (column, value) in overrides {
        match columns.iter_mut().find(|(c, _)| c == column) {
            Some(slot) => slot.1 = value,
            None => columns.push((column, value)),
        }
    }
    let names: Vec<&str> = columns.iter().map(|(c, _)| *c).collect();
    let values: Vec<&str> = columns.iter().map(|(_, v)| *v).collect();
    sqlx::query_scalar(&format!(
        "INSERT INTO mails ({}) VALUES ({}) RETURNING id",
        names.join(", "),
        values.join(", ")
    ))
    .fetch_one(pool)
    .await
}

#[tokio::test]
async fn every_provider_code_is_accepted() {
    let (store, _dir) = test_store().await;
    for kind in [
        ProviderKind::Gmail,
        ProviderKind::Outlook,
        ProviderKind::Imap,
    ] {
        account(store.write(), kind.as_code(), "alice@example.com")
            .await
            .unwrap();
    }
    assert!(
        account(store.write(), "yahoo", "alice@example.com")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn every_priority_code_is_accepted() {
    let (store, _dir) = test_store().await;
    let a = account(store.write(), "gmail", "alice@example.com")
        .await
        .unwrap();
    for p in [Priority::High, Priority::Normal, Priority::Low] {
        let code = format!("'{}'", p.as_code());
        mail(store.write(), a, &[("priority", &code)])
            .await
            .unwrap();
    }
    assert!(
        mail(store.write(), a, &[("priority", "'haute'")])
            .await
            .is_err()
    );
}

#[tokio::test]
async fn unknown_codes_are_rejected() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let a = account(w, "gmail", "alice@example.com").await.unwrap();
    assert!(mail(w, a, &[("location", "'spam'")]).await.is_err());
    assert!(mail(w, a, &[("source", "'archive'")]).await.is_err());
    assert!(
        mail(w, a, &[("review", "'maybe'"), ("review_at", AT)])
            .await
            .is_err()
    );
    assert!(mail(w, a, &[("billing", "'free'")]).await.is_err());
    let audit = |actor: &str| {
        format!(
            "INSERT INTO audit (at, actor, resource, action) VALUES ({AT}, '{actor}', 'mail', 'moved')"
        )
    };
    exec(w, &audit("claude")).await.unwrap();
    assert!(exec(w, &audit("robot")).await.is_err());
    let tag = |key: &str, color: &str| {
        format!(
            "INSERT INTO tags (key, name, name_folded, description, color, created_at) \
             VALUES ('{key}', '{key}', '{key}', 'A tag', '{color}', {AT})"
        )
    };
    assert!(exec(w, &tag("red", "red")).await.is_err());
    assert!(exec(w, &tag("upper", "#ABCDEF")).await.is_err());
    exec(w, &tag("lower", "#abcdef")).await.unwrap();
}

#[tokio::test]
async fn folder_path_only_with_folder_location() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let a = account(w, "gmail", "alice@example.com").await.unwrap();
    assert!(
        mail(w, a, &[("folder_path", "'Clients/Acme'")])
            .await
            .is_err()
    );
    assert!(mail(w, a, &[("location", "'folder'")]).await.is_err());
    mail(
        w,
        a,
        &[("location", "'folder'"), ("folder_path", "'Clients/Acme'")],
    )
    .await
    .unwrap();

    let sync = |location: &str, path: &str| {
        format!(
            "INSERT INTO sync_state (account_id, location, folder_path, days) \
             VALUES ({a}, '{location}', '{path}', 90)"
        )
    };
    assert!(exec(w, &sync("inbox", "Clients")).await.is_err());
    assert!(exec(w, &sync("folder", "")).await.is_err());
    exec(w, &sync("folder", "Clients")).await.unwrap();
    exec(w, &sync("inbox", "")).await.unwrap();
    assert!(
        exec(w, &sync("inbox", "")).await.is_err(),
        "one row per place"
    );
}

#[tokio::test]
async fn review_and_review_at_go_together() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let a = account(w, "gmail", "alice@example.com").await.unwrap();
    assert!(mail(w, a, &[("review", "'confirmed'")]).await.is_err());
    assert!(mail(w, a, &[("review_at", AT)]).await.is_err());
    mail(w, a, &[("review", "'confirmed'"), ("review_at", AT)])
        .await
        .unwrap();
}

#[tokio::test]
async fn spam_confidence_bounds() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let a = account(w, "gmail", "alice@example.com").await.unwrap();
    mail(w, a, &[("spam_confidence", "0")]).await.unwrap();
    mail(w, a, &[("spam_confidence", "100")]).await.unwrap();
    assert!(mail(w, a, &[("spam_confidence", "-1")]).await.is_err());
    assert!(mail(w, a, &[("spam_confidence", "101")]).await.is_err());
}

#[tokio::test]
async fn body_cut_enforced() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let a = account(w, "gmail", "alice@example.com").await.unwrap();
    let body = |n: usize| format!("'{}'", "é".repeat(n));
    mail(w, a, &[("body", &body(50_000))]).await.unwrap();
    assert!(mail(w, a, &[("body", &body(50_001))]).await.is_err());
}

#[tokio::test]
async fn duplicate_provider_id_rejected() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let a = account(w, "gmail", "alice@example.com").await.unwrap();
    mail(w, a, &[("provider_id", "'same'")]).await.unwrap();
    assert!(mail(w, a, &[("provider_id", "'same'")]).await.is_err());
}

#[tokio::test]
async fn spam_without_sender_or_message_key() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let a = account(w, "gmail", "alice@example.com").await.unwrap();
    mail(
        w,
        a,
        &[
            ("sender", "NULL"),
            ("message_key", "NULL"),
            ("thread_id", "NULL"),
        ],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn tag_names_unique_when_folded() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let tag = |key: &str, name: &str, folded: &str| {
        format!(
            "INSERT INTO tags (key, name, name_folded, description, color, created_at) \
             VALUES ('{key}', '{name}', '{folded}', 'A tag', '#868e96', {AT})"
        )
    };
    exec(w, &tag("ecole", "École", "école")).await.unwrap();
    assert!(exec(w, &tag("ecole-2", "école", "école")).await.is_err());
    exec(w, &tag("ecole-3", "Ecole", "ecole")).await.unwrap();
}

#[tokio::test]
async fn account_delete_cascades() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    let a = account(w, "gmail", "alice@example.com").await.unwrap();
    mail(w, a, &[]).await.unwrap();
    exec(
        w,
        &format!(
            "INSERT INTO muted_threads (account_id, thread_id, created_at) VALUES ({a}, 't1', {AT})"
        ),
    )
    .await
    .unwrap();
    exec(
        w,
        &format!("INSERT INTO sync_state (account_id, location, days) VALUES ({a}, 'inbox', 90)"),
    )
    .await
    .unwrap();
    exec(
        w,
        &format!(
            "INSERT INTO audit (at, actor, resource, action, account_id) VALUES ({AT}, 'user', 'account', 'connected', {a})"
        ),
    )
    .await
    .unwrap();

    exec(w, &format!("DELETE FROM accounts WHERE id = {a}"))
        .await
        .unwrap();
    for table in ["mails", "muted_threads", "sync_state"] {
        assert_eq!(count(w, table).await, 0, "{table}");
    }
    assert_eq!(count(w, "audit").await, 1);
}

#[tokio::test]
async fn foreign_keys_enforced() {
    let (store, _dir) = test_store().await;
    assert!(mail(store.write(), 999, &[]).await.is_err());
}

#[tokio::test]
async fn lowercase_addresses_enforced() {
    let (store, _dir) = test_store().await;
    let w = store.write();
    assert!(account(w, "gmail", "Alice@example.com").await.is_err());
    let trusted =
        |p: &str| format!("INSERT INTO trusted_senders (pattern, created_at) VALUES ('{p}', {AT})");
    assert!(exec(w, &trusted("@Example.com")).await.is_err());
    exec(w, &trusted("@example.com")).await.unwrap();
}
