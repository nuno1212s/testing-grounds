use std::future::Future;

pub trait Runtime {
    type Task: Future<Output = ()>;

    fn init(num_threads: usize) -> Result<(), Box<dyn std::error::Error>>;
    fn block_on<F: Future>(fut: F) -> F::Output;
    fn spawn<F: Future>(fut: F) -> Self::Task;
}
