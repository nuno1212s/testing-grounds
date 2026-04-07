use anyhow::Context;
use atlas_common::collections::HashMap;
use atlas_common::error::*;
use atlas_smr_application::serialize::ApplicationData;
use atlas_smr_application::state::monolithic_state::MonolithicState;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::time::Duration;
use getset::CopyGetters;

pub struct MicrobenchmarkData;

#[derive(Clone, Serialize, Deserialize, PartialOrd, PartialEq, Eq, Ord, Hash)]
pub struct Key([u8; 4]);

impl Key {
    pub fn gen_random_key() -> Self {
        Self {
            0: fastrand::i32(..).to_be_bytes(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, CopyGetters)]
pub struct PERequest {
    #[get_copy = "pub"]
    time_delay: Duration,
    request_type: PERequestType
}

impl PERequest {
    
    pub fn new(time_delay: Duration, request_type: PERequestType) -> Self {
        Self { time_delay, request_type }
    }
    
    pub fn into_request_type(self) -> PERequestType {
        self.request_type
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub enum PERequestType {
    Read {
        cf_name: String,
        key: Key,
    },
    Write {
        cf_name: String,
        key: Key,
        data: Vec<u8>,
    },
    Delete {
        cf_name: String,
        key: Key,
    },
}

#[derive(Serialize, Deserialize, Clone)]
pub enum PEReply {
    ReadResult { data: Option<Vec<u8>> },
    WriteResult { previous_value: Option<Vec<u8>> },
}

#[derive(Serialize, Deserialize, Clone)]
pub struct State {
    inner: HashMap<String, HashMap<Key, Vec<u8>>>,
}

impl State {
    pub fn new() -> Self {
        Self {
            inner: HashMap::default(),
        }
    }

    pub fn get_value(&self, cf: &str, key: &Key) -> Option<Vec<u8>> {
        self.inner
            .get(cf)
            .and_then(|cf_map| cf_map.get(key).cloned())
    }

    pub fn set_value(&mut self, cf: &str, key: &Key, value: Vec<u8>) -> Option<Vec<u8>> {
        self.inner
            .entry(cf.to_string())
            .or_insert_with(HashMap::default)
            .insert(key.clone(), value)
    }

    pub fn delete_value(&mut self, cf: &str, key: &Key) -> Option<Vec<u8>> {
        self.inner.get_mut(cf).and_then(|cf_map| cf_map.remove(key))
    }
}

impl MonolithicState for State {
    fn serialize_state<W>(mut w: W, state: &Self) -> Result<()>
    where
        W: Write,
    {
        bincode::serde::encode_into_std_write(state, &mut w, bincode::config::standard())
            .context("Failed to serialize state")?;

        Ok(())
    }

    fn deserialize_state<R>(mut r: R) -> Result<Self>
    where
        R: Read,
        Self: Sized,
    {
        let result: Self =
            bincode::serde::decode_from_std_read(&mut r, bincode::config::standard())
                .context("Failed to deserialize state")?;

        Ok(result)
    }
}

impl ApplicationData for MicrobenchmarkData {
    type Request = PERequest;
    type Reply = PEReply;

    fn serialize_request<W>(mut w: W, request: &Self::Request) -> Result<()>
    where
        W: Write,
    {
        bincode::serde::encode_into_std_write(request, &mut w, bincode::config::standard())
            .context("Failed to serialize state")?;

        Ok(())
    }

    fn deserialize_request<R>(mut r: R) -> Result<Self::Request>
    where
        R: Read,
    {
        let request = bincode::serde::decode_from_std_read(&mut r, bincode::config::standard())
            .context("Failed to deserialize request")?;

        Ok(request)
    }

    fn serialize_reply<W>(mut w: W, reply: &Self::Reply) -> Result<()>
    where
        W: Write,
    {
        bincode::serde::encode_into_std_write(reply, &mut w, bincode::config::standard())
            .context("Failed to serialize state")?;

        Ok(())
    }

    fn deserialize_reply<R>(mut r: R) -> Result<Self::Reply>
    where
        R: Read,
    {
        let result = bincode::serde::decode_from_std_read(&mut r, bincode::config::standard())
            .context("Failed to deserialize reply")?;

        Ok(result)
    }
}
