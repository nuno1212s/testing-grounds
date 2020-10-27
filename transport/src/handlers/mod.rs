use super::params;
use super::nodes::{Client, Server};
use std::io::{self, Read, Write};
use futures::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};

pub fn sync_server_test_2<S: Server>(server: S) -> io::Result<()> {
    asd
}

fn handle_read_sync<R: Read>(mut r: R) -> io::Result<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    r.read(&mut buf[..])?;
    Ok(())
}

fn handle_write_sync<W: Write>(mut w: W) -> io::Result<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    w.write(&mut buf[..])?;
    Ok(())
}

async fn handle_read_async<R: AsyncRead + Unpin>(mut r: R) -> io::Result<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    r.read(&mut buf[..]).await?;
    Ok(())
}

async fn handle_write_async<W: AsyncWrite + Unpin>(mut w: W) -> io::Result<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    w.write(&mut buf[..]).await?;
    Ok(())
}
