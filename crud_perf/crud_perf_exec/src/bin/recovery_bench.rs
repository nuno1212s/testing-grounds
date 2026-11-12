//! Backtrack-recovery micro-benchmark for the three speculative executors.
//!
//! Drives each executor through the same public handle and message sequence, using the real
//! crud_perf application and `State`, and times a re-proposal of an already-speculated slot from
//! the outside: "re-proposal sent" to "corrected reply arrives". Every reply is checked against a
//! sequential reference model; an executor that stops answering is a timeout, not a hang.
//!
//! dual_state recovers by cloning the confirmed state and re-executing kept batches; the CRUD
//! executors drop the later deltas and re-merge the rest.
//!
//! One process per (executor, state size): the executors have no shutdown. See
//! suite/run_recovery.py.

#![allow(dead_code)]

#[path = "../exec.rs"]
mod exec;
#[path = "../knobs.rs"]
mod knobs;
#[path = "../metric/mod.rs"]
mod metric;
#[path = "../serialize/mod.rs"]
mod serialize;

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use clap::Parser;
use rand::{Rng, SeedableRng};
use rand_xoshiro::SplitMix64;

use atlas_common::node_id::NodeId;
use atlas_common::ordering::SeqNo;
use atlas_common::{InitConfig, init};
use atlas_core::execution::requests::{IncrementableUpdateBatch, UpdateBatch, UpdateInfo};
use atlas_smr_application::preemptive_execution::TPreemptiveExecutionHandle;
use atlas_smr_core::SMRReply;
use atlas_smr_core::execution::TExecutor;
use atlas_smr_core::execution::executors::monolithic_state::TMonolithicStateExecutor;
use atlas_smr_core::execution::reply::{ReplyNode, RequestType};
use atlas_smr_preemptive_execution::exec_handle::PreemptiveExecutorHandle;
use atlas_smr_preemptive_execution::{
    CRUDMonolithicPreemptiveExecutor, MonolithicPreemptiveExecutor,
    ScalableCRUDMonolithicPreemptiveExecutor,
};

use crate::exec::Microbenchmark;
use crate::serialize::{CRUDReply, CRUDRequest, CRUDRequestType, Key, MicrobenchmarkData, State};

#[derive(Parser, Debug, Clone)]
#[command(about = "Backtrack-recovery micro-benchmark (see the module docs)")]
struct Args {
    /// dual_state | crud_single | crud_scalable
    #[arg(long)]
    executor: String,

    /// recovery | snapshot | primitives (State::clone and cache rebuild alone; --executor primitives)
    #[arg(long, default_value = "recovery")]
    scenario: String,

    /// Keys in the application state before the first batch.
    #[arg(long, default_value_t = 100_000)]
    state_keys: u64,

    /// Bytes per value, both preloaded and written.
    #[arg(long, default_value_t = 64)]
    value_bytes: usize,

    /// Batches speculated ahead of confirmation when the backtrack hits.
    #[arg(long, value_delimiter = ',', default_value = "32")]
    depths: Vec<usize>,

    /// head = oldest unconfirmed batch (nothing kept); tail = newest (all older kept).
    #[arg(long, value_delimiter = ',', default_value = "head,tail")]
    positions: Vec<String>,

    /// Batches confirmed in a burst just before the backtrack.
    #[arg(long, value_delimiter = ',', default_value = "0")]
    backlogs: Vec<usize>,

    /// Operations per batch.
    #[arg(long, value_delimiter = ',', default_value = "100")]
    batch_sizes: Vec<usize>,

    /// Application cost per operation, in microseconds.
    #[arg(long, value_delimiter = ',', default_value = "0")]
    op_cost_us: Vec<u64>,

    /// Percentage of operations that write (the rest are reads).
    #[arg(long, default_value_t = 50)]
    write_pct: u32,

    /// one = re-propose only the backtracked slot; all = every later slot too.
    #[arg(long, default_value = "one")]
    reproposal: String,

    /// Pause between the confirmation burst and the re-proposal (lets async work drain).
    #[arg(long, default_value_t = 0)]
    pre_trigger_wait_ms: u64,

