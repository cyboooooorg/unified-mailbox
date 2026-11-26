use std::sync::Arc;

use application::{
    AuthenticateDevice, CreatePairingCode, Deps, ListDevices, PairDevice, RevokeDevice,
};

/// The use cases the routes call, built once in `app`.
#[derive(Clone)]
pub struct AppState {
    pub create_pairing_code: Arc<CreatePairingCode>,
    pub pair_device: Arc<PairDevice>,
    pub authenticate_device: Arc<AuthenticateDevice>,
    pub list_devices: Arc<ListDevices>,
    pub revoke_device: Arc<RevokeDevice>,
}

impl AppState {
    pub fn new(deps: Deps) -> AppState {
        AppState {
            create_pairing_code: Arc::new(CreatePairingCode::new(deps.clone())),
            pair_device: Arc::new(PairDevice::new(deps.clone())),
            authenticate_device: Arc::new(AuthenticateDevice::new(deps.clone())),
            list_devices: Arc::new(ListDevices::new(deps.clone())),
            revoke_device: Arc::new(RevokeDevice::new(deps)),
        }
    }
}
