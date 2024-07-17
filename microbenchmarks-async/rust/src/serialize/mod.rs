use anyhow::Context;
use std::default::Default;
use std::io::{Read, Write};
use std::iter;
use std::sync::Arc;

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

use atlas_common::error::*;
use atlas_smr_application::serialize::ApplicationData;
use atlas_smr_application::state::monolithic_state::MonolithicState;

pub struct MicrobenchmarkData;

lazy_static! {
    pub static ref REQUEST_SIZE: usize = std::env::var("REQUEST_SIZE").unwrap().parse().unwrap();
    pub static ref REPLY_SIZE: usize = std::env::var("REPLY_SIZE").unwrap().parse().unwrap();
    pub static ref STATE_SIZE: usize = std::env::var("STATE_SIZE").unwrap().parse().unwrap();
    pub static ref REQUEST: Arc<[u8]> =
        Arc::from(iter::repeat(0u8).take(*REQUEST_SIZE).collect::<Vec<u8>>());
    pub static ref REPLY: Arc<[u8]> =
        Arc::from(iter::repeat(0u8).take(*REPLY_SIZE).collect::<Vec<u8>>());
    pub static ref STATE: Arc<[u8]> =
        Arc::from(iter::repeat(0u8).take(*STATE_SIZE).collect::<Vec<u8>>());
    pub static ref VERBOSE: bool = std::env::var("VERBOSE").unwrap_or(String::from("false")).parse().unwrap();
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Request {
    inner: Arc<[u8]>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Reply {
    inner: Arc<[u8]>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct State {
    inner: Arc<[u8]>,
}

impl Request {
    pub fn new(inner: Arc<[u8]>) -> Self {
        Self { inner }
    }
}

impl Reply {
    pub fn new(inner: Arc<[u8]>) -> Self {
        Self { inner }
    }
}

impl State {
    pub fn new(inner: Arc<[u8]>) -> Self {
        Self { inner }
    }
}

impl MonolithicState for State {
    fn serialize_state<W>(mut w: W, request: &Self) -> Result<()>
    where
        W: Write,
    {
        w.write_all(&**STATE)?;

        Ok(())
    }

    fn deserialize_state<R>(r: R) -> Result<Self>
    where
        R: Read,
        Self: Sized,
    {
        Ok(State {
            inner: Arc::clone(&*STATE),
        })
    }
}

impl ApplicationData for MicrobenchmarkData {
    type Request = Request;
    type Reply = Reply;

    fn serialize_request<W>(w: W, request: &Self::Request) -> Result<()>
    where
        W: Write,
    {
        let mut root = capnp::message::Builder::new(capnp::message::HeapAllocator::new());

        let mut rq_msg: messages_capnp::benchmark_request::Builder = root.init_root();

        rq_msg.set_data(&request.inner);

        capnp::serialize::write_message(w, &root).context("Failed to serialize request")?;

        Ok(())
    }

    fn deserialize_request<R>(r: R) -> Result<Self::Request>
    where
        R: Read,
    {
        let reader = capnp::serialize::read_message(r, Default::default())
            .context("Failed to deserialize request")?;

        let request_msg: messages_capnp::benchmark_request::Reader = reader
            .get_root()
            .context("Failed to read request message")?;

        let _data = request_msg
            .get_data()
            .context("Failed to get data from request message?");

        Ok(Request {
            inner: Arc::clone(&*REQUEST),
        })
    }

    fn serialize_reply<W>(w: W, reply: &Self::Reply) -> Result<()>
    where
        W: Write,
    {
        let mut root = capnp::message::Builder::new(capnp::message::HeapAllocator::new());

        let mut rq_msg: messages_capnp::benchmark_reply::Builder = root.init_root();

        rq_msg.set_data(&reply.inner);

        capnp::serialize::write_message(w, &root).context("Failed to serialize reply")
    }

    fn deserialize_reply<R>(r: R) -> Result<Self::Reply>
    where
        R: Read,
    {
        let reader = capnp::serialize::read_message(r, Default::default())
            .context("Failed to deserialize reply message")?;

        let request_msg: messages_capnp::benchmark_reply::Reader =
            reader.get_root().context("Failed to read reply message")?;

        let _data = request_msg
            .get_data()
            .context("Failed to get data from reply message?");

        Ok(Reply {
            inner: Arc::clone(&*REPLY),
        })
    }
}

mod messages_capnp {
    #![allow(unused)]
    include!(concat!(env!("OUT_DIR"), "/messages_capnp.rs"));
}
