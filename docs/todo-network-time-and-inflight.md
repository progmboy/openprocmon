# TODO: Network-time normalization + optional Procmon-style in-flight display

> Design/plan doc for a future change. Not yet implemented. Tracks two items:
> a PML fidelity fix (network event time) and a GUI UX feature (show in-flight
> operations like Process Monitor). Driver-event time ordering is already handled
> by the adaptive stable sort in `PmlWriter::to_bytes` (commit `dbb0cd5`).

## Why

1. **Network events have wrong timestamps in the PML (fidelity bug).**
   `NetworkEvent.time` holds the ETW **QPC** value, which is written into the PML
   as if it were a FILETIME (`date_filetime = ev.time_raw()`). QPC and the
   driver's FILETIME differ in both origin and magnitude, so network rows get
   wrong timestamps and sort out of order relative to driver events → in stock
   Process Monitor the network rows show up blank / misplaced. After normalizing
   to FILETIME, the existing save-time sort orders network events correctly — no
   writer change needed.
2. **The live GUI does not show in-flight operations (UX).** Procmon shows a row
   the moment an operation starts (PRE) with an empty result, then fills the
   result in place when it completes (POST) — IDA `sub_14002E250` inserts at PRE,
   `sub_14002F520` registers the pending op by sequence, POST updates it in place.

## Approach (decisions locked)

- **Opt-in switch for consumers.** By default the SDK emits only complete events
  (no two-phase burden on consumers). Only a consumer that wants in-flight display
  (the GUI) passes `emit_pending=true`; then the SDK emits an
  `is_complete()==false` pending event at PRE and the complete event at POST,
  which the GUI fills by `sequence`.
- **Writer unchanged.** Keep `Vec<PmlEvent>` + the adaptive stable sort in
  `to_bytes` (≈O(n) on the near-sorted input, one pass, robust to ETW's bursty/
  late network delivery; no tree of any kind).
- **Stay zero-copy.** The PRE/ingest main path is untouched. `Event` holds
  `Record { buf: Arc<[u8]>, off }` offsets into Arc-shared batches; pending events
  carry the PRE Record, complete events carry PRE+POST Records, all zero-copy.

## Current architecture (reference)

- **Correlator** (`crates/sdk/src/parse/mod.rs`): a `STATUS_PENDING` PRE is stored
  in `pending: FxHashMap<i32, Record>`; the POST record is `remove`d and merged
  via `emit(pre, Some(post), proc)` into one complete event; sync ops emit
  immediately; POST records are consumed only internally today.
- **Event** (`crates/sdk/src/event.rs`): `Backing::KernelRecord { pre, post:
  Option<Record>, mode }`, no interior mutability, zero-copy. `time_raw()=pre.time`,
  `status_raw()` prefers post else pre, `sequence()` from pre. `STATUS_PENDING`
  (0x103) is the status of an async PRE record.
- **Channel**: parse thread → `Receiver<Event>`; consumers: GUI (`gui/src/app.rs`
  frame-timer drain), the core capture relay, the example crate. Live source built
  in `EventSource::from_driver` / `Pipeline::start`.
- **PmlWriter** (`crates/sdk/src/pml/writer.rs`): `events: Vec<PmlEvent>`;
  `push_event` copies into a self-contained `PmlEvent`; `to_bytes` stable-sorts by
  `date_filetime`. For a network event it writes `date_filetime = ev.time_raw()`.
- **GUI EventBuffer** (`crates/gui/src/model/buffer.rs`): `all: Vec<CapturedEvent>`
  (append) + `view: Vec<usize>` (indices into `all`); no sequence→row index; the
  event_table is a virtualized DataTable rendering only visible rows.
  `CapturedEvent::from_event` (`domain.rs`) snapshots `result`/`result_kind` at
  insert.
- **Network**: `pipeline.rs`'s `select!` emits via `emit_network` in arrival order
  with `NetworkEvent.time`. `network.rs:218` uses `ClientContext=1` (QPC). The PML
  reader already treats network time as filetime on read-back (`reader.rs:229`).

## TODO

### P0 — Network time QPC→FILETIME normalization (PML fidelity fix)
- [ ] In `crates/sdk/src/network.rs`, normalize `NetworkEvent.time` to FILETIME.
      Preferred: change ETW `ClientContext` from `1` (QPC) to `2` (system time /
      FILETIME) at `network.rs:218` so `EventHeader.TimeStamp` arrives as FILETIME.
- [ ] Fallback if that mode is unreliable: `QueryPerformanceFrequency` + a
      one-time QPC↔FILETIME anchor conversion.
- [ ] Verify the classic kernel TcpIp/UdpIp events actually deliver FILETIME under
      `ClientContext=2`.
- [ ] **Verify:** mixed file+network capture → network timestamps correct, ordered
      with driver events, 0 inversions; open in real Procmon → network rows not
      blank. (No writer/GUI change required for this item.)

