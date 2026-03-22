use std::sync::Arc;

use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_core::execution::requests::{IncrementableUpdateBatch, ReplyBatch, UpdateBatch};
use atlas_smr_application::app::{Application, Reply, Request};

use crate::serialize;
use crate::serialize::{MicrobenchmarkData, REPLY, State, STATE};

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
        serialize::Reply::new(Arc::clone(&*REPLY))
    }

    fn update_batch(
        &self,
        _state: &mut State,
        batch: UpdateBatch<serialize::Request>,
    ) -> ReplyBatch<serialize::Reply> {
        let mut reply_batch = ReplyBatch::new_with_cap(batch.len());

        let (_, updates) = batch.into_inner();

        for update in updates {
            let (update_info, _request) = update.into_inner();
            reply_batch.add(
                update_info,
                serialize::Reply::new(Arc::clone(&*REPLY)),
            );
        }

        reply_batch
    }
}
