use std::time::Duration;

use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_core::execution::requests::{IncrementableUpdateBatch, ReplyBatch, UpdateBatch};
use atlas_metrics::metrics::{
    metric_local_duration_end, metric_local_duration_start, metric_store_count,
};
use atlas_smr_application::app::{Application, Reply, Request};

use crate::metric::{CRUD_BATCH_EXEC_TIME_ID, CRUD_OP_EXEC_TIME_ID, CRUD_OPS_PER_BATCH_ID};
use crate::serialize::{CRUDReply, CRUDRequest, CRUDRequestType, MicrobenchmarkData, State};

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
        let start = metric_local_duration_start();

        let reply = match request.into_request_type() {
            CRUDRequestType::Read { key } => handle_read(state, &key),
            _ => unreachable!("Non-read request routed as unordered"),
        };

        metric_local_duration_end(CRUD_OP_EXEC_TIME_ID, start);

        if sleep_duration > Duration::ZERO {
            std::thread::sleep(sleep_duration);
        }

        reply
    }

    fn update(&self, state: &mut State, request: Request<Self, State>) -> Reply<Self, State> {
        let sleep_duration = request.time_delay();
        let start = metric_local_duration_start();

        let reply = match request.into_request_type() {
            CRUDRequestType::Read { key } => handle_read(state, &key),
            CRUDRequestType::Create { key, data } => handle_write(state, &key, data),
            CRUDRequestType::Update { key, data } => handle_write(state, &key, data),
            CRUDRequestType::Delete { key } => handle_delete(state, &key),
        };

        metric_local_duration_end(CRUD_OP_EXEC_TIME_ID, start);

        if sleep_duration > Duration::ZERO {
            std::thread::sleep(sleep_duration);
        }

        reply
    }

    fn update_batch(
        &self,
        state: &mut State,
        batch: UpdateBatch<CRUDRequest>,
    ) -> ReplyBatch<CRUDReply> {
        let batch_start = metric_local_duration_start();
        let batch_len = batch.len();

        let mut reply_batch = ReplyBatch::new_with_cap(batch_len);
        let (_seq_no, requests) = batch.into_inner();

        for request in requests {
            let (update_info, request) = request.into_inner();
            let reply = self.update(state, request);
            reply_batch.add(update_info, reply);
        }

        metric_local_duration_end(CRUD_BATCH_EXEC_TIME_ID, batch_start);
        metric_store_count(CRUD_OPS_PER_BATCH_ID, batch_len);

        reply_batch
    }
}

fn handle_read(state: &State, key: &crate::serialize::Key) -> CRUDReply {
    CRUDReply::ReadResult {
        data: state.get(key),
    }
}

fn handle_write(state: &mut State, key: &crate::serialize::Key, value: Vec<u8>) -> CRUDReply {
    CRUDReply::WriteResult {
        previous_value: state.set(key, value),
    }
}

fn handle_delete(state: &mut State, key: &crate::serialize::Key) -> CRUDReply {
    CRUDReply::DeleteResult {
        previous_value: state.delete(key),
    }
}
