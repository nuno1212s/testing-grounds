use super::handlers::{
    handle_read_sync,
    handle_write_sync,
};

use std::net::{TcpListener, TcpStream};

const CONNS: usize = 100;
const ADDR: &str = "127.0.0.1:12345";

pub fn server() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(ADDR)?;
    for stream in listener.incoming().take(CONNS) {
        handle_read_sync(stream?)?;
    }
    Ok(())
}

pub fn client() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
