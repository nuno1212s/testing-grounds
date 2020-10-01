use std::io::{self, Read, Write};

pub fn handle_read_sync<R: Read>(mut r: R) -> io::Result<()> {
    let mut buf = [0_u8; 8];
    r.read(&mut buf[..])?;
    Ok(())
}

pub fn handle_write_sync<W: Write>(mut w: W) -> io::Result<()> {
    let mut buf = [0_u8; 8];
    w.write(&mut buf[..])?;
    Ok(())
}
