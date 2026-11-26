//! Jobs: long operations tracked from start to end, with their progress.

use chrono::{DateTime, Utc};
use thiserror::Error;

/// The identifier of a job, assigned by the store.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct JobId(i64);

impl JobId {
    pub fn new(id: i64) -> JobId {
        JobId(id)
    }

    pub fn get(self) -> i64 {
        self.0
    }
}

/// The kind of a job: a snake_case code of 1 to 32 characters, starting
/// with a lower-case letter. Kinds are constants of the code, not user
/// input: no trimming.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobKind(String);

impl JobKind {
    pub const MAX_CHARS: usize = 32;

    pub fn new(raw: &str) -> Result<JobKind, JobKindError> {
        match raw.chars().count() {
            0 => return Err(JobKindError::Empty),
            n if n > Self::MAX_CHARS => {
                return Err(JobKindError::TooLong {
                    max: Self::MAX_CHARS,
                });
            }
            _ => {}
        }
        let mut chars = raw.chars();
        let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
        let rest_ok = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if first_ok && rest_ok {
            Ok(JobKind(raw.to_owned()))
        } else {
            Err(JobKindError::InvalidCharacter)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Errors of [`JobKind`]. Messages are codes.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum JobKindError {
    #[error("job_kind_empty")]
    Empty,
    #[error("job_kind_too_long")]
    TooLong { max: usize },
    #[error("job_kind_invalid_character")]
    InvalidCharacter,
}

/// Where a job stands. `Running` is the only state that changes.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum JobStatus {
    Running,
    Succeeded,
    Failed,
    /// The server stopped while the job ran.
    Interrupted,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            JobStatus::Running => "running",
            JobStatus::Succeeded => "succeeded",
            JobStatus::Failed => "failed",
            JobStatus::Interrupted => "interrupted",
        }
    }

    pub fn parse(raw: &str) -> Option<JobStatus> {
        match raw {
            "running" => Some(JobStatus::Running),
            "succeeded" => Some(JobStatus::Succeeded),
            "failed" => Some(JobStatus::Failed),
            "interrupted" => Some(JobStatus::Interrupted),
            _ => None,
        }
    }
}

/// How far a job went: items done and failed, out of `total` when known.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct JobProgress {
    pub done: u64,
    pub failed: u64,
    pub total: Option<u64>,
}

/// A job as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub id: JobId,
    pub kind: JobKind,
    pub status: JobStatus,
    pub progress: JobProgress,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_accepts_snake_case() {
        assert_eq!(JobKind::new("labels_sync").unwrap().as_str(), "labels_sync");
    }

    #[test]
    fn kind_rejects_empty() {
        assert_eq!(JobKind::new(""), Err(JobKindError::Empty));
    }

    #[test]
    fn kind_length_limit() {
        assert!(JobKind::new(&"a".repeat(32)).is_ok());
        assert_eq!(
            JobKind::new(&"a".repeat(33)),
            Err(JobKindError::TooLong { max: 32 })
        );
    }

    #[test]
    fn kind_rejects_upper_case_digit_start_and_dash() {
        for input in ["Labels", "1abc", "_abc", "a-b"] {
            assert_eq!(
                JobKind::new(input),
                Err(JobKindError::InvalidCharacter),
                "{input}"
            );
        }
    }

    #[test]
    fn status_round_trip() {
        for status in [
            JobStatus::Running,
            JobStatus::Succeeded,
            JobStatus::Failed,
            JobStatus::Interrupted,
        ] {
            assert_eq!(JobStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(JobStatus::parse("paused"), None);
    }
}
