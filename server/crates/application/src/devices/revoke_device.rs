use domain::DeviceId;
use ports::RepositoryError;

use super::Deps;

/// Revokes a device: its token stops working at its next request. The
/// device's row stays.
pub struct RevokeDevice(Deps);

impl RevokeDevice {
    pub fn new(deps: Deps) -> Self {
        RevokeDevice(deps)
    }

    /// `false` when the device is unknown or already revoked.
    pub async fn execute(&self, id: DeviceId) -> Result<bool, RepositoryError> {
        let now = self.0.clock.now();
        self.0.repo.revoke(id, now).await
    }
}
