//! Job use cases. A job is a long operation tracked from start to end.
//!
//! To run one: [`StartJob::execute`] stores it and returns a [`JobHandle`]
//! (its [`id`](JobHandle::id) goes back to the client at once), then
//! [`JobHandle::run`] drives the work, typically from a spawned task. The
//! work receives a [`Progress`] and calls [`Progress::step`] once per item;
//! counters are saved and published to the event bus at most every
//! [`PROGRESS_INTERVAL`], and the end is always saved and published.

use chrono::TimeDelta;

mod handle;
mod queries;
mod recover;
mod start_job;
mod subscribe;

pub use handle::*;
pub use queries::*;
pub use recover::*;
pub use start_job::*;
pub use subscribe::*;

/// The least time between two saved and published progress updates of a job.
pub const PROGRESS_INTERVAL: TimeDelta = TimeDelta::milliseconds(300);

/// How long a finished job is kept before [`RecoverJobs`] purges it.
pub const JOB_RETENTION: TimeDelta = TimeDelta::days(30);

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{DateTime, TimeDelta, TimeZone, Utc};
    use domain::{Event, Job, JobId, JobKind, JobProgress, JobStatus};
    use futures_util::future::join_all;

    use super::*;
    use crate::Deps;
    use crate::testing::{
        FixedClock, InMemoryDevices, InMemoryJobs, InMemorySettings, RecordingBus, SeqSecrets,
    };

    struct Fixture {
        deps: Deps,
        jobs: Arc<InMemoryJobs>,
        bus: Arc<RecordingBus>,
        clock: Arc<FixedClock>,
    }

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
    }

    fn fixture() -> Fixture {
        let jobs = Arc::new(InMemoryJobs::new());
        let bus = Arc::new(RecordingBus::new());
        let clock = Arc::new(FixedClock::new(t0()));
        let deps = Deps {
            repo: Arc::new(InMemoryDevices::new()),
            clock: clock.clone(),
            secrets: Arc::new(SeqSecrets::new()),
            jobs: jobs.clone(),
            events: bus.clone(),
            settings: Arc::new(InMemorySettings::new()),
        };
        Fixture {
            deps,
            jobs,
            bus,
            clock,
        }
    }

    impl Fixture {
        async fn start(&self, total: Option<u64>) -> JobHandle {
            StartJob::new(self.deps.clone())
                .execute(JobKind::new("labels_sync").unwrap(), total)
                .await
                .unwrap()
        }

        async fn stored(&self, id: JobId) -> Job {
            GetJob::new(self.deps.clone())
                .execute(id)
                .await
                .unwrap()
                .unwrap()
        }

        fn job_events(&self) -> Vec<Job> {
            self.bus
                .events()
                .into_iter()
                .map(|e| match e {
                    Event::Job(job) => job,
                    other => panic!("unexpected event {other:?}"),
                })
                .collect()
        }
    }

    #[tokio::test]
    async fn start_creates_running_job_and_publishes() {
        let f = fixture();
        let handle = f.start(Some(5)).await;
        let job = f.stored(handle.id()).await;
        assert_eq!(job.status, JobStatus::Running);
        assert_eq!(job.kind.as_str(), "labels_sync");
        assert_eq!(job.progress.total, Some(5));
        assert_eq!(job.created_at, t0());
        assert_eq!(f.job_events(), vec![job]);
    }

    #[tokio::test]
    async fn first_step_publishes_second_within_interval_does_not() {
        let f = fixture();
        let handle = f.start(None).await;
        let status = handle
            .run(|progress| async move {
                progress.step(true).await;
                progress.step(true).await;
                Ok(())
            })
            .await;
        assert_eq!(status, JobStatus::Succeeded);
        // creation, first step, end: the second step was not published.
        let events = f.job_events();
        assert_eq!(events.len(), 3);
        assert_eq!(events[1].progress.done, 1);
        assert_eq!(f.jobs.save_count(), 1);
    }

    #[tokio::test]
    async fn step_after_interval_publishes() {
        let f = fixture();
        let handle = f.start(None).await;
        let clock = f.clock.clone();
        handle
            .run(|progress| async move {
                progress.step(true).await;
                clock.advance(PROGRESS_INTERVAL);
                progress.step(true).await;
                Ok(())
            })
            .await;
        let events = f.job_events();
        assert_eq!(events[2].progress.done, 2);
        assert_eq!(events[2].status, JobStatus::Running);
        assert_eq!(f.jobs.save_count(), 2);
    }

    #[tokio::test]
    async fn ok_end_saves_and_publishes_final_counters_within_interval() {
        let f = fixture();
        let handle = f.start(None).await;
        let id = handle.id();
        let status = handle
            .run(|progress| async move {
                for _ in 0..3 {
                    progress.step(true).await;
                }
                Ok(())
            })
            .await;
        assert_eq!(status, JobStatus::Succeeded);
        let job = f.stored(id).await;
        assert_eq!(job.status, JobStatus::Succeeded);
        assert_eq!(job.progress.done, 3);
        assert_eq!(job.finished_at, Some(t0()));
        assert_eq!(f.job_events().last(), Some(&job));
    }

    #[tokio::test]
    async fn err_end_carries_message() {
        let f = fixture();
        let handle = f.start(None).await;
        let id = handle.id();
        let status = handle.run(|_| async { Err(JobError("boom".into())) }).await;
        assert_eq!(status, JobStatus::Failed);
        let job = f.stored(id).await;
        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(job.error.as_deref(), Some("boom"));
        assert_eq!(f.job_events().last(), Some(&job));
    }

    #[tokio::test]
    async fn panicking_work_ends_failed() {
        let f = fixture();
        let handle = f.start(None).await;
        let id = handle.id();
        let status = handle
            .run(|_| async {
                if true {
                    panic!("invented panic of the work");
                }
                Ok(())
            })
            .await;
        assert_eq!(status, JobStatus::Failed);
        let job = f.stored(id).await;
        assert_eq!(job.error.as_deref(), Some("job panicked"));
    }

    #[tokio::test]
    async fn failed_step_counts_failure() {
        let f = fixture();
        let handle = f.start(None).await;
        let id = handle.id();
        handle
            .run(|progress| async move {
                progress.step(false).await;
                Ok(())
            })
            .await;
        let job = f.stored(id).await;
        assert_eq!((job.progress.done, job.progress.failed), (1, 1));
    }

    #[tokio::test]
    async fn set_total_updates_total() {
        let f = fixture();
        let handle = f.start(None).await;
        let id = handle.id();
        handle
            .run(|progress| async move {
                progress.set_total(7).await;
                Ok(())
            })
            .await;
        assert_eq!(f.stored(id).await.progress.total, Some(7));
    }

    #[tokio::test]
    async fn set_total_within_interval_is_not_published_but_ends_in_the_job() {
        let f = fixture();
        let handle = f.start(None).await;
        let id = handle.id();
        let clock = f.clock.clone();
        handle
            .run(|progress| async move {
                progress.step(true).await;
                clock.advance(TimeDelta::milliseconds(100));
                progress.set_total(7).await;
                Ok(())
            })
            .await;
        // creation, first step, end: `set_total` was throttled.
        let events = f.job_events();
        assert_eq!(events.len(), 3);
        assert_eq!(events[1].progress.total, None);
        assert_eq!(f.jobs.save_count(), 1);
        assert_eq!(events[2].progress.total, Some(7));
        assert_eq!(f.stored(id).await.progress.total, Some(7));
    }

    #[tokio::test]
    async fn throttle_counts_from_the_last_publish() {
        let f = fixture();
        let handle = f.start(None).await;
        let clock = f.clock.clone();
        handle
            .run(|progress| async move {
                // t: published; t+200 ms: throttled; t+400 ms: 400 ms after
                // the last publish, so published.
                progress.step(true).await;
                clock.advance(TimeDelta::milliseconds(200));
                progress.step(true).await;
                clock.advance(TimeDelta::milliseconds(200));
                progress.step(true).await;
                Ok(())
            })
            .await;
        let events = f.job_events();
        // creation, first step, third step, end.
        assert_eq!(events.len(), 4);
        assert_eq!(events[1].progress.done, 1);
        assert_eq!(events[2].progress.done, 3);
        assert_eq!(events[2].status, JobStatus::Running);
        assert_eq!(f.jobs.save_count(), 2);
    }

    #[tokio::test]
    async fn save_failure_does_not_stop_the_job() {
        let f = fixture();
        let handle = f.start(None).await;
        let id = handle.id();
        let jobs = f.jobs.clone();
        let status = handle
            .run(|progress| async move {
                jobs.fail_writes(true);
                progress.step(true).await;
                progress.step(true).await;
                jobs.fail_writes(false);
                Ok(())
            })
            .await;
        assert_eq!(status, JobStatus::Succeeded);
        let job = f.stored(id).await;
        assert_eq!(job.status, JobStatus::Succeeded);
        assert_eq!(job.progress.done, 2);
    }

    #[tokio::test]
    async fn concurrent_steps_lose_no_count() {
        let f = fixture();
        let handle = f.start(None).await;
        let id = handle.id();
        handle
            .run(|progress| async move {
                join_all((0..100).map(|_| progress.step(true))).await;
                Ok(())
            })
            .await;
        assert_eq!(f.stored(id).await.progress.done, 100);
    }

    #[tokio::test]
    async fn queries_list_running_only() {
        let f = fixture();
        let running = f.start(None).await;
        let done = f.start(None).await;
        done.run(|_| async { Ok(()) }).await;
        let active = ListActiveJobs::new(f.deps.clone()).execute().await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, running.id());
        assert!(
            GetJob::new(f.deps.clone())
                .execute(JobId::new(999))
                .await
                .unwrap()
                .is_none()
        );
    }

    fn finished(id: i64, ago: TimeDelta) -> Job {
        Job {
            id: JobId::new(id),
            kind: JobKind::new("labels_sync").unwrap(),
            status: JobStatus::Succeeded,
            progress: JobProgress::default(),
            error: None,
            created_at: t0() - ago,
            finished_at: Some(t0() - ago),
        }
    }

    #[tokio::test]
    async fn recover_interrupts_running_and_purges_at_30_days() {
        let f = fixture();
        let running = f.start(None).await;
        f.jobs
            .insert(finished(10, TimeDelta::days(30) + TimeDelta::seconds(1)));
        f.jobs.insert(finished(11, TimeDelta::days(29)));
        // "older than 30 days": exactly 30 days is kept, and so is just under.
        f.jobs.insert(finished(12, TimeDelta::days(30)));
        f.jobs
            .insert(finished(13, TimeDelta::days(30) - TimeDelta::seconds(1)));

        let recovered = RecoverJobs::new(f.deps.clone()).execute().await.unwrap();
        assert_eq!(
            recovered,
            RecoveredJobs {
                interrupted: 1,
                deleted: 1
            }
        );
        let job = f.stored(running.id()).await;
        assert_eq!(job.status, JobStatus::Interrupted);
        assert_eq!(job.finished_at, Some(t0()));
        let get = GetJob::new(f.deps.clone());
        assert!(get.execute(JobId::new(10)).await.unwrap().is_none());
        assert!(get.execute(JobId::new(11)).await.unwrap().is_some());
        assert!(get.execute(JobId::new(12)).await.unwrap().is_some());
        assert!(get.execute(JobId::new(13)).await.unwrap().is_some());
    }
}
