use std::time::SystemTime;
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

use crate::serialize::MicrobenchmarkData;

pub struct Microbenchmark {
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

    fn update(&mut self, _s: &mut Vec<u8>, _r: Vec<u8>) -> Weak<Vec<u8>> {
        unimplemented!()
    }

    fn update_batch(
        &mut self,
        _state: &mut Vec<u8>,
        batch: UpdateBatch<Vec<u8>>,
        mut meta: BatchMeta,
    ) -> UpdateBatchReplies<Weak<Vec<u8>>> {
        let mut reply_batch = UpdateBatchReplies::with_capacity(batch.len());

        meta.execution_time = SystemTime::now();

        for update in batch.into_inner() {
            let (peer_id, dig, _req) = update.into_inner();
            let reply = Arc::downgrade(&self.reply);
            reply_batch.add(peer_id, dig, reply);
        }

        // increase iter count
        self.iterations += 1;

        // take measurements
        meta.batch_size.store(&mut self.measurements.batch_size);
        (meta.consensus_decision_time, meta.consensus_start_time).store(&mut self.measurements.consensus_latency);
        (meta.consensus_start_time, meta.reception_time).store(&mut self.measurements.pre_cons_latency);
        (meta.execution_time, meta.consensus_decision_time).store(&mut self.measurements.pos_cons_latency);
        (meta.prepare_sent_time, meta.consensus_start_time).store(&mut self.measurements.pre_prepare_latency);
        (meta.commit_sent_time, meta.prepare_sent_time).store(&mut self.measurements.prepare_latency);
        (meta.consensus_decision_time, meta.commit_sent_time).store(&mut self.measurements.commit_latency);

        if self.iterations % MicrobenchmarkData::MEASUREMENT_INTERVAL == 0 {
            //
        }

        reply_batch
    }
}
