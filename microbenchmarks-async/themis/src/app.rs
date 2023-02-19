use std::borrow::Cow;
use std::cmp::max;
use std::future::{Ready, ready};
use std::mem::size_of;
use chrono::{DateTime, Utc};

use themis_core::Result;
use themis_core::app;
use themis_core::app::{ApplyError, Request, Response};
use themis_core::net::Message;
use crate::variables;

pub struct Microbenchmark {
    id: u64,
    max_tp: f32,
    batch_count: f32,
    tp_time: DateTime<Utc>,
    iterations: usize,

    reply_size: usize,

    measurement_interval: usize,
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
            measurement_interval: variables::measurement_interval()
        }
    }
}

impl<'app> app::Application<'app> for Microbenchmark {
    type ExecuteFut = Ready<Result<Message<Response>>>;

    fn execute(&'app mut self, request: Message<Request>) -> Self::ExecuteFut {
        self.iterations += 1;

        if self.iterations % self.measurement_interval == 0 {
            println!("NodeId({:?}) // --- Measurements after {} ops ({} samples) ---",
                     self.id, self.iterations, self.measurement_interval);

            let time_diff = Utc::now()
                .signed_duration_since(self.tp_time)
                .num_microseconds().expect("Need microseconds");

            let throughput = (self.measurement_interval as f32 * 1000.0 * 1000.0) / (time_diff as f32);

            if throughput > self.max_tp {
                self.max_tp = throughput;
            }

            println!("NodeId({:?}) // Throughput = {} operations/sec (Maximum observed: {} ops/sec)",
                     self.id, throughput, self.max_tp);
        }

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

    type CheckpointHandle = Cow<'app, Vec<u8>>;
    type CheckpointData = ();
    type TakeFut = Ready<Result<Self::CheckpointHandle>>;

    fn take_checkpoint(&'app mut self) -> Self::TakeFut {
        todo!()
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