use domain::{Device, DeviceName, DeviceNameError, PairingCode};
use ports::{NewDevice, RepositoryError};

use super::{Deps, DeviceToken, hash_pairing_code, hash_token};

/// Redeems a pairing code for a new device and its bearer token.
pub struct PairDevice(Deps);

/// The new device, and its token: the only time the token is returned.
#[derive(Debug)]
pub struct PairedDevice {
    pub device: Device,
    pub token: DeviceToken,
}

/// Why pairing failed. Messages are codes and never contain the code or
/// the token.
#[derive(Debug, thiserror::Error)]
pub enum PairError {
    #[error(transparent)]
    InvalidName(DeviceNameError),
    /// Unknown, malformed, used or expired: the same answer for all.
    #[error("invalid_pairing_code")]
    InvalidCode,
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl PairDevice {
    pub fn new(deps: Deps) -> Self {
        PairDevice(deps)
    }

    /// Checks the name first, then the shape of the code (a malformed code
    /// is not looked up), then redeems the code and creates the device in
    /// one step of the repository.
    pub async fn execute(&self, code: &str, name: &str) -> Result<PairedDevice, PairError> {
        let name = DeviceName::new(name).map_err(PairError::InvalidName)?;
        let code = PairingCode::parse(code).map_err(|_| PairError::InvalidCode)?;
        let token = DeviceToken::generate(self.0.secrets.as_ref());
        let now = self.0.clock.now();
        let device = self
            .0
            .repo
            .redeem_pairing_code(
                &hash_pairing_code(&code),
                now,
                NewDevice {
                    name,
                    token_hash: hash_token(token.as_str()),
                    created_at: now,
                },
            )
            .await?
            .ok_or(PairError::InvalidCode)?;
        Ok(PairedDevice { device, token })
    }
}
