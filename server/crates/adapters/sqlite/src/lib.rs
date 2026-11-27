//! SQLite adapter: opens the database, runs the migrations and encrypts
//! account secrets, and implements the repositories of the ports.

mod devices;
#[cfg(test)]
mod fts_tests;
mod jobs;
#[cfg(test)]
mod schema_tests;
mod secret;
mod settings;
mod store;
pub mod time;

pub use secret::{KeyFileError, SecretError, SecretKey, decrypt, encrypt, load_or_create_key};
pub use store::{Store, StoreConfig, StoreError};
