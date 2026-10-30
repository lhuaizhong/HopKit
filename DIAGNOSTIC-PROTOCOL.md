# Diagnostic result contract

Applies to the stage-two Ping, TCPing and Traceroute repairs. GUI and CLI call the same Rust `run` and `envelope` functions. Execution status (`succeeded`, `cancelled`, `failed`) is separate from diagnostic observations (`data.success` / Trace `data.reached`). A completed task may have observed no successful connection. A stopped task may contain successful measurements.

## Envelope and compatibility

The envelope contains `schema_version`, `task_id`, `command`, `status`, `data`, `error`. `error` remains `{code,message}` or null. Probe errors and cancellation now carry accumulated `data` instead of dropping it. CLI cancellation remains exit **130**; non-cancelled diagnostic failure remains exit 1. Invalid arguments remain exit 2.

Ping/TCPing retain schema version 1 with additive fields. **Trace uses schema version 2** in final envelopes and CLI JSONL events, because the incorrect cross-hop `received/lost/loss_percent/rtt_*` summary was removed. Consumers should branch on `command` and version. `--strict` checks Ping/TCPing failures or Trace timeout probes; normal TTL-expired responses do not cause strict failure.

CLI final output also includes the sanitized `request`. GUI task snapshots contain this request, `latest_stats`, a bounded event queue and, when finished, `result` (the shared envelope). Summary listing omits result/stat payloads. `task_poll` returns `latest_stats` even when no new events exist after its cursor. Request snapshots are created before execution and omit `ftp_password` via Rust serialization; credentials remain only in the worker request while required for execution.

## Counting and time

| Field | Meaning |
| --- | --- |
| `sent` | Probe calls attempted, including an interrupted in-flight call; not proof a packet left the host |
| `completed` | Calls that produced a complete observation before cancellation |
| `interrupted_probes` | Calls aborted by cancellation or adapter error before a complete observation |
| `has_samples` | At least one completed observation exists |
| `failure_counts` | Completed failures grouped by status (`timeout`, `refused`, `connect_error`, etc.); Trace TTL expiration is not a failure |
| `started_at_ms`, `ended_at_ms` | Unix wall-clock milliseconds; end exists only in the final result |
| `duration_ms` | Monotonic elapsed duration, including resolution and waits |
| `statistics_scope` | `entire_task` |

For Ping/TCPing, `received` counts Echo Replies/successful TCP connections. `lost = completed - received`, and `loss_percent = lost / completed * 100`. With no completed observation, percentage and RTT metrics are null. `rtt_min_ms/rtt_avg_ms/rtt_max_ms` include successful measurements only (TCP connect time for TCPing). In-flight cancellation is never converted into a timeout or zero-latency measurement. Synchronous TCP/Windows ICMP calls may take up to their configured timeout to return after Stop; they are then classified as interrupted. macOS ICMP checks cancellation during short reads.

## Trace and samples

Trace exposes `hops_probed`, `reached`, `timeout_probes`, `duration_ms` and `success = reached`, with no cross-hop loss rate. Each sample retains `hop`, `seq`, `address`, `rtt_ms` and `status`. `ok` retains its original meaning of a destination Echo Reply. `responded` distinguishes an actual hop response (including TTL expiry/unreachable) from timeout; `reached` distinguishes a destination reply. The loop finishes all three probes in the destination hop. A timeout is an observation, not a conclusion about link failure.

Sample `timestamp_ms` marks call start in wall time; `elapsed_ms` uses the task's monotonic clock for chart positioning. A UI chart covers recent available samples, uses a linear auto-maximum Y axis and ms ticks, and places failure markers separately from latency. A successful zero RTT is valid; a minimum one-pixel bar is used for visibility. Hover retains the exact value. Filtering changes the chart and table, never cumulative metrics.

## Retention and export

- Core final samples: first 10,000 completed observations; `samples_truncated` signals later observations omitted, and `sample_retention` documents the policy. Cumulative counts continue beyond the cap.
- Trace is bounded by 64 hops × 3 probes; snapshots include every available hop probe independently of the event queue.
- Backend queue: 150 events per task; GUI preview: 300 events; chart: at most 200 samples. `latest_stats` is stored separately from the queue. Each completed probe emits a cumulative stats snapshot.
- Desktop: at most four running tasks and 40 retained session tasks; only finished tasks can be evicted. The UI shows all retained tasks and pins active tasks even when history is filtered.
- GUI JSON contains the task snapshot, bounded preview, `preview_scope` and current filter/window metadata. PNG captures cumulative metrics and the current filtered recent view at click time. Samples are partial when capped; this is not a full-history archive. Disk-backed history is a later stage.
