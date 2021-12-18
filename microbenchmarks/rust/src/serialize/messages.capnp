@0x94a43df6c359e805;

using Rust = import "rust.capnp";
$Rust.parentModule("serialize");

struct System {
    union {
        request   @0 :Request;
        reply     @1 :Reply;
        consensus @2 :Consensus;
    }
}

struct Request {
    sessionId   @0 :UInt32;
    operationId @1 :UInt32;
    data        @2 :Data;
}

struct Reply {
    sessionId   @0 :UInt32;
    operationId @1 :UInt32;
    data        @2 :Data;
}

struct Consensus {
    seqNo @0 :UInt32;
    view  @1 :UInt32;
    union {
        prePrepare @2 :List(ForwardedRequest);
        prepare    @3 :Data;
        commit     @4 :Data;
    }
}

struct ForwardedRequest {
    header  @0 :Data;
    request @1 :Request;
}
