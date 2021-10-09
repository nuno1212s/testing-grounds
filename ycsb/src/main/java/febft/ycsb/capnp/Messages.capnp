@0x94a43df6c359e805;

using Java = import "/capnp/java.capnp";
$Java.package("febft.ycsb.capnp");
$Java.outerClassname("Messages");

struct Message {
    header  @0 :Data;
    message @1 :System;
}

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
    union {
        prePrepare @1 :List(Message);
        prepare    @2 :Data;
        commit     @3 :Data;
    }
}
