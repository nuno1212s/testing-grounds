use anyhow::Context;
use std::default::Default;
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;

use atlas_common::collections::HashMap;
use konst::{
    option::unwrap_or,
    primitive::{parse_bool, parse_usize},
    unwrap_ctx,
};
use serde::{Deserialize, Serialize};

use atlas_common::error::*;
use atlas_smr_application::serialize::ApplicationData;
use atlas_smr_application::state::monolithic_state::MonolithicState;

pub struct MicrobenchmarkData;

impl MicrobenchmarkData {
    pub const KEY_SIZE: usize = {
        let result = parse_usize(env!("KEY_SIZE"));
        unwrap_ctx!(result)
    };

    pub const VALUE_SIZE: usize = {
        let result = parse_usize(env!("VALUE_SIZE"));
        unwrap_ctx!(result)
    };

    pub const PRE_LOADED_STATE_SIZE: usize = {
        let result = parse_usize(env!("PRE_LOADED_STATE_SIZE"));
        unwrap_ctx!(result)
    };

    pub fn get_measurement_interval() -> usize {
        std::env::var("MEASUREMENT_INTERVAL")
            .map(|s| s.parse().unwrap())
            .unwrap_or(1000)
    }

    pub fn get_ops_number() -> usize {
        std::env::var("OPS_NUMBER")
            .map(|s| s.parse().unwrap())
            .unwrap_or(1000)
    }

    pub fn get_request_sleep_millis() -> Duration {
        Duration::from_millis(
            std::env::var("REQUEST_SLEEP_MILLIS")
                .map(|s| s.parse().unwrap())
                .unwrap_or(0),
        )
    }

    pub const VERBOSE: bool = {
        let result = parse_bool(unwrap_or!(option_env!("VERBOSE"), "false"));
        unwrap_ctx!(result)
    };
}

pub type ValueType = str;

#[derive(Serialize, Deserialize, Clone)]
pub enum BERequest {
    Read(Arc<ValueType>),
    Write(Arc<ValueType>, Arc<ValueType>),
    Delete(Arc<ValueType>),
}

#[derive(Serialize, Deserialize, Clone)]
pub enum BEReply {
    None,
    Value(Arc<ValueType>),
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct State {
    pub(super) inner: HashMap<Arc<ValueType>, Arc<ValueType>>,
}

impl MonolithicState for State {
    fn serialize_state<W>(mut w: W, state: &Self) -> Result<()>
    where
        W: Write,
    {
        bincode::serde::encode_into_std_write(state, &mut w, bincode::config::standard())?;
        Ok(())
    }

    fn deserialize_state<R>(mut r: R) -> Result<Self>
    where
        R: Read,
        Self: Sized,
    {
        bincode::serde::decode_from_std_read(&mut r, bincode::config::standard())
            .context("Failed to deserialize request")
    }
}

impl ApplicationData for MicrobenchmarkData {
    type Request = BERequest;
    type Reply = BEReply;

    fn serialize_request<W>(mut w: W, request: &Self::Request) -> Result<()>
    where
        W: Write,
    {
        bincode::serde::encode_into_std_write(request, &mut w, bincode::config::standard())?;

        Ok(())
    }

    fn deserialize_request<R>(mut r: R) -> Result<Self::Request>
    where
        R: Read,
    {
        bincode::serde::decode_from_std_read(&mut r, bincode::config::standard())
            .context("Failed to deserialize request")
    }

    fn serialize_reply<W>(mut w: W, reply: &Self::Reply) -> Result<()>
    where
        W: Write,
    {
        bincode::serde::encode_into_std_write(reply, &mut w, bincode::config::standard())?;

        Ok(())
    }

    fn deserialize_reply<R>(mut r: R) -> Result<Self::Reply>
    where
        R: Read,
    {
        bincode::serde::decode_from_std_read(&mut r, bincode::config::standard())
            .context("Failed to deserialize request")
    }
}

impl From<Option<Arc<ValueType>>> for BEReply {
    fn from(value: Option<Arc<ValueType>>) -> Self {
        if let Some(value) = value {
            BEReply::Value(value)
        } else {
            BEReply::None
        }
    }
}

mod messages_capnp {
    #![allow(unused)]
    include!(concat!(env!("OUT_DIR"), "/messages_capnp.rs"));
}
