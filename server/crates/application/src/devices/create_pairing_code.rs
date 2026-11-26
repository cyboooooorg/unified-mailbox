use chrono::{DateTime, Utc};
use domain::{DeviceId, PAIRING_CODE_TTL, PairingCode};
use ports::{NewPairingCode, RepositoryError};

use super::{Deps, hash_pairing_code};

/// Issues a one-time pairing code, valid [`PAIRING_CODE_TTL`]. Only its
/// hash is stored; creating a code also deletes the used and expired ones.
pub struct CreatePairingCode(Deps);

/// A code to show once, to the person pairing a new device.
#[derive(Debug)]
pub struct CreatedPairingCode {
    pub code: PairingCode,
    pub expires_at: DateTime<Utc>,
}

impl CreatePairingCode {
    pub fn new(deps: Deps) -> Self {
        CreatePairingCode(deps)
    }

    /// `created_by` is the device that asked for the code, `None` from the
    /// server's `pair-code` subcommand.
    pub async fn execute(
        &self,
        created_by: Option<DeviceId>,
    ) -> Result<CreatedPairingCode, RepositoryError> {
        let mut bytes = [0u8; 5];
        self.0.secrets.fill(&mut bytes);
        let code = PairingCode::from_bytes(bytes);
        let now = self.0.clock.now();
        let expires_at = now + PAIRING_CODE_TTL;
        self.0
            .repo
            .insert_pairing_code(NewPairingCode {
                code_hash: hash_pairing_code(&code),
                created_at: now,
                expires_at,
                created_by,
            })
            .await?;
        Ok(CreatedPairingCode { code, expires_at })
    }
}
