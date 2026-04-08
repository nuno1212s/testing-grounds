use crate::nodes;

use std::io;
use std::net::{TcpListener, TcpStream};

pub struct C(TcpStream);
pub struct S(TcpListener);

impl nodes::Client for C {
    type Addr = &'static str;

    fn connect_server(addr: Self::Addr) -> io::Result<Self> {
        TcpStream::connect(addr).map(C)
    }
}

impl io::Read for C {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::Write for C {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl nodes::Server for S {
    type Client = C;

    fn listen_clients(addr: <<Self as nodes::Server>::Client as nodes::Client>::Addr) -> io::Result<Self> {
        TcpListener::bind(addr).map(S)
    }

    fn accept_client(&self) -> io::Result<Self::Client> {
        self.0.accept().map(|(client, _)| C(client))
    }
}
