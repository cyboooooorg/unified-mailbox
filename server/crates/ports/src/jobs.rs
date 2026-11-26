//! Storage of jobs: long operations and their progress.

use chrono::{DateTime, Utc};
use domain::{Job, JobId, JobKind, JobProgress, JobStatus};

use crate::RepositoryError;

/// A job to store: it starts `Running` with zero counters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewJob {
    pub kind: JobKind,
    pub total: Option<u64>,
    pub created_at: DateTime<Utc>,
}

/// Jobs, from their creation to their end.
#[async_trait::async_trait]
pub trait JobRepository: Send + Sync {
    /// Stores the job as `Running`, with zero counters.
    async fn create(&self, job: NewJob) -> Result<Job, RepositoryError>;

    async fn save_progress(&self, id: JobId, progress: JobProgress) -> Result<(), RepositoryError>;

    /// Ends the job with its last counters and, on failure, its error.
    async fn finish(
        &self,
        id: JobId,
        status: JobStatus,
        progress: JobProgress,
        error: Option<String>,
        at: DateTime<Utc>,
    ) -> Result<(), RepositoryError>;

    async fn get(&self, id: JobId) -> Result<Option<Job>, RepositoryError>;

    /// The jobs still `Running`, oldest first.
    async fn list_running(&self) -> Result<Vec<Job>, RepositoryError>;

    /// Marks every `Running` job `Interrupted` with `finished_at = at`
    /// (the server stopped under them). Returns how many.
    async fn interrupt_running(&self, at: DateTime<Utc>) -> Result<u64, RepositoryError>;

    /// Deletes the finished jobs that ended before `before`. Returns how many.
    async fn delete_finished_before(&self, before: DateTime<Utc>) -> Result<u64, RepositoryError>;
}
