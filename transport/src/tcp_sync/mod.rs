use super::nodes;

use std::net::{TcpListener, TcpStream};

pub struct Client(TcpStream);
pub struct Server(TcpListener);

impl nodes::Client for Client {
    type Addr = &str;

    fn connect_server(addr: Self::Addr) -> io::Result<Self> {
        TcpStream::connect(addr).map(Client)
    }
}

impl nodes::Server for Server {
    type Addr = &str;
    type Client = Client;

    fn listen_clients(addr: Self::Addr) -> io::Result<Self>
    where
        Self::Addr == Self::Client::Addr
    {
        TcpListener::bind(addr).map(Server)
    }

    fn accept_client(&self) -> io::Result<Self::Client> {
        self.0.accept().map(|(client, _)| Client(client))
    }
}
