use bencher::Bencher;

use std::thread;
use std::sync::mpsc;

pub fn bench_write(b: &mut Bencher) {
    let (tx, rx) = sync_channel(0);
    thread::spawn(move || {
        const ADDR: &str = "127.0.0.1:12345";
        let listener = TcpListener::bind(ADDR)
            .expect("failed to listen on " + ADDR);
        for stream in listener.incoming() {
            // make this identical for each test
            handle_client(stream?);
        }
        tx.send(()).unwrap();
    });
    b.iter(|| {
        // placeholder
        x += 1;
    });
    rx.recv().unwrap()
}
