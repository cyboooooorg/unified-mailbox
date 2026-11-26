use domain::{DeviceId, LAST_SEEN_RESOLUTION};
use ports::RepositoryError;

use super::{Deps, hash_token, looks_like_token};

/// Finds the active device that holds a bearer token, and records when it
/// was last seen, at most once per [`LAST_SEEN_RESOLUTION`].
pub struct AuthenticateDevice(Deps);

impl AuthenticateDevice {
    pub fn new(deps: Deps) -> Self {
        AuthenticateDevice(deps)
    }

    /// `None` for anything that is not the token of an active device. A
    /// string that does not look like a token is not looked up.
    pub async fn execute(&self, token: &str) -> Result<Option<DeviceId>, RepositoryError> {
        if !looks_like_token(token) {
            return Ok(None);
        }
        let Some(device) = self
            .0
            .repo
            .find_active_by_token_hash(&hash_token(token))
            .await?
        else {
            return Ok(None);
        };
        let now = self.0.clock.now();
        if device
            .last_seen_at
            .is_none_or(|seen| now - seen >= LAST_SEEN_RESOLUTION)
        {
            self.0.repo.touch_last_seen(device.id, now).await?;
        }
        Ok(Some(device.id))
    }
}
