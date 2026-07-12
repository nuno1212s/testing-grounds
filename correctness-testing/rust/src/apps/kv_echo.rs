//! A minimal, fully deterministic monolithic application used as the workload for
//! correctness scenarios. It is an in-memory key→value store; each ordered `update`
//! writes a key and returns the running operation count, so decision-log byte-equality
//! and reply matching are trivially checkable.

use std::collections::BTreeMap;
use std::io::{Read, Write};

use atlas_common::error::*;
use atlas_smr_application::app::{Application, Reply, Request};
use atlas_smr_application::serialize::ApplicationData;
use atlas_smr_application::state::monolithic_state::MonolithicState;
use serde::{Deserialize, Serialize};

const BINCODE_CFG: bincode::config::Configuration = bincode::config::standard();

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EchoRequest {
    pub key: u64,
    pub value: u64,
}

impl EchoRequest {
    pub fn new(key: u64, value: u64) -> Self {
        EchoRequest { key, value }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EchoReply {
    /// The value stored for the key after applying this op.
    pub value: u64,
    /// Total ordered ops applied so far (monotonic, deterministic across replicas).
    pub op_count: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct KvState {
    map: BTreeMap<u64, u64>,
    op_count: u64,
}

impl MonolithicState for KvState {
    fn serialize_state<W>(mut w: W, state: &Self) -> Result<()>
    where
        W: Write,
    {
        bincode::serde::encode_into_std_write(state, &mut w, BINCODE_CFG)
            .map_err(|e| anyhow::anyhow!("serialize state: {e}"))?;
        Ok(())
    }

    fn deserialize_state<R>(mut r: R) -> Result<Self>
    where
        R: Read,
    {
        bincode::serde::decode_from_std_read(&mut r, BINCODE_CFG)
            .map_err(|e| anyhow::anyhow!("deserialize state: {e}"))
    }
}

pub struct KvEchoData;

impl ApplicationData for KvEchoData {
    type Request = EchoRequest;
    type Reply = EchoReply;

    fn serialize_request<W>(mut w: W, request: &Self::Request) -> Result<()>
    where
        W: Write,
    {
        bincode::serde::encode_into_std_write(request, &mut w, BINCODE_CFG)
            .map_err(|e| anyhow::anyhow!("serialize request: {e}"))?;
        Ok(())
    }

    fn deserialize_request<R>(mut r: R) -> Result<Self::Request>
    where
        R: Read,
    {
        bincode::serde::decode_from_std_read(&mut r, BINCODE_CFG)
            .map_err(|e| anyhow::anyhow!("deserialize request: {e}"))
    }

    fn serialize_reply<W>(mut w: W, reply: &Self::Reply) -> Result<()>
    where
        W: Write,
    {
        bincode::serde::encode_into_std_write(reply, &mut w, BINCODE_CFG)
            .map_err(|e| anyhow::anyhow!("serialize reply: {e}"))?;
        Ok(())
    }

    fn deserialize_reply<R>(mut r: R) -> Result<Self::Reply>
    where
        R: Read,
    {
        bincode::serde::decode_from_std_read(&mut r, BINCODE_CFG)
            .map_err(|e| anyhow::anyhow!("deserialize reply: {e}"))
    }
}

#[derive(Clone)]
pub struct KvEcho;

impl KvEcho {
    pub fn new() -> Self {
        KvEcho
    }
}

impl Default for KvEcho {
    fn default() -> Self {
        Self::new()
    }
}

impl Application<KvState> for KvEcho {
    type AppData = KvEchoData;

    fn initial_state() -> Result<KvState> {
        Ok(KvState::default())
    }

    fn unordered_execution(
        &self,
        state: &KvState,
        request: Request<Self, KvState>,
    ) -> Reply<Self, KvState> {
        // Read-only: report the current value without mutating.
        EchoReply {
            value: state.map.get(&request.key).copied().unwrap_or(0),
            op_count: state.op_count,
        }
    }

    fn update(&self, state: &mut KvState, request: Request<Self, KvState>) -> Reply<Self, KvState> {
        state.op_count += 1;
        state.map.insert(request.key, request.value);
        EchoReply {
            value: request.value,
            op_count: state.op_count,
        }
    }
}
