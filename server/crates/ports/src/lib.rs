//! Traits the application needs from the outside world (`MailProvider`,
//! `Classifier`, repositories, `Notifier`, `Clock`…), implemented by the
//! adapters. Depends only on `domain`.
//!
//! - [`clock`]: [`Clock`], the current time.
//! - [`secrets`]: [`SecretGenerator`], random bytes for tokens and codes.
//! - [`devices`]: [`DeviceRepository`], paired devices and pairing codes.
//! - [`jobs`]: [`JobRepository`], long operations and their progress.
//! - [`events`]: [`EventBus`], events pushed to the clients.

pub mod clock;
pub mod devices;
pub mod events;
pub mod jobs;
pub mod secrets;

pub use clock::*;
pub use devices::*;
pub use events::*;
pub use jobs::*;
pub use secrets::*;
