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

use febft::bft::error::*;
use febft::bft::crypto::hash::Digest;
use febft::bft::communication::serialize::SharedData;
use febft::bft::communication::message::{Header, ReplyMessage, StoredMessage, SystemMessage, RequestMessage, ConsensusMessage, ConsensusMessageKind, ObserverMessage, ObserveEventKind};
use febft::bft::communication::message::ObserverMessage::ObservedValue;
use febft::bft::communication::NodeId;
use febft::bft::core::server::ViewInfo;
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

    pub const CONCURRENT_RQS: usize = {
        let result = parse_usize(env!("CONCURRENT_RQS"));
        unwrap_ctx!(result)
    };

    pub const CLIENT_SLEEP_INITIAL: u64 = {
        let result = parse_u64(unwrap_or!(option_env!("CLIENT_SLEEP"), "1000"));

        unwrap_ctx!(result)
    };

    const REQUEST: [u8; Self::REQUEST_SIZE] = [0; Self::REQUEST_SIZE];
}

impl MicrobenchmarkData {

    fn deserialize_consensus_msg(messages_capnp::consensus::)

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
            SystemMessage::ObserverMessage(observer_message) => {
                let mut capnp_observer = sys_msg.init_observer_message();

                let mut obs_message_type = capnp_observer.init_message_type();

                match observer_message {
                    ObserverMessage::ObserverRegister => {
                        obs_message_type.set_observer_register(());
                    }
                    ObserverMessage::ObserverRegisterResponse(response) => {
                        obs_message_type.set_observer_register_response((*response));
                    }
                    ObserverMessage::ObserverUnregister => {
                        obs_message_type.set_observer_unregister(());
                    }
                    ObserverMessage::ObservedValue(observed_value) => {
                        let mut observer_value_msg = obs_message_type.init_observed_value();

                        let mut value = observer_value_msg.init_value();

                        match observed_value {
                            ObserveEventKind::CheckpointStart(start) => {
                                value.set_checkpoint_start((*start).into());
                            }
                            ObserveEventKind::CheckpointEnd(end) => {
                                value.set_checkpoint_end((*end).into());
                            }
                            ObserveEventKind::Consensus(seq_no) => {
                                value.set_consensus((*seq_no).into());
                            }
                            ObserveEventKind::NormalPhase((view, seq)) => {

                                let mut normal_phase = value.init_normal_phase();

                                normal_phase.set_seq_num((*seq).into());

                                let mut view_info = normal_phase.init_view();

                                view_info.set_view_num(view.sequence_number().into());
                                view_info.set_n(view.params().n() as u32);
                                view_info.set_f(view.params().f() as u32);
                            }
                            ObserveEventKind::ViewChangePhase => {
                                value.set_view_change(());
                            }
                            ObserveEventKind::CollabStateTransfer => {
                                value.set_collab_state_transfer(());
                            }
                            ObserveEventKind::Prepare(seq_no) => {
                                value.set_prepare((*seq_no).into());
                            }
                            ObserveEventKind::Commit(seq_no) => {
                                value.set_commit((*seq_no).into());
                            }
                            ObserveEventKind::Ready(seq) => {
                                value.set_ready((*seq).into());
                            }
                            ObserveEventKind::Executed(seq) => {
                                value.set_executed((*seq).into());
                            }
                        }
                    }
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
            messages_capnp::system::Which::ObserverMessage(Ok(observerMessage)) => {

                let message_kind = observerMessage.reborrow()
                    .get_message_type()
                    .which().wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get message type")?;

                match message_kind {

                    messages_capnp::observer_message::message_type::Which::ObserverRegister(()) => {
                        Ok(SystemMessage::ObserverMessage(ObserverMessage::ObserverRegister))
                    }
                    messages_capnp::observer_message::message_type::Which::ObserverRegisterResponse(value) => {
                        Ok(SystemMessage::ObserverMessage(ObserverMessage::ObserverRegisterResponse(value)))
                    }
                    messages_capnp::observer_message::message_type::Which::ObserverUnregister(()) => {
                        Ok(SystemMessage::ObserverMessage(ObserverMessage::ObserverUnregister))
                    }
                    messages_capnp::observer_message::message_type::Which::ObservedValue(Ok(value)) => {

                        let observed_kind = value.get_value().which().wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get message type")?;

                        let event = match observed_kind {
                            messages_capnp::observed_value::value::Which::CheckpointStart(value) => {
                                Ok(ObserveEventKind::CheckpointStart(value.into()))
                            }
                            messages_capnp::observed_value::value::Which::CheckpointEnd(value) => {
                                Ok(ObserveEventKind::CheckpointEnd(value.into()))
                            }
                            messages_capnp::observed_value::value::Which::Consensus(value) => {
                                Ok(ObserveEventKind::Consensus(value.into()))
                            }
                            messages_capnp::observed_value::value::Which::NormalPhase(Ok(phase)) => {
                                let seq_num: SeqNo = phase.get_seq_num().into();

                                let view = phase.get_view().unwrap();

                                let view_seq :SeqNo= view.get_view_num().into();
                                let n: usize = view.get_n() as usize;
                                let f: usize = view.get_f() as usize;

                                let view_info = ViewInfo::new(view_seq, n, f).unwrap();

                                Ok(ObserveEventKind::NormalPhase((view_info, seq_num)))
                            }
                            messages_capnp::observed_value::value::Which::NormalPhase(_) => {
                                Err("Failed to read normal phase values").wrapped(ErrorKind::CommunicationSerialize)
                            }
                            messages_capnp::observed_value::value::Which::ViewChange(()) => {
                                Ok(ObserveEventKind::ViewChangePhase)
                            }
                            messages_capnp::observed_value::value::Which::CollabStateTransfer(()) => {
                                Ok(ObserveEventKind::CollabStateTransfer)
                            }
                            messages_capnp::observed_value::value::Which::Prepare(seq_no) => {
                                Ok(ObserveEventKind::Prepare(seq_no.into()))
                            }
                            messages_capnp::observed_value::value::Which::Commit(seq_no) => {
                                Ok(ObserveEventKind::Commit(seq_no.into()))
                            }
                            messages_capnp::observed_value::value::Ready(seq) => {
                                Ok(ObserveEventKind::Ready(seq.into()))
                            }
                            messages_capnp::observed_value::value::Executed(seq) => {
                                Ok(ObserveEventKind::Executed(seq.into()))
                            }
                        }?;

                        Ok(SystemMessage::ObserverMessage(ObservedValue(event)))
                    }
                    messages_capnp::observer_message::message_type::Which::ObservedValue(_) => {
                        Err("Error reading observed value").wrapped(ErrorKind::CommunicationSerialize)
                    }
                }
            }
            messages_capnp::system::Which::ObserverMessage(_) => {
                Err("Failed to read observer message").wrapped(ErrorKind::CommunicationSerialize)
            }
        }
    }

    fn serialize_consensus_message<W>(w: W, m: &ConsensusMessage<Self::Request>) -> Result<()> where W: Write {
        todo!()
    }

    fn deserialize_consensus_message<R>(r: R) -> Result<ConsensusMessage<Self::Request>> where R: Read {
        let reader = capnp::serialize::read_message(r, Default::default())
            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get capnp reader")?;

        let consensus:  messages_capnp::consensus::Reader = reader.get_root()
            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get consensus msg")?;

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

        Ok(ConsensusMessage::new(seq, view, kind))
    }
}

mod messages_capnp {
    #![allow(unused)]
    include!(concat!(env!("OUT_DIR"), "/src/serialize/messages_capnp.rs"));
}
