use std::io::{self, Read, Write};

use async_trait::async_trait;
use futures::io::{AsyncRead, AsyncWrite};

pub trait Client: Read + Write + Sized {
    type Addr;

    fn connect_server(addr: Self::Addr) -> io::Result<Self>;
}

pub trait Server: Sized {
    type Client: Client;

    fn listen_clients(addr: <<Self as Server>::Client as Client>::Addr) -> io::Result<Self>;
    fn accept_client(&self) -> io::Result<Self::Client>;
}

#[async_trait]
pub trait AsyncClient: AsyncRead + AsyncWrite + Sized {
    type Addr;

    async fn connect_server_async(addr: Self::Addr) -> io::Result<Self>;
}

#[async_trait]
pub trait AsyncServer: Sized {
    type Client: AsyncClient;

    async fn listen_clients_async(addr: <<Self as AsyncServer>::Client as AsyncClient>::Addr) -> io::Result<Self>;
    async fn accept_client_async(&self) -> io::Result<Self::Client>;
}