    /// Minimum pause before each configuration (raised to 2x the previous one's execution work),
    /// so leftover async work cannot leak into its first trial.
    #[arg(long, default_value_t = 500)]
    config_pause_ms: u64,

    #[arg(long, default_value_t = 30)]
    trials: usize,

    /// Trial timeout; the executor is then treated as wedged.
    #[arg(long, default_value_t = 30)]
    timeout_s: u64,

    #[arg(long, default_value_t = 1)]
    seed: u64,

    /// Skip checking replies against the reference model.
    #[arg(long)]
    no_verify: bool,

    /// Append one JSON line per trial here (default: stdout).
    #[arg(long)]
    out: Option<PathBuf>,
}

// ── the network node the executors reply to ──────────────────────────────────────────

struct Reply {
    at: Instant,
    session: u32,
    op: u32,
    reply: CRUDReply,
}

/// Records every reply with its arrival time, in place of the replica's network node.
struct RecorderNode {
    tx: Mutex<mpsc::Sender<Reply>>,
}

impl RecorderNode {
    fn record(&self, reply: SMRReply<MicrobenchmarkData>) -> atlas_common::error::Result<()> {
        let at = Instant::now();
        let (session, op, reply) = reply.into_inner();
        let _ = self.tx.lock().unwrap().send(Reply {
            at,
            session: u32::from(session),
            op: u32::from(op),
            reply,
        });
        Ok(())
    }
}

impl ReplyNode<SMRReply<MicrobenchmarkData>> for RecorderNode {
    fn send(
        &self,
        _rt: RequestType,
        reply: SMRReply<MicrobenchmarkData>,
        _target: NodeId,
        _flush: bool,
    ) -> atlas_common::error::Result<()> {
        self.record(reply)
    }

    fn send_signed(
        &self,
        _rt: RequestType,
        reply: SMRReply<MicrobenchmarkData>,
        _target: NodeId,
        _flush: bool,
    ) -> atlas_common::error::Result<()> {
        self.record(reply)
    }

    fn broadcast(
        &self,
        _rt: RequestType,
        _reply: SMRReply<MicrobenchmarkData>,
        _targets: impl Iterator<Item = NodeId>,
    ) -> Result<(), Vec<NodeId>> {
        Ok(())
    }

    fn broadcast_signed(
        &self,
        _rt: RequestType,
        _reply: SMRReply<MicrobenchmarkData>,
        _targets: impl Iterator<Item = NodeId>,
    ) -> Result<(), Vec<NodeId>> {
        Ok(())
    }
}

// ── reference model ──────────────────────────────────────────────────────────────────

/// Sequential model of the application; untouched keys hold their preloaded value.
struct Reference {
    initial_keys: u64,
    value_bytes: usize,
    touched: HashMap<u64, Option<Vec<u8>>>,
    live: i64,
}

impl Reference {
    fn new(initial_keys: u64, value_bytes: usize) -> Self {
        Self {
            initial_keys,
            value_bytes,
            touched: HashMap::new(),
            live: initial_keys as i64,
        }
    }

    fn current(&self, key: u64) -> Option<Vec<u8>> {
        match self.touched.get(&key) {
            Some(v) => v.clone(),
            None if key < self.initial_keys => Some(vec![0u8; self.value_bytes]),
            None => None,
        }
    }

    fn set(&mut self, key: u64, value: Option<Vec<u8>>) {
        let before = self.current(key).is_some();
        let after = value.is_some();
        self.live += after as i64 - before as i64;
        self.touched.insert(key, value);
    }

    /// Mirrors `Microbenchmark::update` (Create and Update both overwrite).
    fn apply(&mut self, request: &CRUDRequestType) -> CRUDReply {
        match request {
            CRUDRequestType::Read { key } => CRUDReply::ReadResult {
                data: self.current(index(key)),
            },
            CRUDRequestType::Create { key, data } | CRUDRequestType::Update { key, data } => {
                let previous = self.current(index(key));
                self.set(index(key), Some(data.clone()));
                CRUDReply::WriteResult {
                    previous_value: previous,
                }
            }
            CRUDRequestType::Delete { key } => {
                let previous = self.current(index(key));
                self.set(index(key), None);
                CRUDReply::DeleteResult {
                    previous_value: previous,
                }
            }
        }
    }
}

