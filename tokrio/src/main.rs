#![allow(dead_code)]
#![allow(unused_imports)]

use rio::{Rio, Ordering};
use futures_lite::future;
use socket2::{Protocol, Socket, Domain, Type};
use std::net::{TcpStream, TcpListener, SocketAddr};
use std::fmt::Write;
use std::io;

const WORKERS: usize = 16;

#[tokio::main]
async fn main() -> io::Result<()> {
    let ring = rio::new()?;
    let addr = "127.0.0.1:4321".parse().unwrap();
    for i in 0..WORKERS {
        let sock = connect(&ring, addr, Ordering::Link).await?;
        tokio::spawn(write_loop(i, ring.clone(), sock));
    }
    drop(ring);
    future::pending().await
}

async fn write_loop(id: usize, ring: Rio, sock: TcpStream) {
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
async fn read<B: AsRef<[u8]> + AsMut<[u8]>>(ring: &Rio, sock: &TcpStream, buf: B, order: Ordering) -> io::Result<usize> {
    ring.recv_ordered(sock, &buf, order).await
}

#[inline(always)]
async fn write<B: AsRef<[u8]>>(ring: &Rio, sock: &TcpStream, buf: B, order: Ordering) -> io::Result<usize> {
    ring.send_ordered(sock, &buf, order).await
}
