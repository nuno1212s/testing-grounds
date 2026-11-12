# Part 1 results: speculative against normal execution

Measured 2026-11-02 on the local Docker + netem WAN setup (4 replicas, f = 1, closed-loop clients,
`FORCE_ORDERED`, cores pinned, `spin` processing, in-process latency histograms). Raw runs and
reports are in `results/` (not committed): `e1_short`, `e1_heavy`, `e1_smallstate`, and
`e1_short.saturated`, the superseded first attempt. Plans are in `plans/`. These are **short**
runs (5 s warm-up, 20-30 s windows, 2 repetitions) meant for a first set of numbers; the full E1-E4
(about 25 h) were not run.

## Result

At the median, speculation hides exactly the request's execution time, up to one replica round
trip: saving = min(execution time, RTT). Nothing more, nothing less.

| RTT (ms) | cost (ms) | model (ms) | measured p50 saving (ms) | measured mean saving, small state (ms) |
|---|---|---|---|---|
| 50 | 0 | 0 | 0.00 | 0.00-0.03 |
| 50 | 1 | 1 | 1.84-2.10 (one 2.1 ms bucket) | 0.99-1.11 |
| 50 | 5 | 5 | 5.24 | 4.30-4.83 |
| 5 | 20 | 5 | 5.24 | (1M-key state; see caveat) |
| 50 | 20 | 20 | 19.92 | (1M-key state; see caveat) |
| 200 | 20 | 20 | 16.78 (bucket is 8.4 ms wide) | (1M-key state; see caveat) |

All three speculative variants give the same median saving. The relative gain is therefore small
unless the request cost is comparable to the RTT: about 30% at 20 ms cost and 50 ms RTT, 6% at 5 ms
cost, nothing at zero cost.

## Caveat that changes how the tails and means must be read

With the default 1M-key state, `baseline`, `crud_single` and `crud_scalable` have a p99 of about
230 ms against a p50 of 77 ms (RTT 50 ms), and `dual_state` does not. The cause is a snapshot stall
of about 150 ms at 1M keys (see Part 2, R5): the single executor worker blocks while the state is
copied, and `dual_state` copies from its second state without blocking. It inflates the mean too:
`dual_state` appeared to save 10-15 ms of mean latency even at zero cost. With 1,000 preloaded keys
(`e1_smallstate`) every p99 is within 2-3 ms of the p50 and the mean savings land on the model
(ratio 0.86-1.11). So state size, not speculation, explains the tail gap. Quote means and p99 from
the small-state runs, or state the state size next to every tail figure.

## Other things to know

- `crud_scalable` is faster than the others when several requests are in flight (up to 43% at RTT
  5 ms, cost 5 ms) because it executes a batch 4 wide, not because it speculates better.
- With 15 requests in flight a 20 ms executor is saturated (p50 about 15 x 20 ms) and the queue
  hides the saving (`e1_short.saturated`). The analysis now warns when an executor is more than 50%
  busy and more than one request can be in flight.
- One run (`crud_scalable`, RTT 5 ms, cost 5 ms) was excluded because a client started 11 s late,
  so that cell has n = 1 for that variant.
- p50 is quantized to buckets of up to 1.6% (2.1 ms at 77 ms, 8.4 ms at 300 ms); the mean is exact.
  p99 with fewer than 1000 samples is marked and unreliable, which includes every RTT 200 cell.
- Only 2 repetitions: intervals are Student-t with n = 2 (12.7 standard errors) and are wide.