fn index(key: &Key) -> u64 {
    u64::from_le_bytes(key.to_bytes())
}

// ── workload ─────────────────────────────────────────────────────────────────────────

/// A deterministic batch of requests; `version` tags written values so a replacement batch is
/// distinguishable from the original.
fn make_ops(
    args: &Args,
    seed: u64,
    version: u8,
    batch_size: usize,
    cost: Duration,
) -> Vec<CRUDRequest> {
    let mut rng = SplitMix64::seed_from_u64(seed);
    let value = vec![version; args.value_bytes];

    (0..batch_size)
        .map(|_| {
            let key = Key::from_index(rng.gen_range(0..args.state_keys.max(1)));

            let request = if rng.gen_range(0..100u32) < args.write_pct {
                match rng.gen_range(0..10u8) {
                    0 => CRUDRequestType::Create {
                        key,
                        data: value.clone(),
                    },
                    1 => CRUDRequestType::Delete { key },
                    _ => CRUDRequestType::Update {
                        key,
                        data: value.clone(),
                    },
                }
            } else {
                CRUDRequestType::Read { key }
            };

            CRUDRequest::new(cost, request)
        })
        .collect()
}

fn make_batch(seq: u32, ops: &[CRUDRequest]) -> UpdateBatch<CRUDRequest> {
    let mut batch = UpdateBatch::new(SeqNo::from(seq));
    for (i, op) in ops.iter().enumerate() {
        // session = batch seq, operation = index: identifies the request a reply answers.
        batch.add(
            UpdateInfo::new_session_based(
                NodeId::from(0u32),
                SeqNo::from(seq),
                SeqNo::from(i as u32),
            ),
            op.clone(),
        );
    }
    batch
}

fn request_type(op: &CRUDRequest) -> CRUDRequestType {
    op.clone().into_request_type()
}

// ── the harness around one executor ──────────────────────────────────────────────────

struct Harness {
    handle: PreemptiveExecutorHandle<CRUDRequest>,
    replies: mpsc::Receiver<Reply>,
    checkpoints: atlas_common::channel::sync::ChannelSyncRx<
        atlas_smr_application::state::monolithic_state::AppStateMessage<State>,
    >,
    /// Replies received early, by batch.
    buffered: HashMap<u32, Vec<Reply>>,
    reference: Reference,
    next_seq: u32,
    timeout: Duration,
}

#[derive(Debug)]
enum TrialError {
    Timeout { missing: Vec<u32>, got: usize },
    Send(String),
}

impl Harness {
    fn spec(&self, seq: u32, ops: &[CRUDRequest]) -> Result<(), TrialError> {
        self.handle
            .queue_preemptive_update(make_batch(seq, ops))
            .map_err(|e| TrialError::Send(e.to_string()))
    }

    fn fin(&self, seq: u32) -> Result<(), TrialError> {
        self.handle
            .queue_update_finalized(SeqNo::from(seq))
            .map_err(|e| TrialError::Send(e.to_string()))
    }

    fn drain_nonblocking(&mut self) {
        while let Ok(r) = self.replies.try_recv() {
            self.buffered.entry(r.session).or_default().push(r);
        }
    }

    /// Wait until every batch in `wanted` has all its replies, or time out.
    fn wait_for(&mut self, wanted: &[u32], batch_size: usize) -> Result<(), TrialError> {
        let deadline = Instant::now() + self.timeout;
        loop {
            self.drain_nonblocking();

            let missing: Vec<u32> = wanted
                .iter()
                .copied()
                .filter(|s| self.buffered.get(s).map_or(0, |v| v.len()) < batch_size)
                .collect();
            if missing.is_empty() {
                return Ok(());
            }

            let now = Instant::now();
            if now >= deadline {
                let got = wanted
                    .iter()
                    .map(|s| self.buffered.get(s).map_or(0, |v| v.len()))
                    .sum();
                return Err(TrialError::Timeout { missing, got });
            }

            if let Ok(r) = self
                .replies
                .recv_timeout((deadline - now).min(Duration::from_millis(50)))
            {
                self.buffered.entry(r.session).or_default().push(r);
            }
        }
    }

