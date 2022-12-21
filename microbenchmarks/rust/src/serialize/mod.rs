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
use febft::bft::communication::serialize::SharedData;

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

    fn serialize_state<W>(_w: W, _state: &Self::State) -> Result<()> where W: Write {
        Ok(())
    }

    fn deserialize_state<R>(_r: R) -> Result<Self::State> where R: Read {
        Ok((0..).into_iter()
            .take(MicrobenchmarkData::STATE_SIZE)
            .map(|x| (x & 0xff) as u8)
            .collect())
    }

    fn serialize_request<W>(w: W, request: &Self::Request) -> Result<()> where W: Write {
        let mut root = capnp::message::Builder::new(capnp::message::HeapAllocator::new());

        let mut rq_msg: messages_capnp::benchmark_request::Builder = root.init_root();

        let request_content = request.upgrade();

        if let Some(request) = request_content {
            rq_msg.set_data(&*request);
        } else {
            panic!("Failed to get message to send");
        }

        capnp::serialize::write_message(w, &root)
            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to serialize request")
    }

    fn deserialize_request<R>(r: R) -> Result<Self::Request> where R: Read {

        let reader = capnp::serialize::read_message(r, Default::default()).wrapped_msg(ErrorKind::CommunicationSerialize,
        "Failed to read message")?;

        let request_msg : messages_capnp::benchmark_request::Reader = reader.get_root()
            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to read request message")?;

        let _data = request_msg.get_data().wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get data from request message?");

        Ok(Weak::new())
    }

    fn serialize_reply<W>(w: W, reply: &Self::Reply) -> Result<()> where W: Write {
        let mut root = capnp::message::Builder::new(capnp::message::HeapAllocator::new());

        let mut rq_msg: messages_capnp::benchmark_reply::Builder = root.init_root();

        let reply_content = reply.upgrade();

        if let Some(reply) = reply_content {
            rq_msg.set_data(&*reply);
        } else {
            panic!("Failed to get message to send");
        }

        capnp::serialize::write_message(w, &root)
            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to serialize reply")
    }

    fn deserialize_reply<R>(r: R) -> Result<Self::Reply> where R: Read {

        let reader = capnp::serialize::read_message(r, Default::default()).wrapped_msg(ErrorKind::CommunicationSerialize,
                                                                                       "Failed to read message")?;

        let request_msg : messages_capnp::benchmark_reply::Reader = reader.get_root()
            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to read reply message")?;

        let _data = request_msg.get_data().wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get data from reply message?");

        Ok(Weak::new())
    }
}

mod messages_capnp {
    #![allow(unused)]
    include!(concat!(env!("OUT_DIR"), "/src/serialize/messages_capnp.rs"));
}
