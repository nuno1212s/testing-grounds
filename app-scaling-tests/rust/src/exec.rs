use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_smr_application::app::{Application, Reply, Request};

use crate::serialize::{BERequest, MicrobenchmarkData, State};

pub struct Microbenchmark {
    id: NodeId,
}

impl Microbenchmark {
    pub fn new(id: NodeId) -> Self {
        Self { id }
    }
}

impl Application<State> for Microbenchmark {
    type AppData = MicrobenchmarkData;

    fn initial_state() -> Result<State> {
        Ok(State::default())
    }

    fn unordered_execution(
        &self,
        state: &State,
        request: Request<Self, State>,
    ) -> Reply<Self, State> {
        match request {
            BERequest::Read(key) => {
                let result = state.inner.get(&key);

                result.cloned().into()
            }
            _ => unreachable!(),
        }
    }

    fn update(&self, state: &mut State, request: Request<Self, State>) -> Reply<Self, State> {
        match request {
            BERequest::Write(key, value) => {
                let val = state.inner.insert(key, value);

                val.into()
            }
            BERequest::Delete(key) => {
                let result = state.inner.remove(&key);

                result.into()
            }
            BERequest::Read(key) => {
                let result = state.inner.get(&key);

                result.cloned().into()
            }
        }
    }
}
