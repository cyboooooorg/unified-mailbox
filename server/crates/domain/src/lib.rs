//! Pure domain types and rules of Unified Mailbox.
//!
//! This crate performs no I/O and reads no global configuration: no
//! `tokio`, `sqlx`, `reqwest` or `axum`. The `layering` test in `app`
//! fails if one of them enters its dependency graph.
//!
//! - [`account`]: accounts, their provider kind, mail addresses.
//! - [`mail`]: mail metadata and where a mail sits ([`Location`]).
//! - [`tag`]: tags with a stable key derived once from the name.
//! - [`classification`]: what Claude says about a mail.
//! - [`device`]: paired devices, device names and pairing codes.
//! - [`job`]: jobs, their kind, status and progress.
//! - [`event`]: [`Event`], what the server pushes to the clients.
//! - [`moves`]: the moves the code may apply. [`Move`] has no `Delete`
//!   by design: nothing deletes a mail permanently.
//! - [`decision`]: [`decide`], the rule from a classification to a move.
//!
//! Types are validated when built, so an invalid value cannot travel
//! further. Values are stable English codes and errors are codes with
//! fields, never sentences: the client translates them. Nothing here
//! derives `serde`; the HTTP layer converts to its own DTOs.

pub mod account;
pub mod classification;
pub mod decision;
pub mod device;
pub mod event;
pub mod job;
pub mod mail;
pub mod moves;
pub mod tag;

pub use account::*;
pub use classification::*;
pub use decision::*;
pub use device::*;
pub use event::*;
pub use job::*;
pub use mail::*;
pub use moves::*;
pub use tag::*;
