//! SQLite adapter: opens the database, runs the migrations and encrypts
//! account secrets. Repositories are added with the use cases that need
//! them.

mod secret;

pub use secret::{KeyFileError, SecretError, SecretKey, decrypt, encrypt, load_or_create_key};
