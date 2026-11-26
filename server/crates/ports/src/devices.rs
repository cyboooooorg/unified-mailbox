//! Storage of paired devices and of the pairing codes that enrol them.
//!
//! Tokens and codes reach the store only as hashes: the store never sees
//! a secret in clear.

use chrono::{DateTime, Utc};
use domain::{Device, DeviceId, DeviceName};

/// A pairing code to store, by the hash of its normalized characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewPairingCode {
    pub code_hash: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// The device that asked for the code; `None` when it comes from the
    /// server's `pair-code` subcommand (first device, recovery).
    pub created_by: Option<DeviceId>,
}

/// A device to store, by the hash of its bearer token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewDevice {
    pub name: DeviceName,
    pub token_hash: String,
    pub created_at: DateTime<Utc>,
}

/// Opaque storage failure: the HTTP layer maps it to 500.
#[derive(Debug, thiserror::Error)]
#[error("repository: {0}")]
pub struct RepositoryError(pub Box<dyn std::error::Error + Send + Sync>);

/// Paired devices and pairing codes.
#[async_trait::async_trait]
pub trait DeviceRepository: Send + Sync {
    /// Also deletes the codes that are used or expired at `code.created_at`.
    async fn insert_pairing_code(&self, code: NewPairingCode) -> Result<(), RepositoryError>;

    /// In one transaction: marks the code used if it exists, is unused and
    /// `now < expires_at`, then inserts the device. `None` otherwise.
    async fn redeem_pairing_code(
        &self,
        code_hash: &str,
        now: DateTime<Utc>,
        device: NewDevice,
    ) -> Result<Option<Device>, RepositoryError>;

    /// Active (not revoked) devices only.
    async fn find_active_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<Device>, RepositoryError>;

    async fn touch_last_seen(&self, id: DeviceId, at: DateTime<Utc>)
    -> Result<(), RepositoryError>;

    /// Active devices, oldest `created_at` first, then by id.
    async fn list_active(&self) -> Result<Vec<Device>, RepositoryError>;

    /// `false` when the device is unknown or already revoked.
    async fn revoke(&self, id: DeviceId, at: DateTime<Utc>) -> Result<bool, RepositoryError>;
}
