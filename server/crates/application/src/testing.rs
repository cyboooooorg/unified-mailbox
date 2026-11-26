//! In-memory test doubles of the ports, for the use case tests and, through
//! the `testing` feature, for other crates' tests. Never used in the
//! server binary.

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use domain::{Device, DeviceId};
use ports::{Clock, DeviceRepository, NewDevice, NewPairingCode, RepositoryError, SecretGenerator};

/// A clock that stays at the instant it is given until moved.
#[derive(Debug)]
pub struct FixedClock(Mutex<DateTime<Utc>>);

impl FixedClock {
    pub fn new(now: DateTime<Utc>) -> FixedClock {
        FixedClock(Mutex::new(now))
    }

    pub fn set(&self, now: DateTime<Utc>) {
        *self.0.lock().unwrap() = now;
    }

    pub fn advance(&self, by: TimeDelta) {
        *self.0.lock().unwrap() += by;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

/// Fills each buffer with one counter byte: `0x01` on the first call,
/// `0x02` on the second… so successive secrets differ and are predictable.
#[derive(Debug, Default)]
pub struct SeqSecrets(Mutex<u8>);

impl SeqSecrets {
    pub fn new() -> SeqSecrets {
        SeqSecrets::default()
    }
}

impl SecretGenerator for SeqSecrets {
    fn fill(&self, buf: &mut [u8]) {
        let mut counter = self.0.lock().unwrap();
        *counter = counter.wrapping_add(1);
        buf.fill(*counter);
    }
}

#[derive(Debug)]
struct StoredCode {
    hash: String,
    expires_at: DateTime<Utc>,
    used: bool,
}

#[derive(Debug)]
struct StoredDevice {
    device: Device,
    token_hash: String,
}

#[derive(Debug, Default)]
struct State {
    codes: Vec<StoredCode>,
    devices: Vec<StoredDevice>,
    touches: usize,
    lookups: usize,
}

/// A [`DeviceRepository`] in memory. Ids start at 1.
#[derive(Debug, Default)]
pub struct InMemoryDevices(Mutex<State>);

impl InMemoryDevices {
    pub fn new() -> InMemoryDevices {
        InMemoryDevices::default()
    }

    /// How many times `touch_last_seen` was called.
    pub fn touch_count(&self) -> usize {
        self.0.lock().unwrap().touches
    }

    /// How many times `find_active_by_token_hash` was called.
    pub fn lookup_count(&self) -> usize {
        self.0.lock().unwrap().lookups
    }

    /// The token hashes of every stored device, revoked ones included.
    pub fn stored_token_hashes(&self) -> Vec<String> {
        let state = self.0.lock().unwrap();
        state.devices.iter().map(|d| d.token_hash.clone()).collect()
    }
}

#[async_trait]
impl DeviceRepository for InMemoryDevices {
    async fn insert_pairing_code(&self, code: NewPairingCode) -> Result<(), RepositoryError> {
        let mut state = self.0.lock().unwrap();
        state
            .codes
            .retain(|c| !c.used && code.created_at < c.expires_at);
        state.codes.push(StoredCode {
            hash: code.code_hash,
            expires_at: code.expires_at,
            used: false,
        });
        Ok(())
    }

    async fn redeem_pairing_code(
        &self,
        code_hash: &str,
        now: DateTime<Utc>,
        device: NewDevice,
    ) -> Result<Option<Device>, RepositoryError> {
        let mut state = self.0.lock().unwrap();
        let Some(code) = state
            .codes
            .iter_mut()
            .find(|c| c.hash == code_hash && !c.used && now < c.expires_at)
        else {
            return Ok(None);
        };
        code.used = true;
        let id = DeviceId::new(state.devices.len() as i64 + 1);
        let stored = Device {
            id,
            name: device.name,
            created_at: device.created_at,
            last_seen_at: None,
            revoked_at: None,
        };
        state.devices.push(StoredDevice {
            device: stored.clone(),
            token_hash: device.token_hash,
        });
        Ok(Some(stored))
    }

    async fn find_active_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<Device>, RepositoryError> {
        let mut state = self.0.lock().unwrap();
        state.lookups += 1;
        Ok(state
            .devices
            .iter()
            .find(|d| d.token_hash == token_hash && d.device.revoked_at.is_none())
            .map(|d| d.device.clone()))
    }

    async fn touch_last_seen(
        &self,
        id: DeviceId,
        at: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        let mut state = self.0.lock().unwrap();
        state.touches += 1;
        if let Some(d) = state.devices.iter_mut().find(|d| d.device.id == id) {
            d.device.last_seen_at = Some(at);
        }
        Ok(())
    }

    async fn list_active(&self) -> Result<Vec<Device>, RepositoryError> {
        let state = self.0.lock().unwrap();
        let mut active: Vec<Device> = state
            .devices
            .iter()
            .filter(|d| d.device.revoked_at.is_none())
            .map(|d| d.device.clone())
            .collect();
        active.sort_by_key(|d| (d.created_at, d.id.get()));
        Ok(active)
    }

    async fn revoke(&self, id: DeviceId, at: DateTime<Utc>) -> Result<bool, RepositoryError> {
        let mut state = self.0.lock().unwrap();
        match state
            .devices
            .iter_mut()
            .find(|d| d.device.id == id && d.device.revoked_at.is_none())
        {
            Some(d) => {
                d.device.revoked_at = Some(at);
                Ok(true)
            }
            None => Ok(false),
        }
    }
}
