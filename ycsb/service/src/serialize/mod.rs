use std::default::Default;
use std::io::{Read, Write};

use febft::bft::error::*;
use febft::bft::crypto::hash::Digest;
use febft::bft::communication::message::{
    ReplyMessage,
    SystemMessage,
    RequestMessage,
    ConsensusMessage,
    ConsensusMessageKind,
};
use febft::bft::communication::serialize::{
    SharedData,
    ReplicaData,
};
use febft::bft::collections::{
    self,
    HashMap,
};

use crate::data::{Update, Request};

pub struct YcsbData;

impl ReplicaData for YcsbData {
    type State = HashMap<String, HashMap<String, HashMap<String, Vec<u8>>>>;

    fn serialize_state<W>(mut w: W, _s: &Self::State) -> Result<()>
    where
        W: Write
    {
        w.write_all(b"OK")
            .wrapped(ErrorKind::Communication)
    }

    fn deserialize_state<R>(_r: R) -> Result<Self::State>
    where
        R: Read
    {
        unimplemented!()
    }
}

impl SharedData for YcsbData {
    type Request = Update;
    type Reply = u32;

    fn serialize_message<W>(w: W, m: &SystemMessage<Update, u32>) -> Result<()>
    where
        W: Write
    {
        let mut root = capnp::message::Builder::new(capnp::message::HeapAllocator::new());
        let sys_msg: messages_capnp::system::Builder = root.init_root();
        match m {
            SystemMessage::Request(m) => {
                let update = sys_msg.init_request();
                let mut requests = update.init_requests(m.operation().requests.len() as u32);
                for i in 0..m.operation().requests.len() {
                    let r = &m.operation().requests[i];
                    let mut request = requests.reborrow().get(i as u32);
                    request.set_table(&r.table);
                    request.set_key(&r.key);
                    let mut values = request.init_values(r.values.len() as u32);
                    let mut i = 0;
                    for (k, v) in r.values.iter() {
                        let mut value = values.reborrow().get(i);
                        value.set_key(k);
                        value.set_value(v);
                        i += 1;
                    }
                }
            },
            SystemMessage::Reply(m) => {
                let mut reply = sys_msg.init_reply();
                reply.set_status(*m.payload());
                reply.set_digest(m.digest().as_ref());
            },
            SystemMessage::Consensus(m) => {
                let mut consensus = sys_msg.init_consensus();
                consensus.set_seq_no(m.sequence_number().into());
                match m.kind() {
                    ConsensusMessageKind::PrePrepare(digest) => consensus.set_pre_prepare(digest.as_ref()),
                    ConsensusMessageKind::Prepare => consensus.set_prepare(()),
                    ConsensusMessageKind::Commit => consensus.set_commit(()),
                }
            },
        }
        capnp::serialize::write_message(w, &root)
            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to serialize using capnp")
    }

    fn deserialize_message<R>(r: R) -> Result<SystemMessage<Update, u32>>
    where
        R: Read
    {
        let reader = capnp::serialize::read_message(r, Default::default())
            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get capnp reader")?;
        let sys_msg: messages_capnp::system::Reader = reader
            .get_root()
            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get system message root")?;
        let sys_msg_which = sys_msg
            .which()
            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get system message kind")?;

        match sys_msg_which {
            messages_capnp::system::Which::Reply(Ok(reply)) => {
                let status = reply.get_status();
                let digest_reader = reply
                    .get_digest()
                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get digest")?;
                let digest = Digest::from_bytes(digest_reader)
                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Invalid digest")?;

                Ok(SystemMessage::Reply(ReplyMessage::new(digest, status)))
            },
            messages_capnp::system::Which::Reply(_) => {
                Err("Failed to read reply message")
                    .wrapped(ErrorKind::CommunicationSerialize)
            },
            messages_capnp::system::Which::Request(Ok(updates)) => {
                let updates = updates
                    .get_requests()
                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get requests")?;
                let mut decoded_update = Update { requests: Vec::new() };

                for request in updates.iter() {
                    let values_reader = request
                        .get_values()
                        .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get request values")?;

                    let table = request
                        .get_table()
                        .map(String::from)
                        .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get request table")?;
                    let key = request
                        .get_key()
                        .map(String::from)
                        .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get request key")?;
                    let mut values = collections::hash_map();

                    for value in values_reader.iter() {
                        let key = value
                            .get_key()
                            .map(String::from)
                            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get request key")?;
                        let value = value
                            .get_value()
                            .map(Vec::from)
                            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get request value")?;

                        values.insert(key, value);
                    }

                    decoded_update.requests.push(Request { table, key, values });
                }

                Ok(SystemMessage::Request(RequestMessage::new(decoded_update)))
            },
            messages_capnp::system::Which::Request(_) => {
                Err("Failed to read request message")
                    .wrapped(ErrorKind::CommunicationSerialize)
            },
            messages_capnp::system::Which::Consensus(Ok(consensus)) => {
                let seq = consensus
                    .reborrow()
                    .get_seq_no();
                let message_kind = consensus
                    .which()
                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get consensus message kind")?;

                let kind = match message_kind {
                    messages_capnp::consensus::Which::PrePrepare(Ok(digest_reader)) => {
                        let digest = Digest::from_bytes(digest_reader)
                            .wrapped_msg(ErrorKind::CommunicationSerialize, "Invalid digest")?;
                        ConsensusMessageKind::PrePrepare(digest)
                    },
                    messages_capnp::consensus::Which::PrePrepare(_) => {
                        return Err("Failed to read consensus message kind")
                            .wrapped(ErrorKind::CommunicationSerialize);
                    },
                    messages_capnp::consensus::Which::Prepare(_) => ConsensusMessageKind::Prepare,
                    messages_capnp::consensus::Which::Commit(_) => ConsensusMessageKind::Commit,
                };

                Ok(SystemMessage::Consensus(ConsensusMessage::new(seq.into(), kind)))
            },
            messages_capnp::system::Which::Consensus(_) => {
                Err("Failed to read consensus message")
                    .wrapped(ErrorKind::CommunicationSerialize)
            },
        }
    }
}

mod messages_capnp {
    #![allow(unused)]
    include!(concat!(env!("OUT_DIR"), "/src/serialize/messages_capnp.rs"));
}
