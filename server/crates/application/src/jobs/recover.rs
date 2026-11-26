use ports::RepositoryError;

use super::JOB_RETENTION;
use crate::Deps;

/// What [`RecoverJobs`] did.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct RecoveredJobs {
    /// Jobs found `Running` and marked `Interrupted`.
    pub interrupted: u64,
    /// Finished jobs older than [`JOB_RETENTION`] that were deleted.
    pub deleted: u64,
}

/// Cleans the jobs at server start: whatever was running was stopped with
/// the previous process, and old finished jobs are purged.
pub struct RecoverJobs(Deps);

impl RecoverJobs {
    pub fn new(deps: Deps) -> Self {
        RecoverJobs(deps)
    }

    pub async fn execute(&self) -> Result<RecoveredJobs, RepositoryError> {
        let now = self.0.clock.now();
        let interrupted = self.0.jobs.interrupt_running(now).await?;
        let deleted = self
            .0
            .jobs
            .delete_finished_before(now - JOB_RETENTION)
            .await?;
        Ok(RecoveredJobs {
            interrupted,
            deleted,
        })
    }
}
