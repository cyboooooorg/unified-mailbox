//! Server binary: reads the configuration once, wires the adapters into
//! the use cases and serves the HTTP API.

mod cli;
mod config;
mod env;
mod system;

use std::fs::DirBuilder;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use application::{CreatePairingCode, Deps, FlushJobEnds, RecoverJobs, UnsavedJobEnds};
use chrono::SecondsFormat;
use cli::Command;
use config::Config;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    // Arguments first: a typo must not create the data directory.
    let command = match cli::parse(std::env::args_os().skip(1)) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("{message}\n{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };

    let file = match env::read_dotenv(Path::new(".env")) {
        Ok(file) => file,
        Err(err) => {
            eprintln!(".env: {err}");
            return ExitCode::FAILURE;
        }
    };
    let config = match Config::from_vars(&env::overlay(file, std::env::vars_os())) {
        Ok(config) => config,
        Err(errors) => {
            eprintln!("configuration errors:\n{errors}");
            return ExitCode::FAILURE;
        }
    };

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(&config.log_filter))
        // Logs go to stderr: stdout holds only what a command prints, such
        // as the two lines of `pair-code`.
        .with_writer(std::io::stderr)
        .init();

    // Fatal errors go to stderr: a `RUST_LOG` that filters this crate out
    // must not hide why the server stopped.
    if let Err(err) = prepare_dirs(&config) {
        eprintln!("{err}");
        return ExitCode::FAILURE;
    }
    let store = match sqlite::Store::open(config.store.clone()).await {
        Ok(store) => Arc::new(store),
        Err(err) => {
            eprintln!("cannot open the store: {err}");
            return ExitCode::FAILURE;
        }
    };
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        dry_run = config.dry_run,
        spam_threshold = config.spam.threshold.get(),
        protect_known_senders = config.spam.protect_known_senders,
        archive = ?config.spam.archive.iter().map(|key| key.as_str()).collect::<Vec<_>>(),
        "store ready"
    );
    let deps = Deps {
        repo: store.clone(),
        clock: Arc::new(system::SystemClock),
        secrets: Arc::new(system::OsSecrets),
        jobs: store.clone(),
        events: Arc::new(events_memory::MemoryBus::new(events_memory::CAPACITY)),
        settings: store.clone(),
        job_ends: Arc::new(UnsavedJobEnds::new()),
    };

    let code = match command {
        Command::Serve => serve(&config, deps).await,
        Command::PairCode => pair_code(deps).await,
    };
    store.close().await;
    code
}

