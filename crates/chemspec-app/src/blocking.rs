//! Platform boundary for finite synchronous work launched by Iced tasks.

/// Failure of the blocking worker itself, distinct from an operation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockingFailure {
    Panicked,
    Cancelled,
}

impl std::fmt::Display for BlockingFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Panicked => "background worker panicked",
            Self::Cancelled => "background worker was cancelled",
        })
    }
}

impl std::error::Error for BlockingFailure {}

impl From<BlockingFailure> for String {
    fn from(error: BlockingFailure) -> Self {
        error.to_string()
    }
}

/// Runs one synchronous operation without occupying an async runtime worker.
///
/// Native builds use Tokio's bounded blocking pool. The web demo cannot launch
/// the native provider and has no thread-backed blocking pool, so its Local
/// Mode work executes directly.
#[cfg(not(target_arch = "wasm32"))]
pub async fn run<T, E, F>(operation: F) -> Result<T, E>
where
    T: Send + 'static,
    E: From<BlockingFailure> + Send + 'static,
    F: FnOnce() -> Result<T, E> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| {
            if error.is_panic() {
                BlockingFailure::Panicked
            } else {
                BlockingFailure::Cancelled
            }
        })?
}

#[cfg(target_arch = "wasm32")]
pub async fn run<T, E, F>(operation: F) -> Result<T, E>
where
    T: 'static,
    E: From<BlockingFailure> + 'static,
    F: FnOnce() -> Result<T, E> + 'static,
{
    operation()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::run;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("test runtime")
    }

    #[test]
    fn native_work_runs_off_the_runtime_thread() {
        let runtime_thread = std::thread::current().id();
        let worker_thread = runtime()
            .block_on(run(|| Ok::<_, String>(std::thread::current().id())))
            .expect("blocking worker result");

        assert_ne!(worker_thread, runtime_thread);
    }

    #[test]
    fn operation_failures_remain_ordinary_results() {
        let result = runtime().block_on(run(|| Err::<(), _>("provider failed".to_owned())));

        assert_eq!(result, Err("provider failed".to_owned()));
    }
}
