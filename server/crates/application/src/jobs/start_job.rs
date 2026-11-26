use domain::{Event, JobKind};
use ports::{NewJob, RepositoryError};

use super::JobHandle;
use crate::Deps;

/// Creates a job and publishes its start. Run the work with
/// [`JobHandle::run`].
pub struct StartJob(Deps);

impl StartJob {
    pub fn new(deps: Deps) -> Self {
        StartJob(deps)
    }

    /// Stores a `Running` job (`total` when the number of items is known)
    /// and publishes [`Event::Job`]. Hand the job id back to the caller
    /// before spawning [`JobHandle::run`].
    pub async fn execute(
        &self,
        kind: JobKind,
        total: Option<u64>,
    ) -> Result<JobHandle, RepositoryError> {
        let job = self
            .0
            .jobs
            .create(NewJob {
                kind,
                total,
                created_at: self.0.clock.now(),
            })
            .await?;
        self.0.events.publish(Event::Job(job.clone()));
        Ok(JobHandle::new(self.0.clone(), job))
    }
}
