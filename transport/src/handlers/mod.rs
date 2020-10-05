use std::sync::atomic::AtomicI32;
use std::io::{self, Read, Write};
use std::time::{Instant, Duration};

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

fn run_for_one_second_sync<F>(f: F) -> i32
where
    F: FnMut(),
{
    asd
}
