use super::params;
use std::io::{self, Read, Write};
use futures::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};

pub fn handle_read_sync<R: Read>(mut r: R) -> io::Result<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    r.read(&mut buf[..])?;
    Ok(())
}

pub fn handle_write_sync<W: Write>(mut w: W) -> io::Result<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    w.write(&mut buf[..])?;
    Ok(())
}

pub async fn handle_read_async<R: AsyncRead + Unpin>(mut r: R) -> io::Result<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    r.read(&mut buf[..]).await?;
    Ok(())
}

pub async fn handle_write_async<W: AsyncWrite + Unpin>(mut w: W) -> io::Result<()> {
    let mut buf = [0_u8; params::BUFSIZ];
    w.write(&mut buf[..]).await?;
    Ok(())
}
