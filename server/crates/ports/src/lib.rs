//! Traits the application needs from the outside world (`MailProvider`,
//! `Classifier`, repositories, `Notifier`, `Clock`…), implemented by the
//! adapters. Depends only on `domain`.
//!
//! - [`clock`]: [`Clock`], the current time.
//! - [`secrets`]: [`SecretGenerator`], random bytes for tokens and codes.
//! - [`devices`]: [`DeviceRepository`], paired devices and pairing codes.

pub mod clock;
pub mod devices;
pub mod secrets;

pub use clock::*;
pub use devices::*;
pub use secrets::*;
