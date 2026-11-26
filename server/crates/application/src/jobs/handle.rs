use std::fmt;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use domain::{Event, Job, JobId, JobProgress, JobStatus};
use futures_util::FutureExt;

use super::PROGRESS_INTERVAL;
use crate::Deps;

/// Why a job's work failed. The message is stored and shown to the
/// clients: no secret in it.
#[derive(Clone, Debug)]
pub struct JobError(pub String);

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for JobError {}

/// A started job, waiting for its work. Made by [`super::StartJob`].
pub struct JobHandle {
    deps: Deps,
    job: Job,
}

impl JobHandle {
    pub(super) fn new(deps: Deps, job: Job) -> JobHandle {
        JobHandle { deps, job }
    }

    pub fn id(&self) -> JobId {
        self.job.id
    }

    /// Runs the work to its end and records the outcome: `Succeeded`,
    /// `Failed` with the error message, or `Failed` with "job panicked"
    /// when the work panics. The final counters are always saved and
    /// published. Storage failures are logged and never stop the job.
    ///
    /// The work reports its advance on the [`Progress`] it receives.
    pub async fn run<F, Fut>(self, work: F) -> JobStatus
    where
        F: FnOnce(Arc<Progress>) -> Fut,
        Fut: Future<Output = Result<(), JobError>> + Send,
    {
        let progress = Arc::new(Progress::new(self.deps.clone(), self.job));
        let outcome = AssertUnwindSafe(work(progress.clone()))
            .catch_unwind()
            .await;
        let (status, error) = match outcome {
            Ok(Ok(())) => (JobStatus::Succeeded, None),
            Ok(Err(err)) => (JobStatus::Failed, Some(err.0)),
            Err(_) => (JobStatus::Failed, Some("job panicked".to_owned())),
        };
        progress.finish(status, error).await;
        status
    }
}

struct State {
    job: Job,
    last_published: Option<DateTime<Utc>>,
}

/// The advance of a running job, reported by its work.
///
/// Counters are kept in memory; they are saved and published at most every
/// [`PROGRESS_INTERVAL`], the first update at once.
pub struct Progress {
    deps: Deps,
    state: Mutex<State>,
}

impl Progress {
    fn new(deps: Deps, job: Job) -> Progress {
        Progress {
            deps,
            state: Mutex::new(State {
                job,
                last_published: None,
            }),
        }
    }

    /// One more item processed: `ok` false counts it as failed too.
    pub async fn step(&self, ok: bool) {
        self.update(|progress| {
            progress.done += 1;
            if !ok {
                progress.failed += 1;
            }
        })
        .await;
    }

    /// The number of items turned out to be `total`. Throttled like
    /// [`step`](Progress::step); the end of the job always carries it.
    pub async fn set_total(&self, total: u64) {
        self.update(|progress| progress.total = Some(total)).await;
    }

    async fn update(&self, change: impl FnOnce(&mut JobProgress)) {
        let now = self.deps.clock.now();
        // The lock is released before any await.
        let snapshot = {
            let mut state = self.state.lock().unwrap();
            change(&mut state.job.progress);
            let due = state
                .last_published
                .is_none_or(|last| now - last >= PROGRESS_INTERVAL);
            if !due {
                return;
            }
            state.last_published = Some(now);
            state.job.clone()
        };
        if let Err(err) = self
            .deps
            .jobs
            .save_progress(snapshot.id, snapshot.progress)
            .await
        {
            tracing::warn!(job = snapshot.id.get(), error = %err, "cannot save the job progress");
        }
        self.deps.events.publish(Event::Job(snapshot));
    }

    async fn finish(&self, status: JobStatus, error: Option<String>) {
        let now = self.deps.clock.now();
        let snapshot = {
            let mut state = self.state.lock().unwrap();
            state.job.status = status;
            state.job.error = error;
            state.job.finished_at = Some(now);
            state.job.clone()
        };
        if let Err(err) = self
            .deps
            .jobs
            .finish(
                snapshot.id,
                status,
                snapshot.progress,
                snapshot.error.clone(),
                now,
            )
            .await
        {
            tracing::warn!(job = snapshot.id.get(), error = %err, "cannot save the job end");
        }
        self.deps.events.publish(Event::Job(snapshot));
    }
}
