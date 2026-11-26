use domain::Device;
use ports::RepositoryError;

use super::Deps;

/// The devices not revoked, oldest first.
pub struct ListDevices(Deps);

impl ListDevices {
    pub fn new(deps: Deps) -> Self {
        ListDevices(deps)
    }

    pub async fn execute(&self) -> Result<Vec<Device>, RepositoryError> {
        self.0.repo.list_active().await
    }
}
