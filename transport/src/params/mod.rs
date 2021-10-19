use std::time::Duration;
use konst::{
    primitive::parse_usize,
    unwrap_ctx,
};

// 2 minutes per test case
pub const SECS: u64 = 2 * 60;
pub const TIME: Duration = Duration::from_secs(SECS);

pub const BUFSIZ: usize = {
    let result = parse_usize(env!("BUFSIZ"));
    unwrap_ctx!(result)
};
pub const LADDR: &str = "0.0.0.0:50003";
pub const N1: &str = "192.168.70.16:50003";
pub const N2: &str = "192.168.70.17:50003";
pub const N3: &str = "192.168.70.18:50003";
pub const N4: &str = "192.168.70.19:50003";
pub const ADDRS: [&str; 4] = [N2, N1, N3, N4];
