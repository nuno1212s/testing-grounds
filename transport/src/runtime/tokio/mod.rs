use lazy_static::lazy_static;
use futures::channel::oneshot;

pub struct Runtime;

lazy_static! {
    static ref INSTANCE: tokio::runtime::Runtime = {
        let num_threads = std::env::var("THREADS")
            .unwrap_or("4")
            .parse()
            .unwrap_or(4);
        Builder::new_multi_thread()
            .worker_threads(num_threads)
            .thread_name("tokio-worker")
            .thread_stack_size(2 * 1024 * 1024)
            .build()
            .unwrap()
    };
}

//impl super::Runtime for Runtime {
//    fn block_on<F: Future>(fut: F) -> F::Output {
//        asd
//    }
//
//    fn spawn<F: Future>(fut: F) -> super::Task<Result<F::Output, oneshot::Canceled>> {
//        let (tx, rx) = oneshot::channel();
//        INSTANCE.spawn(
//        rx
//    }
//}
