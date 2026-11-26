//! `JobRepository` over SQLite: long operations and their progress.

use chrono::{DateTime, Utc};
use domain::{Job, JobId, JobKind, JobProgress, JobStatus};
use ports::{JobRepository, NewJob, RepositoryError};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::store::Store;
use crate::time::{from_db, to_db};

const JOB_COLUMNS: &str = "id, kind, status, done, failed, total, error, created_at, finished_at";

fn fail(err: impl std::error::Error + Send + Sync + 'static) -> RepositoryError {
    RepositoryError(Box::new(err))
}

fn corrupt(what: &str) -> RepositoryError {
    RepositoryError(format!("stored {what} is invalid").into())
}

fn time(value: &str, what: &str) -> Result<DateTime<Utc>, RepositoryError> {
    from_db(value).ok_or_else(|| corrupt(what))
}

fn to_i64(value: u64) -> Result<i64, RepositoryError> {
    i64::try_from(value).map_err(|_| RepositoryError("counter out of range".into()))
}

fn counter(row: &SqliteRow, column: &str) -> Result<u64, RepositoryError> {
    let value: i64 = row.try_get(column).map_err(fail)?;
    u64::try_from(value).map_err(|_| corrupt(column))
}

fn job(row: &SqliteRow) -> Result<Job, RepositoryError> {
    let kind: String = row.try_get("kind").map_err(fail)?;
    let status: String = row.try_get("status").map_err(fail)?;
    let total: Option<i64> = row.try_get("total").map_err(fail)?;
    let finished_at: Option<String> = row.try_get("finished_at").map_err(fail)?;
    Ok(Job {
        id: JobId::new(row.try_get("id").map_err(fail)?),
        kind: JobKind::new(&kind).map_err(|_| corrupt("job kind"))?,
        status: JobStatus::parse(&status).ok_or_else(|| corrupt("job status"))?,
        progress: JobProgress {
            done: counter(row, "done")?,
            failed: counter(row, "failed")?,
            total: total
                .map(|t| u64::try_from(t).map_err(|_| corrupt("total")))
                .transpose()?,
        },
        error: row.try_get("error").map_err(fail)?,
        created_at: time(
            &row.try_get::<String, _>("created_at").map_err(fail)?,
            "created_at",
        )?,
        finished_at: finished_at.map(|v| time(&v, "finished_at")).transpose()?,
    })
}

#[async_trait::async_trait]
impl JobRepository for Store {
    async fn create(&self, new: NewJob) -> Result<Job, RepositoryError> {
        let total = new.total.map(to_i64).transpose()?;
        let row = sqlx::query(&format!(
            "INSERT INTO jobs (kind, status, total, created_at) VALUES (?, 'running', ?, ?) \
             RETURNING {JOB_COLUMNS}"
        ))
        .bind(new.kind.as_str())
        .bind(total)
        .bind(to_db(new.created_at))
        .fetch_one(self.write())
        .await
        .map_err(fail)?;
        job(&row)
    }

