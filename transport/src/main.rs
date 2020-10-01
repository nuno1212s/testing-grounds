mod tcp_sync;

use bencher::{benchmark_group, benchmark_main};

benchmark_group!(benches,
    tcp_sync::bench_write);

benchmark_main!(benches);