    fn last_arrival(&self, seq: u32) -> Option<Instant> {
        self.buffered.get(&seq)?.iter().map(|r| r.at).max()
    }
}

struct Outcome {
    ttr_first_ms: f64,
    ttr_full_ms: f64,
    kept: usize,
    reexecuted_batches: usize,
    mismatches: usize,
    stale_replies: usize,
}

/// Slot (relative to the batch base) that the backtrack hits.
fn backtrack_slot(position: &str, depth: usize, backlog: usize) -> usize {
    match position {
        "head" => backlog + 1,
        "tail" => depth - 1,
        other => panic!("position must be head or tail, got {other:?}"),
    }
}

/// One backtrack: speculate `depth` batches, confirm `backlog + 1`, re-propose slot `k` with
/// different content, confirm up to it. Timed from the re-proposal to the reply for `k`.
fn run_recovery_trial(
    h: &mut Harness,
    args: &Args,
    trial: usize,
    depth: usize,
    position: &str,
    backlog: usize,
    batch_size: usize,
    cost: Duration,
) -> Result<Outcome, TrialError> {
    let base = h.next_seq;
    let k = backtrack_slot(position, depth, backlog);
    assert!(
        k > backlog && k < depth,
        "slot {k} outside ({backlog}, {depth})"
    );
    let reproposal_all = args.reproposal == "all";
    let last = if reproposal_all { depth - 1 } else { k };

    let trial_seed = args.seed ^ ((trial as u64) << 32) ^ ((depth as u64) << 16) ^ (k as u64);
    let tag = |version: u8| ((trial as u8).wrapping_mul(2).wrapping_add(version) % 250) + 1;

    let original: Vec<Vec<CRUDRequest>> = (0..depth)
        .map(|s| make_ops(args, trial_seed + s as u64, tag(0), batch_size, cost))
        .collect();
    let replacement: Vec<Vec<CRUDRequest>> = (0..depth)
        .map(|s| {
            make_ops(
                args,
                trial_seed + 0x9E37_79B9 + s as u64,
                tag(1),
                batch_size,
                cost,
            )
        })
        .collect();

    // 1. speculate everything
    for (s, ops) in original.iter().enumerate() {
        h.spec(base + s as u32, ops)?;
    }

    // 2. barrier: FIFO, so this reply proves all `depth` batches were speculated
    h.fin(base)?;
    h.wait_for(&[base], batch_size)?;

    // 3. burst-confirm `backlog` batches without waiting
    for s in 1..=backlog {
        h.fin(base + s as u32)?;
    }

    if args.pre_trigger_wait_ms > 0 {
        std::thread::sleep(Duration::from_millis(args.pre_trigger_wait_ms));
    }

    // 4. backtrack
    let t0 = Instant::now();
    for s in k..=last {
        h.spec(base + s as u32, &replacement[s])?;
    }
    for s in (backlog + 1)..=last {
        h.fin(base + s as u32)?;
    }

    // 5. wait for every reply
    let wanted: Vec<u32> = (0..=last).map(|s| base + s as u32).collect();
    h.wait_for(&wanted, batch_size)?;

    let t_first = h.last_arrival(base + k as u32).unwrap();
    let t_full = h.last_arrival(base + last as u32).unwrap();

    // 6. verify against the reference model, outside the timed region
    let mut mismatches = 0;
    if !args.no_verify {
        for s in 0..=last {
            let ops = if s >= k {
                &replacement[s]
            } else {
                &original[s]
            };
            let mut got: Vec<&Reply> = h.buffered[&(base + s as u32)].iter().collect();
            got.sort_by_key(|r| r.op);

            for (i, op) in ops.iter().enumerate() {
                let expected = h.reference.apply(&request_type(op));
                if got.get(i).map(|r| &r.reply) != Some(&expected) {
                    mismatches += 1;
                }
            }
        }
    }

    // 7. settle, then catch replies from speculation that should have been discarded
    let settle_ms = 10.0 + 1.3 * (depth * batch_size) as f64 * cost.as_secs_f64() * 1e3;
    std::thread::sleep(Duration::from_micros((settle_ms * 1e3) as u64));
    h.drain_nonblocking();
    let stale_replies: usize = (last + 1..depth)
        .map(|s| h.buffered.get(&(base + s as u32)).map_or(0, |v| v.len()))
        .sum();

    h.buffered.clear();
    h.next_seq = base + (last as u32) + 1;

    Ok(Outcome {
        ttr_first_ms: t_first.duration_since(t0).as_secs_f64() * 1e3,
        ttr_full_ms: t_full.duration_since(t0).as_secs_f64() * 1e3,
        kept: k - backlog - 1,
        reexecuted_batches: last - k + 1,
        mismatches,
        stale_replies,
    })
}

