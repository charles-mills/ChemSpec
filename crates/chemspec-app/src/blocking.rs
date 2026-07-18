//! Platform boundary for finite synchronous work launched by Iced tasks.

/// Runs one synchronous operation without occupying an async runtime worker.
///
/// Native builds use Tokio's bounded blocking pool. The web demo cannot launch
/// the native provider and has no thread-backed blocking pool, so its Local
/// Mode work executes directly.
#[cfg(not(target_arch = "wasm32"))]
pub async fn run<T, F>(operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| {
            if error.is_panic() {
                "background worker panicked".to_owned()
            } else {
                "background worker was cancelled".to_owned()
            }
        })?
}

#[cfg(target_arch = "wasm32")]
pub async fn run<T, F>(operation: F) -> Result<T, String>
where
    T: 'static,
    F: FnOnce() -> Result<T, String> + 'static,
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
            .block_on(run(|| Ok(std::thread::current().id())))
            .expect("blocking worker result");

        assert_ne!(worker_thread, runtime_thread);
    }

    #[test]
    fn operation_failures_remain_ordinary_results() {
        let result = runtime().block_on(run(|| Err::<(), _>("provider failed".to_owned())));

        assert_eq!(result, Err("provider failed".to_owned()));
    }
}
