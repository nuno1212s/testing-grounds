use std::future::Future;
use async_trait::async_trait;
use futures::channel::oneshot;

pub type TaskOutput = Option<u64>;

// should use THREADS env var to configure thread pool
pub trait Runtime {
    type Task: Future<Output = Option<TaskOutput>>;

    fn block_on<F: Future>(fut: F) -> F::Output;
    fn spawn<F: Future<Output = Option<TaskOutput>>>(fut: F) -> Self::Task;
}
