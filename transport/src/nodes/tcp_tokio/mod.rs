use crate::nodes;

use std::io;
use std::pin::Pin;
use std::task::{Poll, Context};

use tokio::net::{TcpListener, TcpStream};

use async_trait::async_trait;
use futures::io::{AsyncRead, AsyncWrite};
use futures::compat::{
    Compat01As03,
    AsyncRead01CompatExt,
    AsyncWrite01CompatExt,
};

pub struct C(Compat01As03<TcpStream>);
pub struct S(Compat01As03<TcpListener>);

impl AsyncRead for C {
    fn poll_read(
        mut self: Pin<&mut Self>, 
        cx: &mut Context<'_>, 
        buf: &mut [u8]
    ) -> Poll<io::Result<usize>>
    {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl AsyncWrite for C {
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

    fn poll_shutdown(
        mut self: Pin<&mut Self>, 
        cx: &mut Context<'_>
    ) -> Poll<io::Result<()>>
    {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

#[async_trait]
impl AsyncClient for C {
    type Addr = &'static str;

    async fn connect_server_async(addr: Self::Addr) -> io::Result<Self> {
        TcpStream::connect(addr)
            .await
            .map(|c| C(c.compat()))
    }
}

#[async_trait]
impl AsyncServer for S {
    type Client = C;

    async fn listen_clients_async(addr: <<Self as AsyncServer>::Client as AsyncClient>::Addr) -> io::Result<Self> {
        TcpListener::bind(addr)
            .await
            .map(|s| S(s.compat()))
    }

    async fn accept_client_async(&self) -> io::Result<Self::Client> {
        self.get_ref()
            .accept()
            .await
            .map(|(c, _)| C(c.compat()))
    }
}
