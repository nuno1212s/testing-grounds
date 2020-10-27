use std::io::{self, Read, Write};
use std::thread;
use std::time::Duration;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use super::params;
use super::nodes::{Client, Server};
use futures::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};

pub type Rs<T> = Result<T, Box<dyn std::error::Error>>;

pub fn server_test1_sync<S: Server>(mut server: S) -> Rs<()> {
    while let Ok(mut client) = server.accept_client() {
        handle_read_sync(&mut client);
        handle_write_sync(&mut client);
    }
    Ok(())
}

pub fn client_test1_sync<C: 'static + Client + Send>(mut clients: Vec<C>) -> Rs<f64> {
    let ops = testcase(move |quit| {
        let mut counter = 0;
        while !quit.load(Ordering::Relaxed) {
            for mut c in clients.iter_mut() {
                handle_write_sync(&mut c);
                handle_read_sync(&mut c);
            }
            counter += 1;
        }
        counter
    })?;
    Ok(ops_per_sec(ops))
}

fn handle_read_sync<R: Read>(mut r: R) -> Rs<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    r.read(&mut buf[..])?;
    Ok(())
}

fn handle_write_sync<W: Write>(mut w: W) -> Rs<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    w.write(&mut buf[..])?;
    Ok(())
}

async fn handle_read_async<R: AsyncRead + Unpin>(mut r: R) -> Rs<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    r.read(&mut buf[..]).await?;
    Ok(())
}

async fn handle_write_async<W: AsyncWrite + Unpin>(mut w: W) -> Rs<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    w.write(&mut buf[..]).await?;
    Ok(())
}

fn testcase<F>(job: F) -> Rs<u64>
where
    F: 'static + Send + FnOnce(Arc<AtomicBool>) -> u64
{
    let quit = Arc::new(AtomicBool::new(false));
    let quit_clone = quit.clone();
    let handle = thread::spawn(|| job(quit_clone));
    thread::sleep(params::TIME);
    quit.store(true, Ordering::Relaxed);
    handle.join().map_err(|_| "Join failed!".into())
}

fn ops_per_sec(ops: u64) -> f64 {
    (ops as f64) / (params::SECS as f64)
}
