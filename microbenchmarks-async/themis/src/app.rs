use std::borrow::Cow;
use std::cmp::max;
use std::future::{Ready, ready};
use std::mem::size_of;
use chrono::{DateTime, Utc};

use themis_core::Result;
use themis_core::app;
use themis_core::app::{ApplyError, Request, Response};
use themis_core::net::Message;
use atlas_metrics::metrics::metric_increment;
use crate::{metrics, variables};

pub struct Microbenchmark {
    id: u64,
    max_tp: f32,
    batch_count: f32,
    tp_time: DateTime<Utc>,
    iterations: usize,

    reply_size: usize,
}

impl Microbenchmark {
    pub fn new(id: u64) -> Self {
        Microbenchmark {
            id,
            max_tp: 0.0,
            batch_count: 0.0,
            tp_time: Utc::now(),
            iterations: 0,
            reply_size: 0,
        }
    }
}

impl<'app> app::Application<'app> for Microbenchmark {
    type ExecuteFut = Ready<Result<Message<Response>>>;

    fn execute(&'app mut self, request: Message<Request>) -> Self::ExecuteFut {
        metric_increment(metrics::OPERATIONS_EXECUTED_PER_SECOND_ID, None);

        let size = max(self.reply_size, size_of::<u64>());

        let mut bytes = Vec::with_capacity(size as usize);

        if self.reply_size > size_of::<usize>() {
            bytes.extend_from_slice(&vec![0; self.reply_size - size_of::<usize>()]);
        }

        ready(Ok(
            Message::new(
                self.id,
                request.source,
                Response::with_contact(request.inner.sequence, bytes.into(), request.destination),
            )
        ))
    }

    type CheckpointHandle = Cow<'app, ()>;
    type CheckpointData = ();
    type TakeFut = Ready<Result<Self::CheckpointHandle>>;

    fn take_checkpoint(&'app mut self) -> Self::TakeFut {
        ready(Ok(Cow::Borrowed(&())))
    }

    type ApplyFut = Ready<std::result::Result<(), ApplyError>>;

    fn apply_checkpoint(&'app mut self, handle: Self::CheckpointHandle, checkpoint: Self::CheckpointData) -> Self::ApplyFut {
        ready(Ok(()))
    }

    type ResolveFut = Ready<Result<()>>;

    fn resolve_checkpoint(&'app mut self, handle: Self::CheckpointHandle) -> Self::ResolveFut {
        ready(Ok(()))
    }
}