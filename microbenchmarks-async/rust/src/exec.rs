use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use chrono::DateTime;
use chrono::offset::Utc;

use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_metrics::benchmarks::{BenchmarkHelperStore, Measurements};
use atlas_smr_application::app::{Application, BatchReplies, Reply, Request, UpdateBatch};

use crate::serialize;
use crate::serialize::{MicrobenchmarkData, State};

struct ExecData {
    max_tp: f32,
    batch_count: f32,
    max_tp_time: DateTime<Utc>,
    last_measurement: usize,
    iterations: usize,
    measurements: Measurements,
}

pub struct Microbenchmark {
    id: NodeId,
    reply: Arc<Vec<u8>>,
    exec_data: RefCell<ExecData>,
    measurement_interval: usize,
}

impl ExecData {
    pub fn new(id: NodeId) -> Self {
        Self {
            max_tp: -1.0,
            batch_count: 0.0,
            max_tp_time: Utc::now(),
            last_measurement: 0,
            iterations: 0,
            measurements: Measurements::new(id),
        }
    }
}

impl Microbenchmark {
    pub fn new(id: NodeId) -> Self {
        let reply = Arc::new((0..)
            .into_iter()
            .take(MicrobenchmarkData::REPLY_SIZE)
            .map(|x| (x & 0xff) as u8)
            .collect());

        Self {
            id: id.clone(),
            reply,
            exec_data: RefCell::new(ExecData::new(id)),
            measurement_interval: MicrobenchmarkData::get_measurement_interval(),
        }
    }
}

impl Application<State> for Microbenchmark {
    type AppData = MicrobenchmarkData;

    fn initial_state() -> Result<serialize::State> {
        Ok(serialize::State::new(MicrobenchmarkData::STATE))
    }

    fn unordered_execution(&self, state: &State, request: Request<Self, State>) -> Reply<Self, State> {
        serialize::Reply::new(MicrobenchmarkData::REPLY)
    }

    fn update(&self, state: &mut State, request: Request<Self, State>) -> Reply<Self, State> {
        let reply = serialize::Reply::new(MicrobenchmarkData::REPLY);

        let mut data = self.exec_data.borrow_mut();

        // increase iter count
        data.iterations += 1;

        if data.iterations % self.measurement_interval == 0 {
            println!("{:?} // --- Measurements after {} ops ({} samples) ---",
                     self.id, data.iterations, self.measurement_interval);

            let diff = Utc::now()
                .signed_duration_since(data.max_tp_time)
                .num_microseconds().expect("Need micro seconds");


            let tp = ((data.iterations - data.last_measurement) as f32 * 1000.0 * 1000.0) / (diff as f32);

            if tp > data.max_tp {
                data.max_tp = tp;
            }

            data.last_measurement = data.iterations;

            println!("{:?} // Throughput = {} operations/sec (Maximum observed: {} ops/sec)",
                     self.id, tp, data.max_tp);

            //This gives us the amount of batches per micro seconds, since the diff is in microseconds
            let mut batches_per_second = data.batch_count / diff as f32;

            //This moves the amount of batches per microsecond to the amount of batches per second
            batches_per_second *= 1000.0 * 1000.0;

            println!("{:?} // Batch throughput = {} batches/sec",
                     self.id, batches_per_second);

            //Reset the amount of batches
            data.batch_count = 0.0;

            data.measurements.total_latency.log_latency("Total");
            data.measurements.consensus_latency.log_latency("Consensus");
            data.measurements.pre_cons_latency.log_latency("Pre-consensus");
            data.measurements.pos_cons_latency.log_latency("Pos-consensus");
            data.measurements.pre_prepare_latency.log_latency("Propose / PrePrepare");
            data.measurements.prepare_latency.log_latency("Write / Prepare");
            data.measurements.commit_latency.log_latency("Accept / Commit");
            data.measurements.prepare_msg_latency.log_latency("Prepare msg");
            data.measurements.propose_time_latency.log_latency("Propose time");
            data.measurements.prepare_time_taken.log_latency("Prepare time taken");
            data.measurements.commit_time_taken.log_latency("Commit time taken");
            data.measurements.batch_size.log_batch();

            data.max_tp_time = Utc::now();
        }

        reply
    }


