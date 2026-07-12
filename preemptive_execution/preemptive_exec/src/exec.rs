use std::time::Duration;
use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_core::execution::requests::{IncrementableUpdateBatch, ReplyBatch, UpdateBatch};
use atlas_smr_application::app::{Application, Reply, Request};

use crate::serialize::{Key, MicrobenchmarkData, PEReply, PERequest, PERequestType, State};

#[derive(Clone)]
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
        Ok(State::new())
    }

    fn unordered_execution(
        &self,
        state: &State,
        request: Request<Self, State>,
    ) -> Reply<Self, State> {
        let sleep_duration = request.time_delay();
        
        let reply = match request.into_request_type() {
            PERequestType::Read { cf_name, key } => Self::handle_read_request(state, &cf_name, &key),
            _ => unreachable!(""),
        };
        
        if sleep_duration > Duration::ZERO {
            std::thread::sleep(sleep_duration);
        }
        
        reply
    }

    fn update(&self, state: &mut State, request: Request<Self, State>) -> Reply<Self, State> {
        let sleep_duration = request.time_delay();
        
        let reply = match request.into_request_type() {
            PERequestType::Read { cf_name, key } => Self::handle_read_request(state, &cf_name, &key),
            PERequestType::Write { cf_name, key, data } => {
                Self::handle_write_request(state, &cf_name, &key, data)
            }
            PERequestType::Delete { cf_name, key } => {
                Self::handle_delete_request(state, &cf_name, &key)
            }
        };
        
        if sleep_duration > Duration::ZERO {
            std::thread::sleep(sleep_duration);
        }
        
        reply
    }

    fn update_batch(
        &self,
        state: &mut State,
        batch: UpdateBatch<PERequest>,
    ) -> ReplyBatch<PEReply> {
        let mut reply_batch = ReplyBatch::new_with_cap(batch.len());

        let (_seq_no, requests) = batch.into_inner();

        for request in requests {

            let (update_info, request) = request.into_inner();

            let reply = self.update(state, request);

            reply_batch.add(update_info, reply);
        }

        reply_batch
    }
}

impl Microbenchmark {
    fn handle_read_request(state: &State, cf_name: &str, key: &Key) -> PEReply {
        let result = state.get_value(cf_name, key);

        PEReply::ReadResult { data: result }
    }

    fn handle_write_request(
        state: &mut State,
        cf_name: &str,
        key: &Key,
        value: Vec<u8>,
    ) -> PEReply {
        let prev_value = state.set_value(cf_name, key, value);

        PEReply::WriteResult {
            previous_value: prev_value,
        }
    }

    fn handle_delete_request(state: &mut State, cf_name: &str, key: &Key) -> PEReply {
        let prev_value = state.delete_value(cf_name, key);

        PEReply::WriteResult {
            previous_value: prev_value,
        }
    }
}
