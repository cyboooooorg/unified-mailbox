//! In-memory test doubles of the ports, for the use case tests and, through
//! the `testing` feature, for other crates' tests. Never used in the
//! server binary.

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use domain::{Device, DeviceId, Event, Job, JobId, JobProgress, JobStatus, Language};
use futures_core::stream::BoxStream;
use futures_util::StreamExt;
use ports::{
    BusMessage, Clock, DeviceRepository, EventBus, JobRepository, NewDevice, NewJob,
    NewPairingCode, RepositoryError, SecretGenerator, SettingsRepository,
};

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
    fail_touch: bool,
    fail_reads: bool,
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

    /// Makes `touch_last_seen` fail (or work again).
    pub fn fail_touch(&self, fail: bool) {
        self.0.lock().unwrap().fail_touch = fail;
    }

    /// Makes the lookups (`find_active_by_token_hash`, `list_active`) fail
    /// (or work again).
    pub fn fail_reads(&self, fail: bool) {
        self.0.lock().unwrap().fail_reads = fail;
    }

    /// The token hashes of every stored device, revoked ones included.
    pub fn stored_token_hashes(&self) -> Vec<String> {
        let state = self.0.lock().unwrap();
        state.devices.iter().map(|d| d.token_hash.clone()).collect()
    }
}

fn failure(detail: &str) -> RepositoryError {
    RepositoryError(detail.into())
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
        if state.fail_reads {
            return Err(failure("secret detail of the lookup"));
        }
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
        if state.fail_touch {
            return Err(failure("secret detail of the touch"));
        }
        if let Some(d) = state.devices.iter_mut().find(|d| d.device.id == id) {
            d.device.last_seen_at = Some(at);
        }
        Ok(())
    }

    async fn list_active(&self) -> Result<Vec<Device>, RepositoryError> {
        let state = self.0.lock().unwrap();
        if state.fail_reads {
            return Err(failure("secret detail of the listing"));
        }
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

#[derive(Debug, Default)]
struct JobsState {
    jobs: Vec<Job>,
    saves: usize,
    fail_writes: bool,
}

/// A [`JobRepository`] in memory. Ids start at 1.
#[derive(Debug, Default)]
pub struct InMemoryJobs(Mutex<JobsState>);

impl InMemoryJobs {
    pub fn new() -> InMemoryJobs {
        InMemoryJobs::default()
    }

    /// Makes every write (`create`, `save_progress`, `finish`,
    /// `interrupt_running`, `delete_finished_before`) fail, or work again.
    pub fn fail_writes(&self, fail: bool) {
        self.0.lock().unwrap().fail_writes = fail;
    }

    /// How many times `save_progress` was called, failed calls included.
    pub fn save_count(&self) -> usize {
        self.0.lock().unwrap().saves
    }

    /// Stores a job as it is, to set up a state.
    pub fn insert(&self, job: Job) {
        self.0.lock().unwrap().jobs.push(job);
    }
}

#[async_trait]
impl JobRepository for InMemoryJobs {
    async fn create(&self, job: NewJob) -> Result<Job, RepositoryError> {
        let mut state = self.0.lock().unwrap();
        if state.fail_writes {
            return Err(failure("secret detail of the creation"));
        }
        let id = JobId::new(state.jobs.iter().map(|j| j.id.get()).max().unwrap_or(0) + 1);
        let stored = Job {
            id,
            kind: job.kind,
            status: JobStatus::Running,
            progress: JobProgress {
                done: 0,
                failed: 0,
                total: job.total,
            },
            error: None,
            created_at: job.created_at,
            finished_at: None,
        };
        state.jobs.push(stored.clone());
        Ok(stored)
    }

    async fn save_progress(&self, id: JobId, progress: JobProgress) -> Result<(), RepositoryError> {
        let mut state = self.0.lock().unwrap();
        state.saves += 1;
        if state.fail_writes {
            return Err(failure("secret detail of the save"));
        }
        if let Some(job) = state.jobs.iter_mut().find(|j| j.id == id) {
            job.progress = progress;
        }
        Ok(())
    }

    async fn finish(
        &self,
        id: JobId,
        status: JobStatus,
        progress: JobProgress,
        error: Option<String>,
        at: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        let mut state = self.0.lock().unwrap();
        if state.fail_writes {
            return Err(failure("secret detail of the end"));
        }
        if let Some(job) = state.jobs.iter_mut().find(|j| j.id == id) {
            job.status = status;
            job.progress = progress;
            job.error = error;
            job.finished_at = Some(at);
        }
        Ok(())
    }

    async fn get(&self, id: JobId) -> Result<Option<Job>, RepositoryError> {
        let state = self.0.lock().unwrap();
        Ok(state.jobs.iter().find(|j| j.id == id).cloned())
    }

    async fn list_running(&self) -> Result<Vec<Job>, RepositoryError> {
        let state = self.0.lock().unwrap();
        let mut running: Vec<Job> = state
            .jobs
            .iter()
            .filter(|j| j.status == JobStatus::Running)
            .cloned()
            .collect();
        running.sort_by_key(|j| (j.created_at, j.id.get()));
        Ok(running)
    }

    async fn interrupt_running(&self, at: DateTime<Utc>) -> Result<u64, RepositoryError> {
        let mut state = self.0.lock().unwrap();
        if state.fail_writes {
            return Err(failure("secret detail of the interruption"));
        }
        let mut count = 0;
        for job in state
            .jobs
            .iter_mut()
            .filter(|j| j.status == JobStatus::Running)
        {
            job.status = JobStatus::Interrupted;
            job.finished_at = Some(at);
            count += 1;
        }
        Ok(count)
    }

    async fn delete_finished_before(&self, before: DateTime<Utc>) -> Result<u64, RepositoryError> {
        let mut state = self.0.lock().unwrap();
        if state.fail_writes {
            return Err(failure("secret detail of the purge"));
        }
        let kept = state.jobs.len();
        state
            .jobs
            .retain(|j| !j.finished_at.is_some_and(|at| at < before));
        Ok((kept - state.jobs.len()) as u64)
    }
}

/// An [`EventBus`] that records what is published. Its `subscribe` returns
/// an empty stream.
#[derive(Debug, Default)]
pub struct RecordingBus(Mutex<Vec<Event>>);

impl RecordingBus {
    pub fn new() -> RecordingBus {
        RecordingBus::default()
    }

    /// Everything published so far, in order.
    pub fn events(&self) -> Vec<Event> {
        self.0.lock().unwrap().clone()
    }
}

impl EventBus for RecordingBus {
    fn publish(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }

    fn subscribe(&self) -> BoxStream<'static, BusMessage> {
        futures_util::stream::empty().boxed()
    }
}