    async fn save_progress(&self, id: JobId, progress: JobProgress) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE jobs SET done = ?, failed = ?, total = ? WHERE id = ?")
            .bind(to_i64(progress.done)?)
            .bind(to_i64(progress.failed)?)
            .bind(progress.total.map(to_i64).transpose()?)
            .bind(id.get())
            .execute(self.write())
            .await
            .map_err(fail)?;
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
        sqlx::query(
            "UPDATE jobs SET status = ?, done = ?, failed = ?, total = ?, error = ?, \
             finished_at = ? WHERE id = ?",
        )
        .bind(status.as_str())
        .bind(to_i64(progress.done)?)
        .bind(to_i64(progress.failed)?)
        .bind(progress.total.map(to_i64).transpose()?)
        .bind(error)
        .bind(to_db(at))
        .bind(id.get())
        .execute(self.write())
        .await
        .map_err(fail)?;
        Ok(())
    }

    async fn get(&self, id: JobId) -> Result<Option<Job>, RepositoryError> {
        let row = sqlx::query(&format!("SELECT {JOB_COLUMNS} FROM jobs WHERE id = ?"))
            .bind(id.get())
            .fetch_optional(self.read())
            .await
            .map_err(fail)?;
        row.as_ref().map(job).transpose()
    }

    async fn list_running(&self) -> Result<Vec<Job>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "SELECT {JOB_COLUMNS} FROM jobs WHERE status = 'running' ORDER BY created_at, id"
        ))
        .fetch_all(self.read())
        .await
        .map_err(fail)?;
        rows.iter().map(job).collect()
    }

    async fn interrupt_running(&self, at: DateTime<Utc>) -> Result<u64, RepositoryError> {
        let done = sqlx::query(
            "UPDATE jobs SET status = 'interrupted', finished_at = ? WHERE status = 'running'",
        )
        .bind(to_db(at))
        .execute(self.write())
        .await
        .map_err(fail)?;
        Ok(done.rows_affected())
    }

    async fn delete_finished_before(&self, before: DateTime<Utc>) -> Result<u64, RepositoryError> {
        let done =
            sqlx::query("DELETE FROM jobs WHERE finished_at IS NOT NULL AND finished_at < ?")
                .bind(to_db(before))
                .execute(self.write())
                .await
                .map_err(fail)?;
        Ok(done.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};
    use domain::{Job, JobId, JobKind, JobProgress, JobStatus};
    use ports::{JobRepository, NewJob};

    use super::*;
    use crate::store::{Store, test_store};

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 26, 9, 0, 0).unwrap()
    }

    fn new_job(kind: &str, total: Option<u64>, at: DateTime<Utc>) -> NewJob {
        NewJob {
            kind: JobKind::new(kind).unwrap(),
            total,
            created_at: at,
        }
    }

    async fn make(store: &Store, kind: &str, at: DateTime<Utc>) -> Job {
        store.create(new_job(kind, None, at)).await.unwrap()
    }

    fn progress(done: u64, failed: u64, total: Option<u64>) -> JobProgress {
        JobProgress {
            done,
            failed,
            total,
        }
    }

    #[tokio::test]
    async fn migration_creates_jobs_table() {
        let (store, _dir) = test_store().await;
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs")
            .fetch_one(store.read())
            .await
            .unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn create_then_get_round_trip() {
        let (store, _dir) = test_store().await;
        for total in [None, Some(5)] {
            let created = store
                .create(new_job("labels_sync", total, t0()))
                .await
                .unwrap();
            assert_eq!(created.kind.as_str(), "labels_sync");
            assert_eq!(created.status, JobStatus::Running);
            assert_eq!(created.progress, progress(0, 0, total));
            assert_eq!(created.error, None);
            assert_eq!(created.created_at, t0());
            assert_eq!(created.finished_at, None);
            assert_eq!(store.get(created.id).await.unwrap(), Some(created));
        }
        assert_eq!(store.get(JobId::new(9999)).await.unwrap(), None);
    }

    #[tokio::test]
    async fn save_progress_updates_counters() {
        let (store, _dir) = test_store().await;
        let job = make(&store, "bulk", t0()).await;
        store
            .save_progress(job.id, progress(3, 1, Some(10)))
            .await
            .unwrap();
        let got = store.get(job.id).await.unwrap().unwrap();
        assert_eq!(got.progress, progress(3, 1, Some(10)));
        assert_eq!(got.status, JobStatus::Running);
    }

    #[tokio::test]
    async fn finish_sets_status_error_finished_at() {
        let (store, _dir) = test_store().await;
        let job = make(&store, "bulk", t0()).await;
        let at = t0() + Duration::minutes(2);
        store
            .finish(
                job.id,
                JobStatus::Failed,
                progress(4, 2, Some(6)),
                Some("provider_unavailable".to_owned()),
                at,
            )
            .await
            .unwrap();
        let got = store.get(job.id).await.unwrap().unwrap();
        assert_eq!(got.status, JobStatus::Failed);
        assert_eq!(got.progress, progress(4, 2, Some(6)));
        assert_eq!(got.error.as_deref(), Some("provider_unavailable"));
        assert_eq!(got.finished_at, Some(at));
    }

    #[tokio::test]
    async fn list_running_oldest_first_excludes_finished() {
        let (store, _dir) = test_store().await;
        let late = make(&store, "late", t0() + Duration::minutes(5)).await;
        let early = make(&store, "early", t0()).await;
        let done = make(&store, "done", t0()).await;
        store
            .finish(
                done.id,
                JobStatus::Succeeded,
                progress(1, 0, None),
                None,
                t0(),
            )
            .await
            .unwrap();
        let ids: Vec<JobId> = store
            .list_running()
            .await
            .unwrap()
            .iter()
            .map(|j| j.id)
            .collect();
        assert_eq!(ids, [early.id, late.id]);
    }

    #[tokio::test]
    async fn interrupt_touches_only_running() {
        let (store, _dir) = test_store().await;
        let running = make(&store, "running", t0()).await;
        let done = make(&store, "done", t0()).await;
        let done_at = t0() + Duration::minutes(1);
        store
            .finish(
                done.id,
                JobStatus::Succeeded,
                progress(1, 0, None),
                None,
                done_at,
            )
            .await
            .unwrap();
        let at = t0() + Duration::minutes(9);
        assert_eq!(store.interrupt_running(at).await.unwrap(), 1);
        let got = store.get(running.id).await.unwrap().unwrap();
        assert_eq!(got.status, JobStatus::Interrupted);
        assert_eq!(got.finished_at, Some(at));
        let untouched = store.get(done.id).await.unwrap().unwrap();
        assert_eq!(untouched.status, JobStatus::Succeeded);
        assert_eq!(untouched.finished_at, Some(done_at));
        assert_eq!(store.interrupt_running(at).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn delete_finished_before_keeps_running_and_recent() {
        let (store, _dir) = test_store().await;
        let cutoff = t0() + Duration::days(1);
        let old = make(&store, "old", t0()).await;
        let at_cutoff = make(&store, "at_cutoff", t0()).await;
        let recent = make(&store, "recent", t0()).await;
        let running = make(&store, "running", t0()).await;
        let end = |id, at| store.finish(id, JobStatus::Succeeded, JobProgress::default(), None, at);
        end(old.id, cutoff - Duration::milliseconds(1))
            .await
            .unwrap();
        end(at_cutoff.id, cutoff).await.unwrap();
        end(recent.id, cutoff + Duration::hours(1)).await.unwrap();
        assert_eq!(store.delete_finished_before(cutoff).await.unwrap(), 1);
        assert_eq!(store.get(old.id).await.unwrap(), None);
        for id in [at_cutoff.id, recent.id, running.id] {
            assert!(store.get(id).await.unwrap().is_some());
        }
    }

    #[tokio::test]
    async fn status_check_refuses_unknown_value() {
        let (store, _dir) = test_store().await;
        let inserted = sqlx::query(
            "INSERT INTO jobs (kind, status, created_at) \
             VALUES ('bulk', 'paused', '2026-11-26T09:00:00.000Z')",
        )
        .execute(store.write())
        .await;
        assert!(inserted.is_err());
    }
}
