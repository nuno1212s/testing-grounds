@0x94a43df6c359e805;

using Rust = import "rust.capnp";
$Rust.parentModule("serialize");
struct BenchmarkRequest {
    data @0 :Data;
}

struct BenchmarkReply {
    data @0 :Data;
}