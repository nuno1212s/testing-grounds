use std::io::{self, Read, Write};
use futures::io::{AsyncRead, AsyncWrite};

pub trait Client: Read + Write {
    type Addr;

    fn connect_server(addr: Self::Addr) -> io::Result<Self>;
}

pub trait Server: Read + Write {
    type Addr;
    type Client: Client;

    fn listen_clients(addr: Self::Addr) -> io::Result<Self>
    where
        Self::Addr == Self::Client::Addr;

    fn accept_client(&self) -> io::Result<Self::Client>;
}
