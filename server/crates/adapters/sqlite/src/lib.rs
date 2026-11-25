//! SQLite adapter: opens the database, runs the migrations and encrypts
//! account secrets. Repositories are added with the use cases that need
//! them.

mod secret;
mod store;
pub mod time;

pub use secret::{KeyFileError, SecretError, SecretKey, decrypt, encrypt, load_or_create_key};
pub use store::{Store, StoreConfig, StoreError};