struct SnapshotOutcome {
    ckpt_ms: f64,
    probe_ms: f64,
    mismatches: usize,
}

/// Snapshot cost, plus a probe batch queued behind it to show whether it stalls speculation.
fn run_snapshot_trial(
    h: &mut Harness,
    args: &Args,
    trial: usize,
    batch_size: usize,
    cost: Duration,
) -> Result<SnapshotOutcome, TrialError> {
    let base = h.next_seq;
    let seed = args.seed ^ ((trial as u64) << 32);
    let batches: Vec<Vec<CRUDRequest>> = (0..2)
        .map(|s| make_ops(args, seed + s, 1, batch_size, cost))
        .collect();
    let probe = make_ops(args, seed + 99, 2, 1, cost);

    h.spec(base, &batches[0])?;
    h.spec(base + 1, &batches[1])?;
    h.spec(base + 2, &probe)?;

    h.fin(base)?;
    h.wait_for(&[base], batch_size)?;

    let t0 = Instant::now();
    h.handle
        .queue_update_finalized_and_get_appstate(SeqNo::from(base + 1))
        .map_err(|e| TrialError::Send(e.to_string()))?;
    h.fin(base + 2)?;

    h.wait_for(&[base + 1], batch_size)?;
    h.wait_for(&[base + 2], 1)?;
    let probe_at = h.last_arrival(base + 2).unwrap();

    let ckpt = h
        .checkpoints
        .recv_timeout(h.timeout)
        .map_err(|_| TrialError::Timeout {
            missing: vec![base + 1],
            got: 0,
        })?;
    let ckpt_at = Instant::now();

    let mut mismatches = 0;
    if !args.no_verify {
        for (s, ops) in batches.iter().enumerate() {
            let mut got: Vec<&Reply> = h.buffered[&(base + s as u32)].iter().collect();
            got.sort_by_key(|r| r.op);
            for (i, op) in ops.iter().enumerate() {
                if got.get(i).map(|r| &r.reply) != Some(&h.reference.apply(&request_type(op))) {
                    mismatches += 1;
                }
            }
        }
        // The snapshot is after base+1: no probe, everything before it.
        let state = ckpt.state();
        if state.len() as i64 != h.reference.live {
            mismatches += 1;
        }
        for ops in &batches {
            for op in ops {
                let (CRUDRequestType::Read { key }
                | CRUDRequestType::Create { key, .. }
                | CRUDRequestType::Update { key, .. }
                | CRUDRequestType::Delete { key }) = request_type(op);
                if state.get(&key) != h.reference.current(index(&key)) {
                    mismatches += 1;
                    break;
                }
            }
        }
    }

    for op in &probe {
        h.reference.apply(&request_type(op));
    }

    let outcome = SnapshotOutcome {
        ckpt_ms: ckpt_at.duration_since(t0).as_secs_f64() * 1e3,
        probe_ms: probe_at.duration_since(t0).as_secs_f64() * 1e3,
        mismatches,
    };

    drop(ckpt);
    std::thread::sleep(Duration::from_millis(10));
    h.drain_nonblocking();
    h.buffered.clear();
    h.next_seq = base + 3;
    Ok(outcome)
}

// ── the two recovery primitives, in isolation ────────────────────────────────────────

