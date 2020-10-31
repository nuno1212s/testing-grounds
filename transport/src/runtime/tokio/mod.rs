use std::future::Future;

use super::TaskOutput;
use lazy_static::lazy_static;

pub struct Runtime;

pub struct Task<T>(::tokio::task::JoinHandle<T>);

lazy_static! {
    static ref INSTANCE: ::tokio::runtime::Runtime = {
        let num_threads = std::env::var("THREADS")
            .as_ref()
            .map(String::as_ref)
            .unwrap_or("4")
            .parse()
            .unwrap_or(4);
        ::tokio::runtime::Builder::new_multi_thread()
            .worker_threads(num_threads)
            .thread_name("tokio-worker")
            .thread_stack_size(2 * 1024 * 1024)
            .enable_all()
            .build()
            .unwrap()
    };
}

impl super::Runtime for Runtime {
    type Task = Task<Option<TaskOutput>>;

    fn block_on<F: Future>(fut: F) -> F::Output {
        INSTANCE.block_on(fut)
    }

    fn spawn<F>(fut: F) -> Self::Task
    where
        F: 'static + Send + Future<Output = Option<TaskOutput>>,
    {
        Task(INSTANCE.spawn(fut))
    }
}

impl<T> Future for Task<T> {
    type Output = T;

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        match std::pin::Pin::new(&mut self.0).poll(cx) {
            std::task::Poll::Ready(x) => std::task::Poll::Ready(x.unwrap()),
            _ => std::task::Poll::Pending,
        }
    }
}
