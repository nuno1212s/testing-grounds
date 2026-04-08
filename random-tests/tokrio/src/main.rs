#![allow(dead_code)]
#![allow(unused_imports)]

use rio::{Rio, Ordering};
use socket2::{Protocol, Socket, Domain, Type};
use tokio::time::{sleep_until, Instant, Duration};
use std::net::{TcpStream, TcpListener, SocketAddr};
use std::sync::atomic::{self, AtomicUsize};
use std::sync::Arc;
use std::task::Poll;
use std::fmt::Write;
use std::io;

const WORKERS: usize = 16;

#[tokio::main]
async fn main() -> io::Result<()> {
    let ring = rio::new()?;
    let addr = "127.0.0.1:4321".parse().unwrap();
    let counter = Arc::new(AtomicUsize::new(0));
    let mut sockets = Vec::new();
    println!("Establishing {} connections...", WORKERS);
    for id in 0..WORKERS {
        let sock = connect(&ring, addr, Ordering::Link).await?;
        sockets.push((id, sock));
    }
    println!("Running test for 5 seconds...");
    {
        let ring = ring;
        let sockets = sockets;
        for (id, sock) in sockets {
            tokio::spawn(write_loop(id, Arc::clone(&counter), ring.clone(), sock));
        }
    }
    sleep_until(Instant::now() + Duration::from_secs(5)).await;
    let result = counter.load(atomic::Ordering::Relaxed);
    println!("Got a throughput of {} writes per sec.", result / 5);
    Ok(())
}

async fn write_loop(id: usize, c: Arc<AtomicUsize>, ring: Rio, sock: TcpStream) {
    let mut buf = String::new();
    loop {
        match write!(&mut buf, "{} -> I'm here!\n", id) {
            Ok(_) => (),
            _ => unreachable!(),
        };
        let result = write(&ring, &sock, &buf, Ordering::Link)
            .await;
        if let Err(_) = result {
            return;
        }
        buf.clear();
        c.fetch_add(1, atomic::Ordering::Relaxed);
    }
}

async fn connect(ring: &Rio, addr: SocketAddr, order: Ordering) -> io::Result<TcpStream> {
    let domain = match addr {
        SocketAddr::V4(_) => Domain::ipv4(),
        SocketAddr::V6(_) => Domain::ipv6(),
    };
    let protocol = Some(Protocol::tcp());
    let socket = Socket::new(domain, Type::stream(), protocol)?;
    ring.connect(&socket, &addr, order).await?;
    Ok(socket.into())
}

#[inline(always)]
fn listen(addr: SocketAddr) -> io::Result<TcpListener> {
    TcpListener::bind(addr)
}

#[inline(always)]
async fn accept(ring: &Rio, ls: &TcpListener) -> io::Result<TcpStream> {
    ring.accept(&ls).await
}

#[inline(always)]
async fn read<B: AsRef<[u8]> + AsMut<[u8]>>(ring: &Rio, sock: &TcpStream, buf: B, order: Ordering) -> io::Result<usize> {
    ring.recv_ordered(sock, &buf, order).await
}

#[inline(always)]
async fn write<B: AsRef<[u8]>>(ring: &Rio, sock: &TcpStream, buf: B, order: Ordering) -> io::Result<usize> {
    ring.send_ordered(sock, &buf, order).await
}
