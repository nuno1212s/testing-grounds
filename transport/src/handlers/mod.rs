use std::io::{self, Read, Write};
use std::thread;
use std::error::Error;
use std::time::Duration;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use super::params;
use super::nodes::{Client, Server};
use futures::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};

pub fn server_test1_sync<S: Server>(mut server: S) -> Result<(), Box<dyn Error>> {
    while let Ok(mut client) = server.accept_client() {
        handle_read_sync(&mut client);
        handle_write_sync(&mut client);
    }
    Ok(())
}

pub fn client_test1_sync<C: Client>(mut clients: Vec<C>) -> Result<(), Box<dyn Error>> {
    let ops = testcase(move |quit| {
        let mut counter = 0;
        while !quit.load(Ordering::Relaxed) {
            for c in clients.iter_mut() {
                handle_write_sync(c);
                handle_read_sync(c);
            }
            counter += 1;
        }
        counter
    })?;
    Ok(())
}

fn handle_read_sync<R: Read>(mut r: R) -> io::Result<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    r.read(&mut buf[..])?;
    Ok(())
}

fn handle_write_sync<W: Write>(mut w: W) -> io::Result<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    w.write(&mut buf[..])?;
    Ok(())
}

async fn handle_read_async<R: AsyncRead + Unpin>(mut r: R) -> io::Result<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    r.read(&mut buf[..]).await?;
    Ok(())
}

async fn handle_write_async<W: AsyncWrite + Unpin>(mut w: W) -> io::Result<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    w.write(&mut buf[..]).await?;
    Ok(())
}

fn testcase<F>(job: F) -> thread::Result<u64>
where
    F: 'static + Send + FnOnce(Arc<AtomicBool>) -> u64
{
    let quit = Arc::new(AtomicBool::new(false));
    let quit_clone = quit.clone();
    let handle = thread::spawn(|| job(quit_clone));
    thread::sleep(TIME);
    quit.store(true, Ordering::Relaxed);
    handle.join()
}

fn ops_per_sec(ops: u64) -> f64 {
    (ops as f64) / (SECS as f64)
}
