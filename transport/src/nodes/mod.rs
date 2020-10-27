use std::io::{self, Read, Write};
//use futures::io::{AsyncRead, AsyncWrite}; TODO: async traits

pub trait Client: Read + Write
where
    Self: Sized,
{
    type Addr;

    fn connect_server(addr: Self::Addr) -> io::Result<Self>;
}

pub trait Server
where
    Self: Sized,
{
    type Client: Client;

    fn listen_clients(addr: <<Self as Server>::Client as Client>::Addr) -> io::Result<Self>;
    fn accept_client(&self) -> io::Result<Self::Client>;
}
