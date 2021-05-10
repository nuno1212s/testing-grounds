use std::io::{Read, Write};

use febft::bft::error::*;
use febft::bft::communication::message::{
    SystemMessage,
    ConsensusMessageKind,
};
use febft::bft::communication::serialize::{
    SharedData,
    ReplicaData,
};

use crate::data::Update;

pub struct YcsbData;

impl SharedData for YcsbData {
    type Request = Update;
    type Reply = u32;

    fn serialize_message<W>(w: W, m: &SystemMessage<Update, u32>) -> Result<()>
    where
        W: Write
    {
        let mut root = capnp::message::Builder::new(HeapAllocator::new());
        let mut sys_msg: messages_capnp::system::Builder = root.init_root();
        match m {
            SystemMessage::Request(_) => {

            },
            SystemMessage::Reply(m) => {
                let mut reply = sys_msg.init_reply();
                reply.set_status(m.payload());
                reply.set_digest(m.digest());
            },
            SystemMessage::Consensus(m) => {
                let mut consensus = sys_msg.init_consensus();
                consensus.set_sequence_number(m.sequence_number());

                let mut message_kind = consensus.init_message_kind();
                match m.kind() {
                    ConsensusMessageKind::PrePrepare(digest) => {
                        let d = digest.as_ref();
                        let pre_prepare = message_kind.init_pre_prepare(d.len() as u32);
                        pre_prepare.copy_from_slice(d);
                    },
                    ConsensusMessageKind::Prepare => message_kind.set_prepare(()),
                    ConsensusMessageKind::Commit => message_kind.set_commit(()),
                }
            },
        }
    }

    fn deserialize_message<R>(r: R) -> Result<SystemMessage<Update, u32>>
    where
        R: Read
    {
        unimplemented!()
    }
}

mod messages_capnp {
    #![allow(unused)]
    include!(concat!(env!("OUT_DIR"), "/src/serialize/messages_capnp.rs"));
}
