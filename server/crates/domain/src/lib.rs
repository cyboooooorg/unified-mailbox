//! Pure domain types and rules of Unified Mailbox.
//!
//! This crate performs no I/O and reads no global configuration: no
//! `tokio`, `sqlx`, `reqwest` or `axum`. The `layering` test in `app`
//! fails if one of them enters its dependency graph.
