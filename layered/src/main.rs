const SRC: &str = "0.0.0.0:5555";
const DST: &str = "127.0.0.1:5555";

fn main() {
    match std::env::args().nth(1).as_ref().map(String::as_ref) {
        Some("client") => ditto::bench::layered_bench(ditto::bench::Side::Client, DST),
        Some("server") => ditto::bench::layered_bench(ditto::bench::Side::Server, SRC),
        _ => panic!("Arg must be either client or server"),
    }
}