### P1 — SDK opt-in pending-emit mode
- [ ] `crates/sdk/src/event.rs`: add `Event::is_complete(&self) -> bool =
      status_raw() != STATUS_PENDING` (no new field, no constructor changes —
      pending event status is always PENDING → false; sync/async-complete/network
      → true). Event stays immutable and zero-copy.
- [ ] Add `emit_pending: bool` (default `false`) to the live source /
      `Pipeline::start`; thread it through `crates/sdk/src/pipeline.rs`.
- [ ] `crates/sdk/src/parse/mod.rs` Correlator:
  - Default (`emit_pending=false`): unchanged — hold PRE, emit one complete event
    at POST. Every event `is_complete()==true`. Channel contract unchanged.
  - Opt-in (`emit_pending=true`): on async PRE (`STATUS_PENDING`) emit a pending
    event `from_records(pre, None, proc)` (`is_complete()==false`) and keep PRE in
    `pending`; on POST emit the complete event `from_records(pre, Some(post),
    proc)`. Sync/network emit a single complete event. Same `sequence` links the
    two.
- [ ] Channel stays `Receiver<Event>` (no new enum).
- [ ] **Verify:** unit test — with `emit_pending=true`, an async PRE/POST yields a
      pending (`!is_complete`) then a complete (`is_complete`) with the same
      sequence; default mode still emits a single complete; all existing SDK tests
      pass.

### P2 — GUI Procmon-style in-flight display
- [ ] GUI source uses `emit_pending=true`. In `crates/gui/src/model/sdk_source.rs`:
      `!ev.is_complete()` → `SourceEvent::Row` (append in-flight row, empty result)
      + record in a pending index by `sequence`; `ev.is_complete()` → if sequence
      is pending → `SourceEvent::Update`, else a normal new row (sync/network).
- [ ] `crates/gui/src/model/buffer.rs`: add `pending: HashMap<i32, usize>`
      (sequence→`all` index, in-flight rows only). `apply_update(seq, ...)` does an
      O(1) lookup of that one row and updates only its `result`/`result_kind`/
      `duration`, re-evaluates only that one row's filter membership (O(1)
      add/remove from `view`), then removes it from the pending index.
      **Never iterate all entries or rebuild the whole view.**
- [ ] Refresh only visible rows: the virtualized event_table already renders only
      visible rows; an update mutates row data + marks dirty, so GPUI repaints only
      visible rows next frame. Off-screen rows update data without rendering.
- [ ] `crates/gui/src/model/domain.rs`: make `CapturedEvent`'s `result`/
      `result_kind`/`duration` refreshable (the current `OnceCell`/snapshot →
      refreshable; empty/"…" while pending, recomputed on completion).
- [ ] `crates/gui/src/app.rs`: drain handles `Update`.
- [ ] Edge case: a pending row trimmed out of the retained ring before completion
      → the completion's sequence isn't in the index → treat as a new row (rare in
      a live capture; document it).
- [ ] **Verify:** live-capture notepad / a slow op → the in-flight row appears with
      no result, filled on completion; under high completion rates no full-table
      refresh / stutter (only visible rows repaint); filter/search reflect the
      filled result; a GUI-saved PML is still time-ordered.

### Unchanged (do NOT touch)
- `crates/sdk/src/pml/writer.rs` — keep `Vec` + adaptive sort.
- `crates/core/src/capture.rs` — uses default mode (`emit_pending=false`).
- `crates/example/*` — default mode.

## Throughout
- [ ] `cargo bench -p procmon-sdk --bench baseline` — `live/ingest` allocations /
      retained must not regress.
- [ ] `cargo test --workspace` + `cargo clippy --workspace --all-targets -D
      warnings` all green.
- [ ] Open artifacts in real Procmon: no blank rows (including network), Properties
      dialog does not crash.

## "Harder than it looks"
1. **GUI fill touches only visible rows** — `apply_update` must be O(1) (seq→row,
   update only that row, re-evaluate only that row's filter membership); no
   full-table refresh / view rebuild.
2. **CapturedEvent refreshable** — `result` is a snapshot/`OnceCell`; make it
   refreshable; if a filter depends on result, re-evaluate only that row's view
   membership after filling.
3. **Network cross-clock** — QPC ≠ FILETIME; verify `ClientContext=2` delivers
   FILETIME for classic kernel TcpIp/UdpIp; otherwise use the anchor conversion.
4. **`is_complete()` semantics** — derived from `status_raw() != STATUS_PENDING`;
   confirm the default mode never emits a PENDING-status event and the opt-in
   pending event's status is always PENDING.
5. **Two-phase is opt-in** — default consumers (writer/example) have zero burden;
   only the GUI (which passes `emit_pending=true`) handles pending + complete.
6. **True streaming-to-disk not done** — still "accumulate then write"; deferred.
