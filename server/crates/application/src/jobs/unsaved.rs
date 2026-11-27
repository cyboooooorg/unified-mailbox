use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use domain::{Job, JobId};

use crate::Deps;

/// The ended jobs whose final write failed, kept until it succeeds.
///
/// Shared through [`Deps`]: `app` creates one per process. Until an end is
/// saved, [`GetJob`](super::GetJob) returns it and
/// [`ListActiveJobs`](super::ListActiveJobs) leaves the job out, so the API
/// agrees with the end already published. [`FlushJobEnds`] writes them
/// again. They are lost if the process dies first: the next startup then
/// marks the job `interrupted`.
#[derive(Debug, Default)]
pub struct UnsavedJobEnds(Mutex<HashMap<JobId, (Job, DateTime<Utc>)>>);

impl UnsavedJobEnds {
    pub fn new() -> UnsavedJobEnds {
        UnsavedJobEnds::default()
    }

    /// How many ends are waiting to be written.
    pub fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Keeps `job`, ended at `at`, until [`FlushJobEnds`] saves it.
    pub(super) fn keep(&self, job: Job, at: DateTime<Utc>) {
        self.0.lock().unwrap().insert(job.id, (job, at));
    }

    pub(super) fn get(&self, id: JobId) -> Option<Job> {
        self.0.lock().unwrap().get(&id).map(|(job, _)| job.clone())
    }

    pub(super) fn contains(&self, id: JobId) -> bool {
        self.0.lock().unwrap().contains_key(&id)
    }

    fn all(&self) -> Vec<(Job, DateTime<Utc>)> {
        self.0.lock().unwrap().values().cloned().collect()
    }

    fn forget(&self, id: JobId) {
        self.0.lock().unwrap().remove(&id);
    }
}

/// Writes the ends kept in [`UnsavedJobEnds`] again. `app` runs it every
/// 30 s and once more after the graceful shutdown.
pub struct FlushJobEnds(Deps);

impl FlushJobEnds {
    pub fn new(deps: Deps) -> Self {
        FlushJobEnds(deps)
    }

    /// Saves each kept end and forgets the ones saved. A failed write keeps
    /// its end for the next run. Returns how many ends are still waiting.
    pub async fn execute(&self) -> usize {
        for (job, at) in self.0.job_ends.all() {
            match self
                .0
                .jobs
                .finish(job.id, job.status, job.progress, job.error.clone(), at)
                .await
            {
                Ok(()) => {
                    self.0.job_ends.forget(job.id);
                    tracing::info!(job = job.id.get(), "job end saved on retry");
                }
                Err(err) => {
                    tracing::warn!(job = job.id.get(), error = %err, "cannot save the job end yet");
                }
            }
        }
        self.0.job_ends.len()
    }
}
