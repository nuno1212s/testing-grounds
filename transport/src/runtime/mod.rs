use std::future::Future;

pub mod tokio;
pub mod async_std;

pub type TaskOutput = u64;

// should use THREADS env var to configure thread pool
pub trait Runtime {
    type Task: Future<Output = Option<TaskOutput>>;

    fn block_on<F: Future>(fut: F) -> F::Output;
    fn spawn<F: 'static + Send + Future<Output = Option<TaskOutput>>>(fut: F) -> Self::Task;
}
