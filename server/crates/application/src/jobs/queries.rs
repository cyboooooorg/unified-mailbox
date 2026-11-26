use domain::{Job, JobId};
use ports::RepositoryError;

use crate::Deps;

/// Reads one job, to resume it after a reconnection.
pub struct GetJob(Deps);

impl GetJob {
    pub fn new(deps: Deps) -> Self {
        GetJob(deps)
    }

    /// `None` when the job is unknown or already purged.
    pub async fn execute(&self, id: JobId) -> Result<Option<Job>, RepositoryError> {
        self.0.jobs.get(id).await
    }
}

/// Lists the jobs still running.
pub struct ListActiveJobs(Deps);

impl ListActiveJobs {
    pub fn new(deps: Deps) -> Self {
        ListActiveJobs(deps)
    }

    /// Oldest first.
    pub async fn execute(&self) -> Result<Vec<Job>, RepositoryError> {
        self.0.jobs.list_running().await
    }
}
