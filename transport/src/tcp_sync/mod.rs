use bencher::Bencher;

use super::handlers::{
    handle_read_sync,
    handle_write_sync,
};

use std::thread;
use std::sync::mpsc;
use std::net::{TcpListener, TcpStream};

pub fn bench_write(b: &mut Bencher) {
    const ADDR: &str = "127.0.0.1:12345";
    let (tx, rx) = mpsc::sync_channel(0);
    thread::spawn(move || {
        let listener = TcpListener::bind(ADDR)
            .expect("failed to listen on addr");
        tx.send(()).unwrap();
        for stream in listener.incoming().take(100) {
            let stream = stream
                .expect("failed to create new TCP stream");
            handle_read_sync(stream)
                .expect("failed to read");
        }
        tx.send(()).unwrap();
    });
    rx.recv().unwrap();
    b.iter(|| {
        let stream = TcpStream::connect(ADDR)
            .expect("failed to connect to addr");
        handle_write_sync(stream)
            .expect("failed to write");
    });
    rx.recv().unwrap()
}
