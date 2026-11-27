use domain::{Job, JobId};
use ports::RepositoryError;

use crate::Deps;

/// Reads one job, to resume it after a reconnection.
pub struct GetJob(Deps);

impl GetJob {
    pub fn new(deps: Deps) -> Self {
        GetJob(deps)
    }

    /// `None` when the job is unknown or already purged. An end not
    /// written yet is returned as it was published.
    pub async fn execute(&self, id: JobId) -> Result<Option<Job>, RepositoryError> {
        if let Some(job) = self.0.job_ends.get(id) {
            return Ok(Some(job));
        }
        self.0.jobs.get(id).await
    }
}

/// Lists the jobs still running.
pub struct ListActiveJobs(Deps);

impl ListActiveJobs {
    pub fn new(deps: Deps) -> Self {
        ListActiveJobs(deps)
    }

    /// Oldest first. A job whose end is not written yet is left out.
    pub async fn execute(&self) -> Result<Vec<Job>, RepositoryError> {
        // The kept ids are read before the store: an end saved in between
        // is already out of the store's running jobs, so it never shows.
        let kept = self.0.job_ends.ids();
        let mut running = self.0.jobs.list_running().await?;
        running.retain(|job| !kept.contains(&job.id));
        Ok(running)
    }
}
