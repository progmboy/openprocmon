# SDK performance baseline

Tracked results of `cargo bench -p procmon-sdk --bench baseline` (full mode,
release). Re-run after each hot-path optimization and append a row-set here, so
CPU and memory effects stay quantified. Numbers are machine-dependent —
compare runs from the same machine only.

Fixture: local `tests/resources/Logfile.PML` (34 MB, 92,598 events, not
committed). The `live/ingest` phase is synthetic (256 batches / ~29 MB /
127,489 events) and needs no fixture.

## 2026-06-10 — pre-refactor baseline (commit b9f8ce8 + bench)

```
phase             med(ms)    min(ms)        kev/s         allocs    allocMB     retainMB    peakMB
live/ingest          33.8       33.1         3777        148,772       45.8          0.0      36.8
pml/open              1.6        1.4            -         13,895        1.3          0.6       0.7
pml/parse            40.1       37.8         2308        214,360       70.5         44.2      44.2
pml/columns          63.0       61.6         1469        429,376       16.5          0.0       0.0
pml/filter           82.6       82.1         1121      1,110,660       27.2          0.0       0.0
```

Reading of the baseline (what the planned optimizations should move):

- `live/ingest` — 148,772 allocs for 127,489 events ≈ **1.2 allocs/event**:
  the per-record `Box<[u8]>` copy in `Correlator::ingest` plus per-batch
  buffers. The Arc-shared-batch refactor (#1/#2) should drop this to roughly
  the batch count (hundreds), and `allocMB` from 45.8 toward the raw batch
  bytes (~29 MB).
- `pml/filter` — 1,110,660 allocs for 92,598 events × 4 rules ≈ **3 allocs
  per rule evaluation** (`column_value` String + two `to_ascii_lowercase`).
  Allocation-free case-insensitive matching (#3) should bring this near zero
  and is expected to dominate the CPU win (slowest phase, 1121 kev/s).
- `pml/parse` — 2.3 allocs/event, 44.2 MB retained for 92,598 events
  (~500 B/event): per-event synth record + `Event` boxes.
- `pml/columns` — 4.6 allocs/event from path/detail string assembly
  (`decode_utf16`'s double allocation, #5).

## 2026-06-10 — after zero-copy ingest (#1 + #2: Arc-shared batches, Record)

Events hold a `Record` (offset into the `Arc`-shared batch / synthesized
buffer); the receive loop hands its buffer to the parser whole; the PML reader
synthesizes records directly into `Arc<[u8]>` (one allocation per record).

```
phase             med(ms)    min(ms)        kev/s         allocs    allocMB     retainMB    peakMB
live/ingest          10.8       10.4        11757             35       22.0          0.0      11.0
pml/open              1.4        1.3            -         13,895        1.3          0.6       0.7
pml/parse            34.2       33.6         2704        214,360       76.9       48.5       48.5
pml/columns          50.2       49.0         1844        429,376       16.5          0.0       0.0
pml/filter           71.3       70.6         1298      1,110,660       27.2          0.0       0.0
```

vs the pre-refactor baseline:

- `live/ingest`: **3.1× faster** (33.8 → 10.8 ms; 3,777 → 11,757 kev/s) and
  **148,772 → 35 allocations** (~0 per event — the remaining 35 are HashMap/Vec
  growth). Peak memory 36.8 → 11.0 MB.
- `pml/parse`: 40.1 → 34.2 ms (**15% faster**), same allocation count (1/record,
  now written straight into the `Arc`). `retainMB` 44.2 → 48.5 (+4.3 MB): each
  record now carries a 16-byte `Arc` refcount header (~190k records) — the
  accepted cost of the shared-buffer design.
- `pml/columns` / `pml/filter`: unchanged within noise (not targeted yet —
  next: allocation-free filter evaluation, single-allocation UTF-16 decode).

## 2026-06-10 — after allocation-free filter evaluation (#3)

`relation_matches` compares ASCII-case-insensitively in place (no
`to_ascii_lowercase` copies); `FilterFields`/`column_value` return
`Cow<'_, str>` so string-backed columns (operation, process name, image path,
result, …) are borrowed, not allocated.

```
phase             med(ms)    min(ms)        kev/s         allocs    allocMB     retainMB    peakMB
live/ingest          11.1       10.4        11478             35       22.0          0.0      11.0
pml/open              1.4        1.3            -         13,895        1.3          0.6       0.7
pml/parse            34.9       34.0         2654        214,360       76.9       48.5       48.5
pml/columns          49.1       47.4         1885        429,376       16.5          0.0       0.0
pml/filter           28.9       28.1         3207        185,024        9.6          0.0       0.0
```

vs the previous run:

- `pml/filter`: **2.9× faster** (82.6 → 28.9 ms vs the original baseline;
  1,121 → 3,207 kev/s) and **1,110,660 → 185,024 allocations** (−83%).
- The remaining ~2 allocs/event are the two `Path` rules in the bench set —
  `Event::path()` derives the string per evaluation. A follow-up could
  evaluate each referenced column once per `matches()` call (the GUI's
  Advanced Display set has ~13 Path rules, so it would gain the most).
- Other phases unchanged within noise.

