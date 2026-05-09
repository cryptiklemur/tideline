//! Multi-transport tracing init shared by the host and every plugin.
//!
//! Layered subscriber:
//!   - human-readable layer to stderr (so the dev terminal stays readable)
//!   - JSON layer to a daily-rolled file under `<cache>/tideline/logs/`
//!     (one file per process, named after `process_name`)
//!
//! Both layers respect `RUST_LOG`. If unset, defaults to
//! `info,tideline=debug,tideline_effects=debug,tideline_sdk=debug,tideline_host=debug`.

use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt, Layer};

const DEFAULT_FILTER: &str =
    "info,tideline=debug,tideline_effects=debug,tideline_sdk=debug,tideline_host=debug,tideline_core=debug";

pub fn log_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| std::env::temp_dir())
        .join("tideline")
        .join("logs")
}

/// Init the global tracing subscriber. Returns a guard that must be kept
/// alive for the duration of the process — dropping it stops the background
/// writer thread and any buffered lines are lost.
///
/// `process_name` is used as the filename stem (e.g. "tideline" → tideline.log).
pub fn init(process_name: &str) -> Option<WorkerGuard> {
    let dir = log_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        // Fall back to stderr-only if the dir can't be created.
        eprintln!("logging: cant create {}: {e}; using stderr only", dir.display());
        let stderr_layer = fmt::layer()
            .with_writer(std::io::stderr)
            .with_target(true)
            .with_filter(env_filter());
        let _ = tracing_subscriber::registry().with(stderr_layer).try_init();
        return None;
    }

    let file_appender = tracing_appender::rolling::daily(&dir, format!("{process_name}.log"));
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(true)
        .with_filter(env_filter());

    let json_layer = fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(false)
        .with_target(true)
        .with_writer(file_writer)
        .with_filter(env_filter());

    let res = tracing_subscriber::registry()
        .with(stderr_layer)
        .with(json_layer)
        .try_init();

    if let Err(e) = res {
        eprintln!("logging: try_init failed: {e}");
        return None;
    }

    tracing::info!(
        process = process_name,
        log_file = %dir.join(format!("{process_name}.log")).display(),
        "logging initialized"
    );
    Some(guard)
}

fn env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER))
}
