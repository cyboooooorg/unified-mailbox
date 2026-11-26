use std::sync::Arc;
use std::time::Duration;

use application::{
    AuthenticateDevice, CreatePairingCode, Deps, DeviceIsActive, GetJob, ListActiveJobs,
    ListDevices, PairDevice, RevokeDevice, SubscribeEvents,
};

/// The use cases the routes call, built once in `app`.
#[derive(Clone)]
pub struct AppState {
    pub create_pairing_code: Arc<CreatePairingCode>,
    pub pair_device: Arc<PairDevice>,
    pub authenticate_device: Arc<AuthenticateDevice>,
    pub list_devices: Arc<ListDevices>,
    pub revoke_device: Arc<RevokeDevice>,
    pub device_is_active: Arc<DeviceIsActive>,
    pub get_job: Arc<GetJob>,
    pub list_active_jobs: Arc<ListActiveJobs>,
    pub subscribe_events: Arc<SubscribeEvents>,
    /// Turns `true` when the server starts shutting down.
    pub shutdown: tokio::sync::watch::Receiver<bool>,
    /// How often an idle event stream sends a ping.
    pub ping_interval: Duration,
}

impl AppState {
    pub fn new(deps: Deps, shutdown: tokio::sync::watch::Receiver<bool>) -> AppState {
        AppState {
            create_pairing_code: Arc::new(CreatePairingCode::new(deps.clone())),
            pair_device: Arc::new(PairDevice::new(deps.clone())),
            authenticate_device: Arc::new(AuthenticateDevice::new(deps.clone())),
            list_devices: Arc::new(ListDevices::new(deps.clone())),
            revoke_device: Arc::new(RevokeDevice::new(deps.clone())),
            device_is_active: Arc::new(DeviceIsActive::new(deps.clone())),
            get_job: Arc::new(GetJob::new(deps.clone())),
            list_active_jobs: Arc::new(ListActiveJobs::new(deps.clone())),
            subscribe_events: Arc::new(SubscribeEvents::new(deps)),
            shutdown,
            ping_interval: Duration::from_secs(15),
        }
    }

    pub fn with_ping_interval(mut self, ping_interval: Duration) -> AppState {
        self.ping_interval = ping_interval;
        self
    }
}
