//! Job routes: the running jobs and one job's state.

use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
use domain::{Job, JobId, JobStatus};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::CurrentDevice;
use crate::error::{ApiError, ErrorBody, ErrorCode};
use crate::extract::ApiPath;
use crate::state::AppState;

/// Where a job stands.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobStatusDto {
    Running,
    Succeeded,
    Failed,
    /// The server stopped while the job ran.
    Interrupted,
}

impl From<JobStatus> for JobStatusDto {
    fn from(status: JobStatus) -> JobStatusDto {
        match status {
            JobStatus::Running => JobStatusDto::Running,
            JobStatus::Succeeded => JobStatusDto::Succeeded,
            JobStatus::Failed => JobStatusDto::Failed,
            JobStatus::Interrupted => JobStatusDto::Interrupted,
        }
    }
}

/// A long operation and its progress.
#[derive(Debug, Serialize, ToSchema)]
pub struct JobDto {
    pub id: i64,
    pub kind: String,
    pub status: JobStatusDto,
    /// Items processed with success.
    pub done: u64,
    /// Items that failed.
    pub failed: u64,
    /// Always present, `null` while the number of items is unknown.
    #[schema(required = true, nullable)]
    pub total: Option<u64>,
    /// Always present, `null` unless the job failed.
    #[schema(required = true, nullable)]
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Always present, `null` while the job runs.
    #[schema(required = true, nullable)]
    pub finished_at: Option<DateTime<Utc>>,
}

impl From<Job> for JobDto {
    fn from(job: Job) -> JobDto {
        JobDto {
            id: job.id.get(),
            kind: job.kind.as_str().to_owned(),
            status: job.status.into(),
            done: job.progress.done,
            failed: job.progress.failed,
            total: job.progress.total,
            error: job.error,
            created_at: job.created_at,
            finished_at: job.finished_at,
        }
    }
}

/// Lists the running jobs, oldest first.
#[utoipa::path(get, path = "/jobs", operation_id = "list_jobs",
    responses(
        (status = 200, description = "The running jobs", body = Vec<JobDto>),
        (status = 401, description = "Missing or invalid token", body = ErrorBody),
        (status = 500, description = "Internal error", body = ErrorBody),
    ))]
async fn list(
    CurrentDevice(_): CurrentDevice,
    State(state): State<AppState>,
) -> Result<Json<Vec<JobDto>>, ApiError> {
    let jobs = state.list_active_jobs.execute().await?;
    Ok(Json(jobs.into_iter().map(JobDto::from).collect()))
}

/// Returns one job, running or finished.
#[utoipa::path(get, path = "/jobs/{id}", operation_id = "get_job",
    params(("id" = i64, Path, description = "Job id")),
    responses(
        (status = 200, description = "The job", body = JobDto),
        (status = 401, description = "Missing or invalid token", body = ErrorBody),
        (status = 404, description = "Unknown job", body = ErrorBody),
        (status = 500, description = "Internal error", body = ErrorBody),
    ))]
async fn get(
    CurrentDevice(_): CurrentDevice,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<i64>,
) -> Result<Json<JobDto>, ApiError> {
    match state.get_job.execute(JobId::new(id)).await? {
        Some(job) => Ok(Json(job.into())),
        None => Err(ApiError::new(ErrorCode::NotFound, "job not found")),
    }
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list))
        .routes(routes!(get))
}
