use std::io::{Read, Write};
use std::future::Future;
use std::pin::Pin;
use std::thread;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{sync_channel, SyncSender},
};

use super::params;
use super::runtime;
use super::nodes::{Client, Server};
use futures_timer::Delay;
use futures::channel::oneshot::{self, Sender};
use futures::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};

pub type Rs<T> = Result<T, Box<dyn std::error::Error>>;

pub fn server_test1_sync<S: Server>(server: S) -> Rs<()> {
    while let Ok(mut client) = server.accept_client() {
        read_sync(&mut client)?;
        write_sync(&mut client)?;
    }
    Ok(())
}

pub fn client_test1_sync<C>(mut clients: Vec<C>) -> Rs<f64>
where
    C: 'static + Client + Send,
{
    testcase(move |_ready, quit| {
        let mut counter = 0;
        while !quit.load(Ordering::Relaxed) {
            for mut c in clients.iter_mut() {
                write_sync(&mut c).ok()?;
                read_sync(&mut c).ok()?;
            }
            counter += 1;
        }
        Some(counter)
    })
    .map(ops_per_sec)
}

pub fn server_test2_sync<S>(server: S) -> Rs<f64>
where
    S: 'static + Server + Send,
    <S as Server>::Client: 'static + Send,
{
    testcase(move |ready, quit| {
        // synchronization phase
        {
            let mut c = server.accept_client().ok()?;
            write_sync(&mut c).ok()?;
            read_sync(&mut c).ok()?;
        }
        // first client connected, proceed with test
        ready.send(()).ok()?;
        let mut counter = 0;
        while !quit.load(Ordering::Relaxed) {
            match server.accept_client() {
                Ok(mut c) => thread::spawn(move || {
                    write_sync(&mut c)
                        .and_then(|_| read_sync(&mut c))
                        .map_err(|_| ())
                }),
                _ => continue,
            };
            counter += 1;
        }
        Some(counter)
    })
    .map(ops_per_sec)
}

pub fn client_test2_sync<C, F>(f: F) -> Rs<()>
where
    C: 'static + Client + Send,
    F: Fn() -> Rs<C>,
{
    while let Ok(mut c) = f() {
        read_sync(&mut c)?;
        write_sync(&mut c)?;
    }
    Ok(())
}

fn read_sync<R: Read>(mut r: R) -> Rs<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    r.read(&mut buf[..])?;
    Ok(())
}

fn write_sync<W: Write>(mut w: W) -> Rs<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    w.write(&mut buf[..])?;
    Ok(())
}

async fn read_async<R: AsyncRead + Unpin>(mut r: R) -> Rs<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    r.read(&mut buf[..]).await?;
    Ok(())
}

async fn write_async<W: AsyncWrite + Unpin>(mut w: W) -> Rs<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    w.write(&mut buf[..]).await?;
    Ok(())
}

fn testcase<F>(job: F) -> Rs<u64>
where
    F: 'static + Send + FnOnce(SyncSender<()>, Arc<AtomicBool>) -> Option<u64>
{
    let (tx, rx) = sync_channel(0);
    let quit = Arc::new(AtomicBool::new(false));
    let quit_clone = quit.clone();
    let handle = thread::spawn(|| job(tx, quit_clone));
    rx.recv()?; // ready to start test
    thread::sleep(params::TIME);
    quit.store(true, Ordering::Relaxed);
    handle
        .join()
        .map_err(|_| "Thread join failed.".into())
        .and_then(|opt| opt.ok_or_else(|| "Thread ran into a problem.".into()))
}

fn testcase_async<F, R>(_runtime: R, job: F) -> Rs<runtime::TaskOutput>
where
    R: runtime::Runtime,
    F: 'static + Send + FnOnce(Sender<()>, Arc<AtomicBool>) -> Pin<Box<dyn Future<Output = Option<runtime::TaskOutput>>>>,
{
    R::block_on(async move {
        let (tx, rx) = oneshot::channel();
        let quit = Arc::new(AtomicBool::new(false));
        let quit_clone = quit.clone();
        let handle = R::spawn(job(tx, quit_clone));
        rx.await?; // ready to start test
        let timer = Delay::new(params::TIME);
        timer.await;
        quit.store(true, Ordering::Relaxed);
        handle.await.ok_or_else(|| "Thread join failed.".into())
    })
}

fn ops_per_sec(ops: u64) -> f64 {
    (ops as f64) / (params::SECS as f64)
}
