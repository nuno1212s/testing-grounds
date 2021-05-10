@0x94a43df6c359e805;

using Java = import "/capnp/java.capnp";
$Java.package("febft.ycsb.capnp");
$Java.outerClassname("Messages");

struct System {
    union {
        request   @0 :Update;
        reply     @1 :Reply;
        consensus @2 :Consensus;
    }
}

struct Update {
    requests @0 :List(Request);
}

struct Request {
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
    union {
        prePrepare @1 :Data;
        prepare    @2 :Void;
        commit     @3 :Void;
    }
}
