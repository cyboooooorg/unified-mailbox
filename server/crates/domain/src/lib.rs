//! Pure domain types and rules of Unified Mailbox.
//!
//! This crate performs no I/O and reads no global configuration: no
//! `tokio`, `sqlx`, `reqwest` or `axum`. The `layering` test in `app`
//! fails if one of them enters its dependency graph.

pub mod account;

pub use account::*;

pub mod mail;

pub use mail::*;

pub mod tag;

pub use tag::*;

pub mod classification;

pub use classification::*;
