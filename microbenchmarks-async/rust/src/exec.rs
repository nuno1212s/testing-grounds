use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use chrono::offset::Utc;
use chrono::DateTime;

use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_metrics::benchmarks::{BenchmarkHelperStore, Measurements};
use atlas_smr_application::app::{Application, BatchReplies, Reply, Request, UpdateBatch};

use crate::serialize;
use crate::serialize::{MicrobenchmarkData, State, REPLY, REPLY_SIZE, STATE};

pub struct Microbenchmark {
    id: NodeId,
}

impl Microbenchmark {
    pub fn new(id: NodeId) -> Self {
        Self { id: id.clone() }
    }
}

impl Application<State> for Microbenchmark {
    type AppData = MicrobenchmarkData;

    fn initial_state() -> Result<State> {
        Ok(State::new(Arc::clone(&*STATE)))
    }

    fn unordered_execution(
        &self,
        _state: &State,
        _request: Request<Self, State>,
    ) -> Reply<Self, State> {
        serialize::Reply::new(Arc::clone(&*REPLY))
    }

    fn update(&self, _state: &mut State, _request: Request<Self, State>) -> Reply<Self, State> {
        let reply = serialize::Reply::new(Arc::clone(&*REPLY));
        reply
    }

    fn update_batch(
        &self,
        _state: &mut State,
        mut batch: UpdateBatch<serialize::Request>,
    ) -> BatchReplies<serialize::Reply> {
        let batch_len = batch.len();

        let mut reply_batch = BatchReplies::with_capacity(batch.len());

        for update in batch.into_inner() {
            let (peer_id, sess, opid, _req) = update.into_inner();
            reply_batch.add(
                peer_id,
                sess,
                opid,
                serialize::Reply::new(Arc::clone(&*REPLY)),
            );
        }

        reply_batch
    }
}