async fn serve(config: &Config, deps: Deps) -> ExitCode {
    let listener = match tokio::net::TcpListener::bind(config.listen_addr).await {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!(
                "LISTEN_ADDR: cannot listen on {}: {err}",
                config.listen_addr
            );
            return ExitCode::FAILURE;
        }
    };
    // After binding: a second server started by mistake on the same data
    // directory fails to bind and leaves the live server's jobs alone.
    // Before serving: what was running died with the previous process.
    match RecoverJobs::new(deps.clone()).execute().await {
        Ok(recovered) => tracing::info!(
            interrupted = recovered.interrupted,
            deleted = recovered.deleted,
            "jobs recovered"
        ),
        Err(err) => {
            eprintln!("cannot recover the jobs: {err}");
            return ExitCode::FAILURE;
        }
    }
    tracing::info!(addr = %config.listen_addr, "listening");
    // Kept alive for the server's lifetime: dropping it would end the
    // streams. The graceful shutdown sends `true` on it first, so open
    // `/events` streams end and the server can finish.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    if let Some(dir) = &config.client_dir {
        tracing::info!(client_dir = %dir.display(), "serving the web client");
    }
    let retry = tokio::spawn(retry_job_ends(deps.clone(), JOB_ENDS_RETRY));
    let app = http_api::router_with_client(
        http_api::AppState::new(deps.clone(), shutdown_rx),
        config.client_dir.clone().map(http_api::ClientDir::new),
    );
    let served = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            let _ = shutdown_tx.send(true);
        })
        .await;
    stop_retrying_job_ends(&retry, deps).await;
    match served {
        Ok(()) => {
            tracing::info!("stopped");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("server error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// How often [`retry_job_ends`] writes the job ends the store refused.
const JOB_ENDS_RETRY: Duration = Duration::from_secs(30);

/// Writes the job ends the store refused again, every `period` (the first
/// time one `period` after the start), until the task is aborted.
async fn retry_job_ends(deps: Deps, period: Duration) {
    let flush = FlushJobEnds::new(deps);
    let mut ticks = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        flush.execute().await;
    }
}

/// Stops the `retry` task, then tries the refused job ends once more,
/// whatever ended the server. Returns how many are still unsaved: they are
/// lost, and found `interrupted` at the next start.
async fn stop_retrying_job_ends(retry: &tokio::task::JoinHandle<()>, deps: Deps) -> usize {
    retry.abort();
    let unsaved = FlushJobEnds::new(deps).execute().await;
    if unsaved > 0 {
        tracing::warn!(unsaved, "job ends lost: marked interrupted at next start");
    }
    unsaved
}

async fn pair_code(deps: Deps) -> ExitCode {
    match CreatePairingCode::new(deps).execute(None).await {
        Ok(created) => {
            println!("Pairing code: {}", created.code);
            println!(
                "Valid until {} (10 minutes, single use).",
                created
                    .expires_at
                    .to_rfc3339_opts(SecondsFormat::Secs, true)
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("cannot create the pairing code: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Resolves on SIGINT or SIGTERM.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = signal(SignalKind::terminate()).expect("install the SIGTERM handler");
        let mut int = signal(SignalKind::interrupt()).expect("install the SIGINT handler");
        tokio::select! {
            _ = term.recv() => {}
            _ = int.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Creates `DATA_DIR` (mode `0700`) when missing and checks that the key
/// file has a directory to live in. An existing `DATA_DIR` keeps its
/// permissions. The store creates the files themselves.
fn prepare_dirs(config: &Config) -> Result<(), String> {
    let data_dir = &config.data_dir;
    if !data_dir.exists() {
        let mut builder = DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder
            .create(data_dir)
            .map_err(|err| format!("DATA_DIR: cannot create {}: {err}", data_dir.display()))?;
    }
    if !data_dir.is_dir() {
        return Err(format!(
            "DATA_DIR: {} is not a directory",
            data_dir.display()
        ));
    }
    // A bare file name has an empty parent: the working directory.
    let key_dir = match config.store.key_file.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    if !key_dir.is_dir() {
        return Err(format!(
            "KEY_FILE: the directory {} does not exist",
            key_dir.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use application::testing::{
        FixedClock, InMemoryDevices, InMemoryJobs, InMemorySettings, RecordingBus, SeqSecrets,
    };
    use application::{GetJob, StartJob};
    use chrono::TimeZone;
    use domain::{JobId, JobKind, JobStatus};
    use ports::JobRepository;

    use super::*;
    use crate::config::Config;

    /// `Deps` over in-memory doubles, and the job store to make it fail.
    fn job_deps() -> (Deps, Arc<InMemoryJobs>) {
        let jobs = Arc::new(InMemoryJobs::new());
        let deps = Deps {
            repo: Arc::new(InMemoryDevices::new()),
            clock: Arc::new(FixedClock::new(
                chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            )),
            secrets: Arc::new(SeqSecrets::new()),
            jobs: jobs.clone(),
            events: Arc::new(RecordingBus::new()),
            settings: Arc::new(InMemorySettings::new()),
            job_ends: Arc::new(UnsavedJobEnds::new()),
        };
        (deps, jobs)
    }

    /// Runs a job to success while the store refuses its end.
    async fn refused_end(deps: &Deps, jobs: &InMemoryJobs) -> JobId {
        let handle = StartJob::new(deps.clone())
            .execute(JobKind::new("labels_sync").unwrap(), None)
            .await
            .unwrap();
        let id = handle.id();
        jobs.fail_writes(true);
        handle.run(|_| async { Ok(()) }).await;
        jobs.fail_writes(false);
        id
    }

    async fn status_in_store(jobs: &InMemoryJobs, id: JobId) -> JobStatus {
        jobs.get(id).await.unwrap().unwrap().status
    }

    /// Lets the spawned task run while the paused clock stands still.
    async fn settle() {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn refused_end_is_written_again_after_one_period() {
        let (deps, jobs) = job_deps();
        let id = refused_end(&deps, &jobs).await;
        let retry = tokio::spawn(retry_job_ends(deps.clone(), JOB_ENDS_RETRY));

        settle().await;
        tokio::time::advance(JOB_ENDS_RETRY - Duration::from_millis(1)).await;
        settle().await;
        assert_eq!(status_in_store(&jobs, id).await, JobStatus::Running);

        tokio::time::advance(Duration::from_millis(1)).await;
        settle().await;
        assert_eq!(status_in_store(&jobs, id).await, JobStatus::Succeeded);
        assert!(deps.job_ends.is_empty());
        retry.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn stopping_aborts_the_retry_and_writes_the_ends_once_more() {
        let (deps, jobs) = job_deps();
        let id = refused_end(&deps, &jobs).await;
        let retry = tokio::spawn(retry_job_ends(deps.clone(), JOB_ENDS_RETRY));

        assert_eq!(stop_retrying_job_ends(&retry, deps.clone()).await, 0);
        assert_eq!(status_in_store(&jobs, id).await, JobStatus::Succeeded);
        assert!(retry.await.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn stopping_counts_the_ends_still_refused() {
        let (deps, jobs) = job_deps();
        let id = refused_end(&deps, &jobs).await;
        let retry = tokio::spawn(std::future::pending::<()>());
        jobs.fail_writes(true);

        assert_eq!(stop_retrying_job_ends(&retry, deps.clone()).await, 1);
        let kept = GetJob::new(deps).execute(id).await.unwrap().unwrap();
        assert_eq!(kept.status, JobStatus::Succeeded);
        assert_eq!(status_in_store(&jobs, id).await, JobStatus::Running);
    }

    fn config(data_dir: &Path, key_file: Option<&Path>) -> Config {
        let mut vars = std::collections::HashMap::new();
        vars.insert("DATA_DIR".to_owned(), data_dir.display().to_string());
        if let Some(key_file) = key_file {
            vars.insert("KEY_FILE".to_owned(), key_file.display().to_string());
        }
        Config::from_vars(&vars).unwrap()
    }

    #[test]
    fn prepare_dirs_creates_data_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("nested/data");
        prepare_dirs(&config(&data_dir, None)).unwrap();
        assert!(data_dir.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&data_dir).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
    }

    #[cfg(unix)]
    #[test]
    fn prepare_dirs_keeps_existing_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        fs::create_dir(&data_dir).unwrap();
        fs::set_permissions(&data_dir, fs::Permissions::from_mode(0o755)).unwrap();
        prepare_dirs(&config(&data_dir, None)).unwrap();
        let mode = fs::metadata(&data_dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    #[test]
    fn prepare_dirs_rejects_key_parent_file() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("not-a-dir");
        fs::write(&parent, "").unwrap();
        let err = prepare_dirs(&config(
            &tmp.path().join("data"),
            Some(&parent.join("secret.key")),
        ))
        .unwrap_err();
        assert!(err.contains("KEY_FILE"), "{err}");
    }

    #[test]
    fn prepare_dirs_rejects_file() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        fs::write(&data_dir, "").unwrap();
        let err = prepare_dirs(&config(&data_dir, None)).unwrap_err();
        assert!(err.contains("DATA_DIR"), "{err}");
    }

    #[test]
    fn prepare_dirs_rejects_missing_key_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let key_file = tmp.path().join("missing/secret.key");
        let err = prepare_dirs(&config(&tmp.path().join("data"), Some(&key_file))).unwrap_err();
        assert!(err.contains("KEY_FILE"), "{err}");
    }

    #[test]
    fn prepare_dirs_accepts_bare_key_file() {
        let tmp = tempfile::tempdir().unwrap();
        prepare_dirs(&config(
            &tmp.path().join("data"),
            Some(Path::new("secret.key")),
        ))
        .unwrap();
    }
}
