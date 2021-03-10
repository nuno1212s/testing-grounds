use rio::{Rio, Ordering};
use socket2::{Protocol, Socket, Domain, Type};
use std::net::SocketAddr;
use std::io;

#[tokio::main]
async fn main() -> io::Result<()> {
    let ring = rio::new()?;
    let addr = "127.0.0.1:4321".parse().unwrap();
    let sock = connect(&ring, addr, Ordering::Link).await?;
    write(&ring, &sock, b"ganda cena mano\n", Ordering::Link).await?;
    Ok(())
}

async fn connect(ring: &Rio, addr: SocketAddr, order: Ordering) -> io::Result<Socket> {
    let domain = match addr {
        SocketAddr::V4(_) => Domain::ipv4(),
        SocketAddr::V6(_) => Domain::ipv6(),
    };
    let protocol = Some(Protocol::tcp());
    let socket = Socket::new(domain, Type::stream(), protocol)?;
    ring.connect(&socket, &addr, order).await?;
    Ok(socket)
}

async fn write<B: AsRef<[u8]>>(ring: &Rio, sock: &Socket, buf: B, order: Ordering) -> io::Result<usize> {
    ring.send_ordered(sock, &buf, order).await
}
