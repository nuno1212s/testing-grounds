use std::sync::Weak;
use std::time::Duration;
use std::default::Default;
use std::io::{Read, Write};

use konst::{
    primitive::{
        parse_usize,
        parse_bool,
        parse_u64,
    },
    option::unwrap_or,
    unwrap_ctx,
};
use konst::primitive::parse_u128;

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
use febft::bft::ordering::{
    SeqNo,
    Orderable,
};

pub struct MicrobenchmarkData;

impl MicrobenchmarkData {
    pub const REQUEST_SIZE: usize = {
        let result = parse_usize(env!("REQUEST_SIZE"));
        unwrap_ctx!(result)
    };

    pub const REPLY_SIZE: usize = {
        let result = parse_usize(env!("REPLY_SIZE"));
        unwrap_ctx!(result)
    };

    pub const STATE_SIZE: usize = {
        let result = parse_usize(env!("STATE_SIZE"));
        unwrap_ctx!(result)
    };

    pub const MEASUREMENT_INTERVAL: usize = {
        let result = parse_usize(env!("MEASUREMENT_INTERVAL"));
        unwrap_ctx!(result)
    };

    pub const OPS_NUMBER: usize = {
        let result = parse_usize(env!("OPS_NUMBER"));
        unwrap_ctx!(result)
    };

    pub const REQUEST_SLEEP_MILLIS: Duration = {
        let result = parse_u64(unwrap_or!(option_env!("REQUEST_SLEEP_MILLIS"), "0"));
        Duration::from_millis(unwrap_ctx!(result))
    };

    pub const VERBOSE: bool = {
        let result = parse_bool(unwrap_or!(option_env!("VERBOSE"), "false"));
        unwrap_ctx!(result)
    };

    const REQUEST: [u8; Self::REQUEST_SIZE] = [0; Self::REQUEST_SIZE];
}

impl SharedData for MicrobenchmarkData {
    type State = Vec<u8>;
    type Request = Weak<Vec<u8>>;
    type Reply = Weak<Vec<u8>>;

    fn serialize_state<W>(_w: W, _s: &Self::State) -> Result<()>
        where
            W: Write
    {
        Ok(())
    }

    fn deserialize_state<R>(_r: R) -> Result<Vec<u8>>
        where
            R: Read
    {
        Ok((0..)
            .into_iter()
            .take(MicrobenchmarkData::STATE_SIZE)
            .map(|x| (x & 0xff) as u8)
            .collect())
    }

    fn serialize_message<W>(w: W, m: &SystemMessage<Vec<u8>, Weak<Vec<u8>>, Weak<Vec<u8>>>) -> Result<()>
        where
            W: Write
    {
        let mut root = capnp::message::Builder::new(capnp::message::HeapAllocator::new());
        let sys_msg: messages_capnp::system::Builder = root.init_root();
        match m {
            SystemMessage::Request(m) => {
                let mut request = sys_msg.init_request();
                let operation = match m.operation().upgrade() {
                    Some(p) => p,
                    _ => return Err("No operation available").wrapped(ErrorKind::CommunicationSerialize),
                };

                request.set_operation_id(m.sequence_number().into());
                request.set_session_id(m.session_id().into());
                request.set_data(&*operation);
            }
            SystemMessage::Reply(m) => {
                let mut reply = sys_msg.init_reply();
                let payload = match m.payload().upgrade() {
                    Some(p) => p,
                    _ => return Err("No payload available").wrapped(ErrorKind::CommunicationSerialize),
                };
                reply.set_operation_id(m.sequence_number().into());
                reply.set_session_id(m.session_id().into());
                reply.set_data(&*payload);
            }
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
                                request.set_data(&Self::REQUEST);
                            }
                        }
                    }
                    ConsensusMessageKind::Prepare(digest) => consensus.set_prepare(digest.as_ref()),
                    ConsensusMessageKind::Commit(digest) => consensus.set_commit(digest.as_ref()),
                }
            }
            _ => return Err("Unsupported system message").wrapped(ErrorKind::CommunicationSerialize),
        }
        capnp::serialize::write_message(w, &root)
            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to serialize using capnp")
    }

    fn deserialize_message<R>(r: R) -> Result<SystemMessage<Vec<u8>, Weak<Vec<u8>>, Weak<Vec<u8>>>>
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
                let session_id: SeqNo = reply.get_session_id().into();
                let operation_id: SeqNo = reply.get_operation_id().into();
                let _data = reply
                    .get_data()
                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get data")?
                    .to_owned();

                Ok(SystemMessage::Reply(ReplyMessage::new(session_id, operation_id, Weak::new())))
            }
            messages_capnp::system::Which::Reply(_) => {
                Err("Failed to read reply message")
                    .wrapped(ErrorKind::CommunicationSerialize)
            }
            messages_capnp::system::Which::Request(Ok(request)) => {
                let session_id: SeqNo = request.get_session_id().into();
                let operation_id: SeqNo = request.get_operation_id().into();
                let _data = request
                    .get_data()
                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get data")?;

                Ok(SystemMessage::Request(RequestMessage::new(session_id, operation_id, Weak::new())))
            }
            messages_capnp::system::Which::Request(_) => {
                Err("Failed to read request message")
                    .wrapped(ErrorKind::CommunicationSerialize)
            }
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

                                let _data = request
                                    .get_data()
                                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get data")?;

                                RequestMessage::new(session_id, operation_id, Weak::new())
                            };

                            requests.push(StoredMessage::new(header, message));
                        }

                        ConsensusMessageKind::PrePrepare(requests)
                    }
                    messages_capnp::consensus::Which::Prepare(Ok(digest)) => {
                        let digest = Digest::from_bytes(digest)
                            .wrapped_msg(ErrorKind::CommunicationSerialize, "Invalid digest")?;
                        ConsensusMessageKind::Prepare(digest)
                    }
                    messages_capnp::consensus::Which::Commit(Ok(digest)) => {
                        let digest = Digest::from_bytes(digest)
                            .wrapped_msg(ErrorKind::CommunicationSerialize, "Invalid digest")?;
                        ConsensusMessageKind::Commit(digest)
                    }
                    _ => return Err("Failed to read consensus message kind").wrapped(ErrorKind::CommunicationSerialize),
                };

                Ok(SystemMessage::Consensus(ConsensusMessage::new(seq, view, kind)))
            }
            messages_capnp::system::Which::Consensus(_) => {
                Err("Failed to read consensus message")
                    .wrapped(ErrorKind::CommunicationSerialize)
            }
        }
    }
}

mod messages_capnp {
    #![allow(unused)]
    include!(concat!(env!("OUT_DIR"), "/src/serialize/messages_capnp.rs"));
}