#[derive(Debug, Default)]
struct SettingsState {
    language: Option<Language>,
    sets: usize,
    fail_reads: bool,
    fail_writes: bool,
}

/// A [`SettingsRepository`] in memory. Nothing is set at first.
#[derive(Debug, Default)]
pub struct InMemorySettings(Mutex<SettingsState>);

impl InMemorySettings {
    pub fn new() -> InMemorySettings {
        InMemorySettings::default()
    }

    /// Makes every read fail.
    pub fn fail_reads(&self) {
        self.0.lock().unwrap().fail_reads = true;
    }

    /// Makes every write fail.
    pub fn fail_writes(&self) {
        self.0.lock().unwrap().fail_writes = true;
    }

    /// How many times `set_language` stored a value.
    pub fn set_count(&self) -> usize {
        self.0.lock().unwrap().sets
    }
}

#[async_trait]
impl SettingsRepository for InMemorySettings {
    async fn language(&self) -> Result<Option<Language>, RepositoryError> {
        let state = self.0.lock().unwrap();
        if state.fail_reads {
            return Err(RepositoryError("settings read failed".into()));
        }
        Ok(state.language)
    }

    async fn set_language(
        &self,
        language: Language,
        _at: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        let mut state = self.0.lock().unwrap();
        if state.fail_writes {
            return Err(RepositoryError("settings write failed".into()));
        }
        state.language = Some(language);
        state.sets += 1;
        Ok(())
    }
}
