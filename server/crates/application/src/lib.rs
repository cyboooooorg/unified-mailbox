//! Use cases of Unified Mailbox, one per action. Depends only on `domain`
//! and `ports`: adapters are injected by `app`.
//!
//! - [`devices`]: pairing codes, pairing, bearer token authentication,
//!   listing and revocation of devices.
//! - [`jobs`]: long operations tracked with their progress, and the event
//!   stream that carries them to the clients.
//! - `testing` (tests, or the `testing` feature): in-memory doubles of the
//!   ports, for tests only.

mod deps;
pub mod devices;
pub mod jobs;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use deps::Deps;
pub use devices::*;
pub use jobs::*;