/// `single_threaded_crud::caching_state::AccumulatedCache`.
type AccumulatedCache = atlas_common::collections::HashMap<
    String,
    atlas_common::collections::HashMap<Vec<u8>, Option<Vec<u8>>>,
>;

/// Copy of `merge_delta_into` / `rebuild_accumulated_cache` from `single_threaded_crud/
/// caching_state.rs` (`pub(crate)`), to time the rebuild alone. End-to-end trials are authoritative.
fn merge_delta_into(dst: &mut AccumulatedCache, src: &AccumulatedCache) {
    for (col, keys) in src {
        let col_map = dst.entry(col.clone()).or_default();
        for (key, val) in keys {
            col_map.insert(key.clone(), val.clone());
        }
    }
}

fn rebuild_accumulated_cache<'a>(
    deltas: impl Iterator<Item = &'a AccumulatedCache>,
) -> AccumulatedCache {
    let mut acc = AccumulatedCache::default();
    for delta in deltas {
        merge_delta_into(&mut acc, delta);
    }
    acc
}

/// A delta of `entries` writes over the key space (one in ten a tombstone).
fn make_delta(
    rng: &mut SplitMix64,
    keys: u64,
    entries: usize,
    value_bytes: usize,
) -> AccumulatedCache {
    let mut delta = AccumulatedCache::default();
    let col = delta.entry("microbenchmark".to_string()).or_default();
    for _ in 0..entries {
        let key = rng.gen_range(0..keys.max(1)).to_le_bytes().to_vec();
        let value = if rng.gen_range(0..10u8) == 0 {
            None
        } else {
            Some(vec![7u8; value_bytes])
        };
        col.insert(key, value);
    }
    delta
}

fn run_primitives(args: &Args) {
    let mut sink = Sink {
        file: args.out.as_ref().map(|p| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .expect("cannot open --out")
        }),
    };
    let common = format!(
        "\"scenario\":\"primitives\",\"executor\":\"primitives\",\"state_keys\":{},\"value_bytes\":{}",
        args.state_keys, args.value_bytes
    );

    // 1. State::clone: the floor under dual_state's recovery
    let state = State::preloaded_with(args.state_keys, args.value_bytes);
    for trial in 0..args.trials {
        let t = Instant::now();
        let copy = state.clone();
        let ms = t.elapsed().as_secs_f64() * 1e3;
        std::hint::black_box(&copy);
        drop(copy);
        sink.emit(format!(
            "{{{common},\"what\":\"state_clone\",\"trial\":{trial},\"status\":\"ok\",\"ms\":{ms:.4},\"entries\":{}}}",
            state.len()
        ));
    }
    // Clone of a clone: its values sit in iteration order, so it clones faster. The dual-state
    // confirmed state is a clone, so both numbers matter.
    let layout_ordered = state.clone();
    for trial in 0..args.trials {
        let t = Instant::now();
        let copy = layout_ordered.clone();
        let ms = t.elapsed().as_secs_f64() * 1e3;
        std::hint::black_box(&copy);
        drop(copy);
        sink.emit(format!(
            "{{{common},\"what\":\"state_clone_of_clone\",\"trial\":{trial},\"status\":\"ok\",\"ms\":{ms:.4},\"entries\":{}}}",
            layout_ordered.len()
        ));
    }
    drop(layout_ordered);
    drop(state);

    // 2. delta re-merge: the floor under the CRUD executors' recovery
    let writes = |batch: usize| (batch * args.write_pct as usize / 100).max(1);
    for &batch in &args.batch_sizes {
        for &depth in &args.depths {
            let mut rng =
                SplitMix64::seed_from_u64(args.seed ^ ((batch as u64) << 20) ^ depth as u64);
            let deltas: Vec<AccumulatedCache> = (0..depth)
                .map(|_| make_delta(&mut rng, args.state_keys, writes(batch), args.value_bytes))
                .collect();
            let entries: usize = deltas
                .iter()
                .map(|d| d.values().map(|c| c.len()).sum::<usize>())
                .sum();

            for trial in 0..args.trials {
                let t = Instant::now();
                let rebuilt = rebuild_accumulated_cache(deltas.iter());
                let ms = t.elapsed().as_secs_f64() * 1e3;
                std::hint::black_box(&rebuilt);
                drop(rebuilt);
                sink.emit(format!(
                    "{{{common},\"what\":\"cache_rebuild\",\"trial\":{trial},\"status\":\"ok\",\"ms\":{ms:.4},\
                     \"pending_batches\":{depth},\"batch_size\":{batch},\"pending_entries\":{entries}}}"
                ));
            }
        }
    }
}

