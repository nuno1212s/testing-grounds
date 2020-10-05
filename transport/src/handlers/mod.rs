use std::io::{self, Read, Write};

const BUFSIZ: usize = 4096;

pub fn handle_read_sync<R: Read>(mut r: R) -> io::Result<()> {
    let mut buf = [0_u8; BUFSIZ];
    r.read(&mut buf[..])?;
    Ok(())
}

pub fn handle_write_sync<W: Write>(mut w: W) -> io::Result<()> {
    let mut buf = [0_u8; BUFSIZ];
    w.write(&mut buf[..])?;
    Ok(())
}
