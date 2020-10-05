use std::time::Duration;
use std::net::UdpSocket;
use std::io::{self, Read, Write};

use super::params;
use super::handlers::{
    handle_read_sync,
    handle_write_sync,
};

struct Sock(UdpSocket);

pub fn server() -> Result<(), Box<dyn std::error::Error>> {
    let mut listener = Sock(UdpSocket::bind(params::ADDR)?);
    listener.0.set_read_timeout(Some(Duration::from_secs(3)))?;
    for i in 0..params::CONNS {
        handle_read_sync(&mut listener)
            .map_err(|_| format!("Only {}/{} messages received.", i, params::CONNS))?;
    }
    Ok(())
}

pub fn client() -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..params::CONNS {
        let stream = Sock(UdpSocket::bind(params::ADDR2)?);
        stream.0.connect(params::ADDR)?;
        handle_write_sync(stream)?;
    }
    Ok(())
}

impl Read for Sock {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.recv(buf)
    }
}

impl Write for Sock {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.send(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
