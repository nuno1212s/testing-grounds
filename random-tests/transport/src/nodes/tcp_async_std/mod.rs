use crate::nodes;

use std::io;
use std::pin::Pin;
use std::task::{Poll, Context};

use async_trait::async_trait;
use async_std::io::{Read, Write};
use async_std::net::{TcpStream, TcpListener};

pub struct C(TcpStream);
pub struct S(TcpListener);

impl Read for C {
    fn poll_read(
        mut self: Pin<&mut Self>, 
        cx: &mut Context<'_>, 
        buf: &mut [u8]
    ) -> Poll<io::Result<usize>>
    {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl Write for C {
    fn poll_write(
        mut self: Pin<&mut Self>, 
        cx: &mut Context<'_>, 
        buf: &[u8]
    ) -> Poll<io::Result<usize>>
    {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>, 
        cx: &mut Context<'_>
    ) -> Poll<io::Result<()>>
    {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_close(
        mut self: Pin<&mut Self>, 
        cx: &mut Context<'_>
    ) -> Poll<io::Result<()>>
    {
        Pin::new(&mut self.0).poll_close(cx)
    }
}

#[async_trait]
impl nodes::AsyncClient for C {
    type Addr = &'static str;

    async fn connect_server_async(addr: Self::Addr) -> io::Result<Self> {
        TcpStream::connect(addr)
            .await
            .map(C)
    }
}

#[async_trait]
impl nodes::AsyncServer for S {
    type Client = C;

    async fn listen_clients_async(addr: <<Self as nodes::AsyncServer>::Client as nodes::AsyncClient>::Addr) -> io::Result<Self> {
        TcpListener::bind(addr)
            .await
            .map(S)
    }

    async fn accept_client_async(&self) -> io::Result<Self::Client> {
        self.0
            .accept()
            .await
            .map(|(c, _)| C(c))
    }
}