// ── startup and driving ──────────────────────────────────────────────────────────────

fn start<E>(args: &Args) -> Harness
where
    E: TExecutor<Microbenchmark, State, ExecutionHandle = PreemptiveExecutorHandle<CRUDRequest>>
        + TMonolithicStateExecutor<Microbenchmark, State, RecorderNode>,
{
    let (tx, replies) = mpsc::channel();
    let node = Arc::new(RecorderNode { tx: Mutex::new(tx) });

    let t = Instant::now();
    let state = State::preloaded_with(args.state_keys, args.value_bytes);
    eprintln!(
        "built {}-key state ({} B values) in {:.2?}",
        args.state_keys,
        args.value_bytes,
        t.elapsed()
    );

    let handle = E::init_handle();
    let (_install_tx, checkpoints) = E::init(
        handle.clone(),
        Some((state, vec![])),
        Microbenchmark::new(NodeId::from(0u32)),
        node,
    )
    .expect("failed to start executor");

    Harness {
        handle,
        replies,
        checkpoints,
        buffered: HashMap::new(),
        reference: Reference::new(args.state_keys, args.value_bytes),
        next_seq: 0,
        timeout: Duration::from_secs(args.timeout_s),
    }
}

struct Sink {
    file: Option<std::fs::File>,
}

impl Sink {
    fn emit(&mut self, line: String) {
        match &mut self.file {
            Some(f) => {
                writeln!(f, "{line}").unwrap();
                f.flush().unwrap();
            }
            None => println!("{line}"),
        }
    }
}

fn common_fields(args: &Args) -> String {
    format!(
        "\"executor\":\"{}\",\"state_keys\":{},\"value_bytes\":{},\"write_pct\":{},\"pre_trigger_wait_ms\":{}",
        args.executor, args.state_keys, args.value_bytes, args.write_pct, args.pre_trigger_wait_ms
    )
}

