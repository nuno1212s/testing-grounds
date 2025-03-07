use std::io::{Read, Write};
use std::iter;
use std::sync::{Arc, LazyLock};

use serde::{Deserialize, Serialize};

use atlas_common::error::Result;
use atlas_smr_application::serialize::ApplicationData;
use atlas_smr_application::state::monolithic_state::MonolithicState;

pub struct MicrobenchmarkData;

macro_rules! build_rq_body {
    ($rq_size: expr) => {
        Arc::from(
            iter::repeat_with(|| fastrand::u8(..))
                .take($rq_size)
                .collect::<Vec<u8>>(),
        )
    };
}

pub static REQUEST_SIZE: LazyLock<usize> = LazyLock::new(|| {
    std::env::var("REQUEST_SIZE")
        .map(|s| s.parse().unwrap())
        .unwrap_or(0)
});
pub static STATE_SIZE: LazyLock<usize> = LazyLock::new(|| {
    std::env::var("STATE_SIZE")
        .map(|s| s.parse().unwrap())
        .unwrap_or(0)
});
pub static REPLY_SIZE: LazyLock<usize> = LazyLock::new(|| {
    std::env::var("REPLY_SIZE")
        .map(|s| s.parse().unwrap())
        .unwrap_or(0)
});
pub static REQUEST: LazyLock<Arc<[u8]>> = LazyLock::new(|| build_rq_body!(*REQUEST_SIZE));

pub static REPLY: LazyLock<Arc<[u8]>> = LazyLock::new(|| build_rq_body!(*REPLY_SIZE));

pub static STATE: LazyLock<Arc<[u8]>> = LazyLock::new(|| build_rq_body!(*STATE_SIZE));

pub static VERBOSE: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("VERBOSE")
        .map(|s| s.parse().unwrap())
        .unwrap_or(false)
});

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
    fn serialize_state<W>(mut w: W, _request: &Self) -> Result<()>
    where
        W: Write,
    {
        w.write_all(&STATE)?;

        Ok(())
    }

    fn deserialize_state<R>(_r: R) -> Result<Self>
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

    fn serialize_request<W>(mut w: W, request: &Self::Request) -> Result<()>
    where
        W: Write,
    {
        w.write_all(&request.inner)?;

        Ok(())
    }

    fn deserialize_request<R>(_r: R) -> Result<Self::Request>
    where
        R: Read,
    {
        /*let reader = capnp::serialize::read_message(r, Default::default())
            .context("Failed to deserialize request")?;

        let request_msg: messages_capnp::benchmark_request::Reader = reader
            .get_root()
            .context("Failed to read request message")?;

        let _data = request_msg
            .get_data()
            .context("Failed to get data from request message?");*/

        Ok(Request {
            inner: Arc::clone(&*REQUEST),
        })
    }

    fn serialize_reply<W>(mut w: W, reply: &Self::Reply) -> Result<()>
    where
        W: Write,
    {
        w.write_all(&reply.inner)?;

        Ok(())
    }

    fn deserialize_reply<R>(_r: R) -> Result<Self::Reply>
    where
        R: Read,
    {
        /*let reader = capnp::serialize::read_message(r, Default::default())
            .context("Failed to deserialize reply message")?;

        let request_msg: messages_capnp::benchmark_reply::Reader =
            reader.get_root().context("Failed to read reply message")?;

        let _data = request_msg
            .get_data()
            .context("Failed to get data from reply message?");*/

        Ok(Reply {
            inner: Arc::clone(&*REPLY),
        })
    }
}

mod messages_capnp {
    #![allow(unused)]
    include!(concat!(env!("OUT_DIR"), "/messages_capnp.rs"));
}
