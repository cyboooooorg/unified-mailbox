use std::sync::Arc;

use crate::jobs::UnsavedJobEnds;
use ports::{
    Clock, DeviceRepository, EventBus, JobRepository, SecretGenerator, SettingsRepository,
};

/// What the use cases need, injected by `app`.
#[derive(Clone)]
pub struct Deps {
    pub repo: Arc<dyn DeviceRepository>,
    pub clock: Arc<dyn Clock>,
    pub secrets: Arc<dyn SecretGenerator>,
    pub jobs: Arc<dyn JobRepository>,
    pub events: Arc<dyn EventBus>,
    pub settings: Arc<dyn SettingsRepository>,
    /// Job ends not written yet, shared by every use case of the process.
    pub job_ends: Arc<UnsavedJobEnds>,
}