    fn update_batch(
        &self,
        _state: &mut State,
        mut batch: UpdateBatch<serialize::Request>,
    ) -> BatchReplies<serialize::Reply> {
        let batch_len = batch.len();

        let mut data = self.exec_data.borrow_mut();
        let meta = batch.take_meta();

        let mut reply_batch = BatchReplies::with_capacity(batch.len());

        for update in batch.into_inner() {
            let (peer_id, sess, opid, _req) = update.into_inner();
            reply_batch.add(peer_id, sess, opid, serialize::Reply::new(MicrobenchmarkData::REPLY));
        }

        data.batch_count += 1.0;

        if let Some(mut meta) = meta {
            meta.execution_time = Utc::now();
            // take measurements
            meta.batch_size.store(&mut data.measurements.batch_size);
            (meta.consensus_decision_time, meta.consensus_start_time).store(&mut data.measurements.consensus_latency);
            (meta.reception_time, meta.consensus_start_time).store(&mut data.measurements.pre_cons_latency);
            (meta.execution_time, meta.consensus_decision_time).store(&mut data.measurements.pos_cons_latency);
            (meta.prepare_sent_time, meta.consensus_start_time).store(&mut data.measurements.pre_prepare_latency);
            (meta.commit_sent_time, meta.prepare_sent_time).store(&mut data.measurements.prepare_latency);
            (meta.consensus_decision_time, meta.commit_sent_time).store(&mut data.measurements.commit_latency);
            (meta.prepare_sent_time, meta.pre_prepare_received_time).store(&mut data.measurements.prepare_msg_latency);
            (meta.done_propose, meta.started_propose).store(&mut data.measurements.propose_time_latency);
            (meta.commit_sent_time, meta.first_prepare_received).store(&mut data.measurements.prepare_time_taken);
            (meta.consensus_decision_time, meta.first_commit_received).store(&mut data.measurements.commit_time_taken);
        }

        for _ in 0..batch_len {
            // increase iter count
            data.iterations += 1;

            if data.iterations % self.measurement_interval == 0 && batch_len < self.measurement_interval {
                println!("{:?} // --- Measurements after {} ops ({} samples) ---",
                         self.id, data.iterations, self.measurement_interval);

                let diff = Utc::now()
                    .signed_duration_since(data.max_tp_time)
                    .num_microseconds().expect("Need micro seconds");


                let tp = ((data.iterations - data.last_measurement) as f32 * 1000.0 * 1000.0) / (diff as f32);

                if tp > data.max_tp {
                    data.max_tp = tp;
                }

                data.last_measurement = data.iterations;

                println!("{:?} // Throughput = {} operations/sec (Maximum observed: {} ops/sec)",
                         self.id, tp, data.max_tp);

                //This gives us the amount of batches per micro seconds, since the diff is in microseconds
                let mut batches_per_second = data.batch_count / diff as f32;

                //This moves the amount of batches per microsecond to the amount of batches per second
                batches_per_second *= 1000.0 * 1000.0;

                println!("{:?} // Batch throughput = {} batches/sec",
                         self.id, batches_per_second);

                //Reset the amount of batches
                data.batch_count = 0.0;

                data.measurements.total_latency.log_latency("Total");
                data.measurements.consensus_latency.log_latency("Consensus");
                data.measurements.pre_cons_latency.log_latency("Pre-consensus");
                data.measurements.pos_cons_latency.log_latency("Pos-consensus");
                data.measurements.pre_prepare_latency.log_latency("Propose / PrePrepare");
                data.measurements.prepare_latency.log_latency("Write / Prepare");
                data.measurements.commit_latency.log_latency("Accept / Commit");
                data.measurements.prepare_msg_latency.log_latency("Prepare msg");
                data.measurements.propose_time_latency.log_latency("Propose time");
                data.measurements.prepare_time_taken.log_latency("Prepare time taken");
                data.measurements.commit_time_taken.log_latency("Commit time taken");
                data.measurements.batch_size.log_batch();

                data.max_tp_time = Utc::now();
            }
        }

        reply_batch
    }
}