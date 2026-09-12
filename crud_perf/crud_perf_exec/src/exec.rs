use std::time::Duration;

use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_core::execution::requests::{IncrementableUpdateBatch, ReplyBatch, UpdateBatch};
use atlas_metrics::metrics::{
    metric_local_duration_end, metric_local_duration_start, metric_store_count,
};
use atlas_smr_application::app::{Application, Reply, Request};

use crate::metric::{
    CRUD_BATCH_EXEC_TIME_ID, CRUD_OP_EXEC_TIME_ID, CRUD_OPS_PER_BATCH_ID,
    CRUD_SPEC_OP_EXEC_TIME_ID, CRUD_UNORDERED_OP_EXEC_TIME_ID,
};
use crate::serialize::{
    CRUD_COLUMN, CRUDReply, CRUDRequest, CRUDRequestType, MicrobenchmarkData, State,
};
use atlas_smr_preemptive_execution::{CRUDApplication, CRUDState};

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

        metric_local_duration_end(CRUD_UNORDERED_OP_EXEC_TIME_ID, start);

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

impl CRUDApplication<State> for Microbenchmark {
    /// Speculative counterpart to [`Application::update`].
    ///
    /// This must produce byte-identical replies and state effects to `update`, or speculation
    /// diverges from the confirmed execution and the executor backtracks (or worse, silently
    /// disagrees). Two things are deliberate here:
    ///
    /// - `Create` routes through `CRUDState::update`, not `CRUDState::create`. `update`'s
    ///   `handle_write` uses overwrite-insert semantics and returns the previous value, whereas
    ///   `CRUDState::create` is insert-if-absent returning a `bool`. Using `create` would both
    ///   change the reply and diverge on an already-present key.
    /// - The `time_delay` sleep is applied here too. It models the application's own work; if
    ///   speculation skipped it, speculative execution would look artificially fast and bias
    ///   the very comparison this benchmark exists to make.
    fn speculatively_execute(
        &self,
        state: &mut impl CRUDState,
        request: Request<Self, State>,
    ) -> Reply<Self, State> {
        let sleep_duration = request.time_delay();
        let start = metric_local_duration_start();

        let reply = match request.into_request_type() {
            CRUDRequestType::Read { key } => CRUDReply::ReadResult {
                data: state.read(CRUD_COLUMN, &key.to_bytes()),
            },
            CRUDRequestType::Create { key, data } | CRUDRequestType::Update { key, data } => {
                CRUDReply::WriteResult {
                    previous_value: state.update(CRUD_COLUMN, &key.to_bytes(), &data),
                }
            }
            CRUDRequestType::Delete { key } => CRUDReply::DeleteResult {
                previous_value: state.delete(CRUD_COLUMN, &key.to_bytes()),
            },
        };

        // Deliberately not CRUD_OP_EXEC_TIME: dual_state runs every operation on both
        // paths, so sharing the name would report one average over two different pieces
        // of work and hide the re-execution cost this benchmark is trying to price.
        metric_local_duration_end(CRUD_SPEC_OP_EXEC_TIME_ID, start);

        if sleep_duration > Duration::ZERO {
            std::thread::sleep(sleep_duration);
        }

        reply
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
