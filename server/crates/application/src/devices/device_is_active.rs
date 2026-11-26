use domain::DeviceId;
use ports::RepositoryError;

use super::Deps;

/// Whether a device is paired and not revoked.
///
/// The event stream asks it right after subscribing: a revocation that
/// happened between the authentication of the request and the subscription
/// published its event before the stream could receive it.
pub struct DeviceIsActive(Deps);

impl DeviceIsActive {
    pub fn new(deps: Deps) -> Self {
        DeviceIsActive(deps)
    }

    /// `false` when the device is unknown or revoked.
    pub async fn execute(&self, id: DeviceId) -> Result<bool, RepositoryError> {
        // Few devices: the active list is short.
        let active = self.0.repo.list_active().await?;
        Ok(active.iter().any(|device| device.id == id))
    }
}