fn run<E>(args: &Args)
where
    E: TExecutor<Microbenchmark, State, ExecutionHandle = PreemptiveExecutorHandle<CRUDRequest>>
        + TMonolithicStateExecutor<Microbenchmark, State, RecorderNode>,
{
    let mut sink = Sink {
        file: args.out.as_ref().map(|p| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .expect("cannot open --out")
        }),
    };
    let mut h = start::<E>(args);

    // Snapshot has no depth/position/backlog axes: it runs once per (cost, batch size).
    let mut combos = Vec::new();
    for &backlog in &args.backlogs {
        for &cost_us in &args.op_cost_us {
            for &batch in &args.batch_sizes {
                if args.scenario == "snapshot" {
                    if backlog == args.backlogs[0] {
                        combos.push((0, cost_us, batch, 0, "head".to_string()));
                    }
                    continue;
                }
                for &depth in &args.depths {
                    for position in &args.positions {
                        combos.push((backlog, cost_us, batch, depth, position.clone()));
                    }
                }
            }
        }
    }
    combos.sort_by_key(|c| c.0);
    let mut previous_work_ms = 0.0f64;

    for (backlog, cost_us, batch, depth, position) in combos {
        let cost = Duration::from_micros(cost_us);
        let cfg = format!(
            "{},\"scenario\":\"{}\",\"depth\":{depth},\"position\":\"{position}\",\"backlog\":{backlog},\
             \"batch_size\":{batch},\"op_cost_us\":{cost_us},\"reproposal\":\"{}\"",
            common_fields(args),
            args.scenario,
            args.reproposal
        );

        // Skip combinations with nothing left to backtrack over.
        if args.scenario == "recovery"
            && !(depth >= backlog + 2 && backtrack_slot(&position, depth, backlog) < depth)
        {
            continue;
        }

        let pause_ms = (2.0 * previous_work_ms).max(args.config_pause_ms as f64);
        std::thread::sleep(Duration::from_micros((pause_ms * 1e3) as u64));
        h.drain_nonblocking();
        h.buffered.clear();
        previous_work_ms = (depth * batch) as f64 * cost_us as f64 / 1e3;

        eprintln!(
            "[{}] {} keys={} depth={depth} pos={position} backlog={backlog} batch={batch} cost={cost_us}us",
            args.executor, args.scenario, args.state_keys
        );

        // A silent executor is dead, so replace it; abandon a configuration after 3 wedges in a row.
        let mut consecutive_wedges = 0;

        for trial in 0..args.trials {
            let (record, wedged) = if args.scenario == "snapshot" {
                match run_snapshot_trial(&mut h, args, trial, batch, cost) {
                    Ok(o) => (
                        format!(
                            "{{{cfg},\"trial\":{trial},\"status\":\"{}\",\"ckpt_ms\":{:.4},\"probe_ms\":{:.4},\"mismatches\":{}}}",
                            if o.mismatches == 0 { "ok" } else { "mismatch" },
                            o.ckpt_ms,
                            o.probe_ms,
                            o.mismatches
                        ),
                        false,
                    ),
                    Err(e) => (
                        format!(
                            "{{{cfg},\"trial\":{trial},\"status\":\"timeout\",\"detail\":\"{}\"}}",
                            format!("{e:?}").replace('"', "'")
                        ),
                        true,
                    ),
                }
            } else {
                match run_recovery_trial(
                    &mut h, args, trial, depth, &position, backlog, batch, cost,
                ) {
                    Ok(o) => (
                        format!(
                            "{{{cfg},\"trial\":{trial},\"status\":\"{}\",\"ttr_first_ms\":{:.4},\"ttr_full_ms\":{:.4},\
                             \"kept_batches\":{},\"reexecuted_batches\":{},\"mismatches\":{},\"stale_replies\":{}}}",
                            if o.mismatches == 0 && o.stale_replies == 0 {
                                "ok"
                            } else {
                                "mismatch"
                            },
                            o.ttr_first_ms,
                            o.ttr_full_ms,
                            o.kept,
                            o.reexecuted_batches,
                            o.mismatches,
                            o.stale_replies
                        ),
                        false,
                    ),
                    Err(e) => (
                        format!(
                            "{{{cfg},\"trial\":{trial},\"status\":\"timeout\",\"detail\":\"{}\"}}",
                            format!("{e:?}").replace('"', "'")
                        ),
                        true,
                    ),
                }
            };

            sink.emit(record);

            if wedged {
                consecutive_wedges += 1;
                eprintln!("  executor stopped answering (trial {trial}); replacing it");
                if consecutive_wedges >= 3 {
                    eprintln!("  3 wedges in a row; abandoning this configuration");
                    h = start::<E>(args);
                    break;
                }
                h = start::<E>(args);
            } else {
                consecutive_wedges = 0;
            }
        }
    }
}

fn main() {
    let args = Args::parse();

    // RUST_LOG=error shows why an executor stopped answering.
    if std::env::var_os("RUST_LOG").is_some() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .init();
    }

    // The replier sends via the shared thread pool.
    let _guard = unsafe {
        init(InitConfig {
            threadpool_threads: 4,
            async_threads: 1,
        })
        .expect("failed to initialise the Atlas runtime")
    };

    if args.scenario == "primitives" {
        run_primitives(&args);
        std::process::exit(0);
    }

    match args.executor.as_str() {
        "dual_state" => run::<MonolithicPreemptiveExecutor>(&args),
        "crud_single" => run::<CRUDMonolithicPreemptiveExecutor>(&args),
        "crud_scalable" => run::<ScalableCRUDMonolithicPreemptiveExecutor>(&args),
        other => {
            eprintln!("--executor must be dual_state, crud_single or crud_scalable, got {other:?}");
            std::process::exit(2);
        }
    }

    // No executor shutdown exists.
    std::process::exit(0);
}
