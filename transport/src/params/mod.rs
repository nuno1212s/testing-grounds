use std::time::Duration;

pub const SECS: u64 = 5;
pub const TIME: Duration = Duration::from_secs(SECS);

pub const BUFSIZ: usize = 4096;
pub const LADDR: &str = "0.0.0.0:50001";
pub const N1: &str = "192.168.70.16:50001";
pub const N2: &str = "192.168.70.17:50001";
pub const N3: &str = "192.168.70.18:50001";
pub const N4: &str = "192.168.70.19:50001";
pub const ADDRS: [&str; 4] = [N1, N2, N3, N4];
