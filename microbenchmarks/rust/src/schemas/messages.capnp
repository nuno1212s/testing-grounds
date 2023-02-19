@0xebe43d92ab9b3048;

using Rust = import "rust.capnp";
$Rust.parentModule("serialize");

struct BenchmarkRequest {
    data @0 :Data;
}

struct BenchmarkReply {
    data @0 :Data;
}