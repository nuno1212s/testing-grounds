use std::default::Default;
use std::sync::{Arc, Weak};

use febft::bft::error::*;
use febft::bft::benchmarks::{
    BatchMeta,
    Measurements,
    BenchmarkHelperStore,
};
use febft::bft::executable::{
    Service,
    UpdateBatch,
    UpdateBatchReplies,
};

use chrono::DateTime;
use chrono::offset::Utc;

use crate::serialize::MicrobenchmarkData;

pub struct Microbenchmark {
    max_tp: f32,
    max_tp_time: DateTime<Utc>,
    iterations: usize,
    reply: Arc<Vec<u8>>,
    measurements: Measurements,
}

impl Microbenchmark {
    pub fn new() -> Self {
        let reply = Arc::new((0..)
            .into_iter()
            .take(MicrobenchmarkData::REPLY_SIZE)
            .map(|x| (x & 0xff) as u8)
            .collect());

        Self {
            reply,
            max_tp: -1.0,
            max_tp_time: Utc::now(),
            iterations: 0,
            measurements: Measurements::default(),
        }
    }
}

impl Service for Microbenchmark {
    type Data = MicrobenchmarkData;

    fn initial_state(&mut self) -> Result<Vec<u8>> {
        // TODO: remove this
        drop(MicrobenchmarkData::REQUEST_SIZE);

        Ok((0..)
            .into_iter()
            .take(MicrobenchmarkData::STATE_SIZE)
            .map(|x| (x & 0xff) as u8)
            .collect())
    }

    fn update(&mut self, _s: &mut Vec<u8>, _r: Weak<Vec<u8>>) -> Weak<Vec<u8>> {
        unimplemented!()
    }

    fn update_batch(
        &mut self,
        _state: &mut Vec<u8>,
        batch: UpdateBatch<Weak<Vec<u8>>>,
        mut meta: BatchMeta,
    ) -> UpdateBatchReplies<Weak<Vec<u8>>> {
        let mut reply_batch = UpdateBatchReplies::with_capacity(batch.len());

        for update in batch.into_inner() {
            let (peer_id, sess, opid, _req) = update.into_inner();
            let reply = Arc::downgrade(&self.reply);
            reply_batch.add(peer_id, sess, opid, reply);
        }

        // increase iter count
        self.iterations += 1;

        meta.execution_time = Utc::now();

        // take measurements
        meta.batch_size.store(&mut self.measurements.batch_size);
        (meta.consensus_decision_time, meta.consensus_start_time).store(&mut self.measurements.consensus_latency);
        (meta.consensus_start_time, meta.reception_time).store(&mut self.measurements.pre_cons_latency);
        (meta.execution_time, meta.consensus_decision_time).store(&mut self.measurements.pos_cons_latency);
        (meta.prepare_sent_time, meta.consensus_start_time).store(&mut self.measurements.pre_prepare_latency);
        (meta.commit_sent_time, meta.prepare_sent_time).store(&mut self.measurements.prepare_latency);
        (meta.consensus_decision_time, meta.commit_sent_time).store(&mut self.measurements.commit_latency);

        if self.iterations % MicrobenchmarkData::MEASUREMENT_INTERVAL == 0 {
            println!("--- Measurements after {} ops ({} samples) ---", self.iterations, MicrobenchmarkData::MEASUREMENT_INTERVAL);

            let diff = Utc::now()
                .signed_duration_since(self.max_tp_time)
                .num_milliseconds();
            let tp = (MicrobenchmarkData::MEASUREMENT_INTERVAL as f32 * 1000.0) / (diff as f32);

            if tp > self.max_tp {
                self.max_tp = tp;
            }

            println!("Throughput = {} operations/sec (Maximum observed: {} ops/sec)", tp, self.max_tp);

            self.measurements.total_latency.log_latency("Total");
            self.measurements.consensus_latency.log_latency("Consensus");
            self.measurements.pre_cons_latency.log_latency("Pre-consensus");
            self.measurements.pos_cons_latency.log_latency("Pos-consensus");
            self.measurements.pre_prepare_latency.log_latency("Propose");
            self.measurements.prepare_latency.log_latency("Write");
            self.measurements.commit_latency.log_latency("Accept");
            self.measurements.batch_size.log_batch();

            self.max_tp_time = Utc::now();
        }

        reply_batch
    }
}
