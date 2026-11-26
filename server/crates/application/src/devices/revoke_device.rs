use domain::{DeviceId, Event};
use ports::RepositoryError;

use crate::Deps;

/// Revokes a device: its token stops working at its next request. The
/// device's row stays.
pub struct RevokeDevice(Deps);

impl RevokeDevice {
    pub fn new(deps: Deps) -> Self {
        RevokeDevice(deps)
    }

    /// Publishes [`Event::DeviceRevoked`] so the device's streams close.
    /// `false` (and no event) when the device is unknown or already revoked.
    pub async fn execute(&self, id: DeviceId) -> Result<bool, RepositoryError> {
        let now = self.0.clock.now();
        let revoked = self.0.repo.revoke(id, now).await?;
        if revoked {
            self.0.events.publish(Event::DeviceRevoked(id));
        }
        Ok(revoked)
    }
}
