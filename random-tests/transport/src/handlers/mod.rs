use std::io::{Read, Write};
use std::future::Future;
use std::thread;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{sync_channel, SyncSender},
};

use super::params;
use super::runtime;
use super::nodes::{Client, Server, AsyncClient, AsyncServer};
use futures_timer::Delay;
use futures::channel::mpsc;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use futures::channel::oneshot::{self, Sender};
use futures::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};

pub type Rs<T> = Result<T, Box<dyn std::error::Error>>;

// TODO: fix test1
pub fn server_test1_sync<S: Server>(server: S) -> Rs<()> {
    while let Ok(mut client) = server.accept_client() {
        read_sync(&mut client)?;
        write_sync(&mut client)?;
    }
    Ok(())
}

// TODO: fix test1
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
        let counter = Arc::new(AtomicU64::new(0));
        while !quit.load(Ordering::Relaxed) {
            match server.accept_client() {
                Ok(mut c) => {
                    let counter = Arc::clone(&counter);
                    thread::spawn(move || {
                        let _ = write_sync(&mut c);
                        let _ = read_sync(&mut c);
                        counter.fetch_add(1, Ordering::Relaxed);
                    });
                },
                _ => continue,
            };
        }
        Some(counter.load(Ordering::Relaxed))
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

pub async fn server_test2_async<S, R>(_runtime: R, server: S) -> Rs<f64>
where
    R: runtime::Runtime,
    S: 'static + AsyncServer + Send + Sync + Unpin,
    <S as AsyncServer>::Client: 'static + Send + Unpin,
{
    testcase_async(_runtime, move |ready, quit| async move {
        // synchronization phase
        {
            let mut c = server.accept_client_async().await.ok()?;
            write_async(&mut c).await.ok()?;
            read_async(&mut c).await.ok()?;
        }
        // first client connected, proceed with test
        ready.send(()).ok()?;
        let counter = Arc::new(AtomicU64::new(0));
        while !quit.load(Ordering::Relaxed) {
            match server.accept_client_async().await {
                Ok(mut c) => {
                    let counter = Arc::clone(&counter);
                    R::spawn(async move {
                        let _ = write_async(&mut c).await;
                        let _ = read_async(&mut c).await;
                        counter.fetch_add(1, Ordering::Relaxed);
                        Some(0)
                    });
                },
                _ => continue,
            };
        }
        Some(counter.load(Ordering::Relaxed))
    })
    .await
    .map(ops_per_sec)
}

pub fn client_test2_async<R, C, N, F>(_runtime: R, f: F) -> Rs<()>
where
    R: runtime::Runtime,
    C: 'static + AsyncClient + Send + Unpin,
    N: Future<Output = Rs<C>>,
    F: Fn() -> N,
{
    R::block_on(async move {
        // limit number of active connections
        let (mut tx, mut rx) = mpsc::channel(100);
        R::spawn(async move {
            while let Some(mut c) = rx.next().await {
                read_async(&mut c).await.ok()?;
                write_async(&mut c).await.ok()?;
            }
            Some(0)
        });
        while let Ok(c) = f().await {
            tx.send(c).await?;
        }
        Ok(())
    })
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

async fn testcase_async<F, R, T>(_runtime: R, job: F) -> Rs<runtime::TaskOutput>
where
    R: runtime::Runtime,
    T: 'static + Send + Future<Output = Option<runtime::TaskOutput>>,
    F: 'static + Send + Unpin + FnOnce(Sender<()>, Arc<AtomicBool>) -> T,
{
    let (tx, rx) = oneshot::channel();
    let quit = Arc::new(AtomicBool::new(false));
    let quit_clone = Arc::clone(&quit);
    let handle = R::spawn(job(tx, quit_clone));
    rx.await?; // ready to start test
    let timer = Delay::new(params::TIME);
    timer.await;
    quit.store(true, Ordering::Relaxed);
    handle.await.ok_or_else(|| "Task join failed.".into())
}

fn ops_per_sec(ops: u64) -> f64 {
    (ops as f64) / (params::SECS as f64)
}
