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
    update      @2 :Update;
}

struct Update {
    table  @0 :Text;
    key    @1 :Text;
    values @2 :List(Value);
}

struct Value {
    key   @0 :Text;
    value @1 :Data;
}

struct Reply {
    status @0 :UInt32;
    digest @1 :Data;
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
