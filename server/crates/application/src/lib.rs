//! Use cases of Unified Mailbox, one per action. Depends only on `domain`
//! and `ports`: adapters are injected by `app`.
//!
//! - [`devices`]: pairing codes, pairing, bearer token authentication,
//!   listing and revocation of devices.
//! - `testing` (tests, or the `testing` feature): in-memory doubles of the
//!   ports, for tests only.

pub mod devices;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use devices::*;
