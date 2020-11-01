use std::future::Future;

use super::TaskOutput;
use async_std::task;

pub struct Runtime;

impl Runtime {
    fn init() {
        match std::env::var("THREADS").as_ref() {
            Ok(ref n) => std::env::set_var("ASYNC_STD_THREAD_COUNT", &n),
            _ => (),
        }
    }
}

impl super::Runtime for Runtime {
    type Task = task::JoinHandle<Option<TaskOutput>>;

    fn block_on<F: Future>(fut: F) -> F::Output {
        task::block_on(fut)
    }

    fn spawn<F>(fut: F) -> Self::Task
    where
        F: 'static + Send + Future<Output = Option<TaskOutput>>,
    {
        task::spawn(fut)
    }
}
