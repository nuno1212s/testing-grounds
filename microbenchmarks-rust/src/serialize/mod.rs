use std::sync::Arc;
use std::default::Default;
use std::io::{Read, Write};

use konst::{
    primitive::parse_usize,
    unwrap_ctx,
};

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
}

impl SharedData for MicrobenchmarkData {
    type State = Vec<u8>;
    type Request = Vec<u8>;
    type Reply = Arc<Vec<u8>>;

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

    fn serialize_message<W>(w: W, m: &SystemMessage<Vec<u8>, Vec<u8>, Arc<Vec<u8>>>) -> Result<()>
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
                request.set_data(m.operation());
            },
            SystemMessage::Reply(m) => {
                let mut reply = sys_msg.init_reply();
                reply.set_digest(m.digest().as_ref());
                reply.set_data(m.payload());
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
                                request.set_data(stored.message().operation());
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

    fn deserialize_message<R>(r: R) -> Result<SystemMessage<Vec<u8>, Vec<u8>, Arc<Vec<u8>>>>
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
                let digest_reader = reply
                    .get_digest()
                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get digest")?;
                let digest = Digest::from_bytes(digest_reader)
                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Invalid digest")?;
                let data = reply
                    .get_data()
                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get data")?
                    .to_owned();

                Ok(SystemMessage::Reply(ReplyMessage::new(digest, Arc::new(data))))
            },
            messages_capnp::system::Which::Reply(_) => {
                Err("Failed to read reply message")
                    .wrapped(ErrorKind::CommunicationSerialize)
            },
            messages_capnp::system::Which::Request(Ok(request)) => {
                let session_id: SeqNo = request.get_session_id().into();
                let operation_id: SeqNo = request.get_operation_id().into();
                let data = request
                    .get_data()
                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get data")?;

                Ok(SystemMessage::Request(RequestMessage::new(session_id, operation_id, data.to_owned())))
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

                                let data = request
                                    .get_data()
                                    .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get data")?;

                                RequestMessage::new(session_id, operation_id, data.to_owned())
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
