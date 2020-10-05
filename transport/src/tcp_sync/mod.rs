use super::params;
use super::handlers::{
    handle_read_sync,
    handle_write_sync,
};

use std::net::{TcpListener, TcpStream};

pub fn server() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(params::ADDR)?;
    for stream in listener.incoming().take(params::CONNS) {
        handle_read_sync(stream?)?;
    }
    Ok(())
}

pub fn client() -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..params::CONNS {
        let stream = TcpStream::connect(params::ADDR)?;
        handle_write_sync(stream)?;
    }
    Ok(())
}
