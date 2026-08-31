use anyhow::Context;
use atlas_common::collections::HashMap;
use atlas_common::error::*;
use atlas_smr_application::serialize::ApplicationData;
use atlas_smr_application::state::monolithic_state::MonolithicState;
use atlas_smr_preemptive_execution::CRUDState;
use getset::CopyGetters;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::time::Duration;

pub struct MicrobenchmarkData;

#[derive(Clone, Serialize, Deserialize, PartialOrd, PartialEq, Eq, Ord, Hash)]
pub struct Key(u64);

impl Key {
    pub fn from_index(index: u64) -> Self {
        Self(index)
    }

    pub fn random_in(key_space: u64) -> Self {
        Self(fastrand::u64(0..key_space))
    }

    /// Byte encoding used when this key crosses the `CRUDState` interface, which is
    /// byte-oriented. Fixed-width little-endian so the mapping is total and reversible.
    pub fn to_bytes(&self) -> [u8; 8] {
        self.0.to_le_bytes()
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        Some(Self(u64::from_le_bytes(bytes.try_into().ok()?)))
    }
}

/// The single column this benchmark's state uses. `CRUDState` is column-oriented; this
/// workload is a flat keyspace, so everything lives in one column.
pub const CRUD_COLUMN: &str = "microbenchmark";

#[derive(Serialize, Deserialize, Clone, CopyGetters)]
pub struct CRUDRequest {
    #[get_copy = "pub"]
    time_delay: Duration,
    request_type: CRUDRequestType,
}

impl CRUDRequest {
    pub fn new(time_delay: Duration, request_type: CRUDRequestType) -> Self {
        Self {
            time_delay,
            request_type,
        }
    }

    pub fn into_request_type(self) -> CRUDRequestType {
        self.request_type
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub enum CRUDRequestType {
    Read { key: Key },
    Create { key: Key, data: Vec<u8> },
    Update { key: Key, data: Vec<u8> },
    Delete { key: Key },
}

#[derive(Serialize, Deserialize, Clone)]
pub enum CRUDReply {
    ReadResult { data: Option<Vec<u8>> },
    WriteResult { previous_value: Option<Vec<u8>> },
    DeleteResult { previous_value: Option<Vec<u8>> },
}

#[derive(Serialize, Deserialize, Clone)]
pub struct State {
    inner: HashMap<Key, Vec<u8>>,
}

impl State {
    pub fn new() -> Self {
        Self {
            inner: HashMap::default(),
        }
    }

    pub fn get(&self, key: &Key) -> Option<Vec<u8>> {
        self.inner.get(key).cloned()
    }

    pub fn set(&mut self, key: &Key, value: Vec<u8>) -> Option<Vec<u8>> {
        self.inner.insert(key.clone(), value)
    }

    pub fn delete(&mut self, key: &Key) -> Option<Vec<u8>> {
        self.inner.remove(key)
    }
}

impl CRUDState for State {
    fn read(&self, _column: &str, key: &[u8]) -> Option<Vec<u8>> {
        self.get(&Key::from_bytes(key)?)
    }

    fn create(&mut self, _column: &str, key: &[u8], value: &[u8]) -> bool {
        let Some(key) = Key::from_bytes(key) else {
            return false;
        };

        if self.inner.contains_key(&key) {
            return false;
        }

        self.inner.insert(key, value.to_vec());

        true
    }

    fn update(&mut self, _column: &str, key: &[u8], value: &[u8]) -> Option<Vec<u8>> {
        self.set(&Key::from_bytes(key)?, value.to_vec())
    }

    fn delete(&mut self, _column: &str, key: &[u8]) -> Option<Vec<u8>> {
        // Fully qualified: `State` has an inherent `delete` too, and relying on arity alone
        // to disambiguate from this trait method would be a recursion waiting to happen.
        State::delete(self, &Key::from_bytes(key)?)
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
    type Request = CRUDRequest;
    type Reply = CRUDReply;

    fn serialize_request<W>(mut w: W, request: &Self::Request) -> Result<()>
    where
        W: Write,
    {
        bincode::serde::encode_into_std_write(request, &mut w, bincode::config::standard())
            .context("Failed to serialize request")?;
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
            .context("Failed to serialize reply")?;
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
