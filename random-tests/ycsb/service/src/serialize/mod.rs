use std::default::Default;
use std::io::{Read, Write};

use febft::bft::error::*;
use febft::bft::crypto::hash::Digest;
use febft::bft::communication::serialize::SharedData;
use febft::bft::communication::message::{
    Header,
    ReplyMessage,
    StoredMessage,
    SystemMessage,
    RequestMessage,
    ConsensusMessage,
    ConsensusMessageKind,
};
use febft::bft::collections::{
    self,
    HashMap,
};
use febft::bft::ordering::{
    SeqNo,
    Orderable,
};

use crate::data::Update;

pub struct YcsbData;

pub type YcsbDataState = HashMap<String, HashMap<String, HashMap<String, Vec<u8>>>>;

impl SharedData for YcsbData {
    type State = YcsbDataState;
    type Request = Update;
    type Reply = u32;

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

    fn serialize_message<W>(w: W, m: &SystemMessage<YcsbDataState, Update, u32>) -> Result<()>
    where
        W: Write
    {
        let mut root = capnp::message::Builder::new(capnp::message::HeapAllocator::new());
        let sys_msg: messages_capnp::system::Builder = root.init_root();
        match m {
            SystemMessage::Request(m) => {
                let mut request = sys_msg.init_request();

                request.set_operation_id(m.sequence_number().into());
                request.set_session_id(m.session_id().into());

                let u = m.operation();
                let mut update = request.init_update();

                update.set_table(&u.table);
                update.set_key(&u.key);

                let mut values = update.init_values(u.values.len() as u32);

                for (i, (k, v)) in u.values.iter().enumerate() {
                    let mut value = values.reborrow().get(i as u32);
                    value.set_key(k);
                    value.set_value(v);
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
                consensus.set_view(m.view().into());
                match m.kind() {
                    ConsensusMessageKind::PrePrepare(requests) => {
                        let mut header = [0; Header::LENGTH];
                        let mut pre_prepare_requests = consensus.init_pre_prepare(requests.len() as u32);

                        for (i, stored) in requests.iter().enumerate() {
                            let mut forwarded = pre_prepare_requests.reborrow().get(i as u32);

                            // set header
                            {
                                stored.header().serialize_into(&mut header[..]).unwrap();
                                forwarded.set_header(&header[..]);
                            }

                            // set request
                            {
                                let mut request = forwarded.init_request();

                                request.set_operation_id(stored.message().sequence_number().into());
                                request.set_session_id(stored.message().session_id().into());

                                let u = stored.message().operation();
                                let mut update = request.init_update();

                                update.set_table(&u.table);
                                update.set_key(&u.key);

                                let mut values = update.init_values(u.values.len() as u32);

                                for (i, (k, v)) in u.values.iter().enumerate() {
                                    let mut value = values.reborrow().get(i as u32);
                                    value.set_key(k);
                                    value.set_value(v);
                                }
                            }
                        }
                    },
                    ConsensusMessageKind::Prepare(digest) => consensus.set_prepare(digest.as_ref()),
                    ConsensusMessageKind::Commit(digest) => consensus.set_commit(digest.as_ref()),
                }
            },
            _ => return Err("Unsupported system message").wrapped(ErrorKind::CommunicationSerialize),
        }
        capnp::serialize::write_message(w, &root)
            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to serialize using capnp")
    }

    fn deserialize_message<R>(r: R) -> Result<SystemMessage<YcsbDataState, Update, u32>>
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
            messages_capnp::system::Which::Request(Ok(request)) => {
                let session_id: SeqNo = request.get_session_id().into();
                let operation_id: SeqNo = request.get_operation_id().into();
                let update = request
                    .get_update()
                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get update")?;

                let values_reader = update
                    .get_values()
                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get update values")?;

                let table = update
                    .get_table()
                    .map(String::from)
                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get update table")?;
                let key = update
                    .get_key()
                    .map(String::from)
                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get update key")?;
                let mut values = collections::hash_map();

                for value in values_reader.iter() {
                    let key = value
                        .get_key()
                        .map(String::from)
                        .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get values key")?;
                    let value = value
                        .get_value()
                        .map(Vec::from)
                        .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get values value")?;

                    values.insert(key, value);
                }

                let decoded_update = Update {
                    values,
                    table,
                    key,
                };

                Ok(SystemMessage::Request(RequestMessage::new(session_id, operation_id, decoded_update)))
            },
            messages_capnp::system::Which::Request(_) => {
                Err("Failed to read request message")
                    .wrapped(ErrorKind::CommunicationSerialize)
            },
            messages_capnp::system::Which::Consensus(Ok(consensus)) => {
                let seq: SeqNo = consensus
                    .reborrow()
                    .get_seq_no()
                    .into();
                let view: SeqNo = consensus
                    .reborrow()
                    .get_view()
                    .into();
                let message_kind = consensus
                    .which()
                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get consensus message kind")?;

                let kind = match message_kind {
                    messages_capnp::consensus::Which::PrePrepare(Ok(requests_reader)) => {
                        let mut requests = Vec::new();

                        for forwarded in requests_reader.iter() {
                            let header = {
                                let raw_header = forwarded
                                    .get_header()
                                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get request header")?;

                                Header::deserialize_from(raw_header).unwrap()
                            };
                            let message = {
                                let request = forwarded
                                    .get_request()
                                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get request message")?;

                                let session_id: SeqNo = request.get_session_id().into();
                                let operation_id: SeqNo = request.get_operation_id().into();

                                let update = request
                                    .get_update()
                                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get update")?;

                                let values_reader = update
                                    .get_values()
                                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get update values")?;

                                let table = update
                                    .get_table()
                                    .map(String::from)
                                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get update table")?;
                                let key = update
                                    .get_key()
                                    .map(String::from)
                                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get update key")?;
                                let mut values = collections::hash_map();

                                for value in values_reader.iter() {
                                    let key = value
                                        .get_key()
                                        .map(String::from)
                                        .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get values key")?;
                                    let value = value
                                        .get_value()
                                        .map(Vec::from)
                                        .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get values value")?;

                                    values.insert(key, value);
                                }

                                let decoded_update = Update {
                                    values,
                                    table,
                                    key,
                                };

                                RequestMessage::new(session_id, operation_id, decoded_update)
                            };

                            requests.push(StoredMessage::new(header, message));
                        }

                        ConsensusMessageKind::PrePrepare(requests)
                    },
                    messages_capnp::consensus::Which::Prepare(Ok(digest)) => {
                        let digest = Digest::from_bytes(digest)
                            .wrapped_msg(ErrorKind::CommunicationSerialize, "Invalid digest")?;
                        ConsensusMessageKind::Prepare(digest)
                    },
                    messages_capnp::consensus::Which::Commit(Ok(digest)) => {
                        let digest = Digest::from_bytes(digest)
                            .wrapped_msg(ErrorKind::CommunicationSerialize, "Invalid digest")?;
                        ConsensusMessageKind::Commit(digest)
                    },
                    _ => return Err("Failed to read consensus message kind").wrapped(ErrorKind::CommunicationSerialize),
                };

                Ok(SystemMessage::Consensus(ConsensusMessage::new(seq, view, kind)))
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
