# SDK performance baseline

Tracked results of `cargo bench -p procmon-sdk --bench baseline` (full mode,
release). Re-run after each hot-path optimization and append a row-set here, so
CPU and memory effects stay quantified. Numbers are machine-dependent —
compare runs from the same machine only.

Fixture: `tests/resources/CompressedLogFileBench64PML` (zlib-compressed PML,
34 MB / 92,598 events unpacked; committed). The `live/ingest` phase is
synthetic (256 batches / ~29 MB / 127,489 events) and needs no fixture.

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

## 2026-06-10 — after streaming UTF-16 decode (#5) + new live/columns phase

`decode_utf16` streams units through `char::decode_utf16` into a
pre-sized `String` — the old intermediate `Vec<u16>` (whose `take_while`
erased the size hint, forcing doubling reallocs per string) is gone. The
bench gains a **live/columns** phase: column extraction over live-mode
events, which (unlike the mostly-ASCII PML strings) exercises this wire
decode plus `nt_to_dos`.

```
phase             med(ms)    min(ms)        kev/s         allocs    allocMB     retainMB    peakMB
live/ingest          11.9       11.4        10718             24       22.0          0.0      11.0
live/columns         94.4       90.2         1351        956,172       28.2          0.0       0.0
pml/open              0.7        0.6            -          3,888        0.8          0.6       0.7
pml/parse            37.7       34.9         2454        214,360       76.9       48.5       48.5
pml/columns          49.4       48.6         1875        429,376       16.5          0.0       0.0
pml/filter           31.2       29.2         2972        185,024        9.6          0.0       0.0
```

A/B for this change alone (same bench, decode change stashed):

- `live/columns`: 141.9 → 94.4 ms (**1.5×**), 1,753,378 → 956,172 allocs
  (−45%), 70.1 → 28.2 MB allocated (−60%).
- `pml/open`: 1.5 → 0.7 ms, 13,895 → 3,888 allocs (the PML strings table
  decodes through the same helper).
- `pml/columns` unchanged: PML detail strings carry the ASCII flag and
  never hit the UTF-16 decode; their remaining allocations are the
  `format!` detail composition.

## 2026-06-10 — after process-lookup tuning (#7)

The per-event hot maps (`ProcessManager::{by_seq,by_pid}`,
`Correlator::pending`) use `FxHashMap` (kernel-assigned integer keys need no
SipHash flooding resistance), and `Correlator` keeps a single-entry
`(process_seq, Arc<ProcessRecord>)` cache — consecutive events from the same
process skip the table's lock + hash entirely (positive hits only, so a
late-tracked process is never masked).

```
phase             med(ms)    min(ms)        kev/s         allocs    allocMB     retainMB    peakMB
live/ingest           8.5        8.3        14928             24       22.0          0.0      11.0
live/columns         91.2       87.8         1399        956,172       28.2          0.0       0.0
pml/open              0.7        0.6            -          3,888        0.8          0.6       0.7
pml/parse            35.6       35.2         2603        214,360       76.9       48.5       48.5
pml/columns          51.3       46.9         1806        429,376       16.5          0.0       0.0
pml/filter           27.4       27.1         3379        185,024        9.6          0.0       0.0
```

- `live/ingest`: 11.9 → 8.5 ms (**1.4×**; cumulatively 33.8 → 8.5 ms = **4×**
  over the original baseline). Caveat: the synthetic batches use a single
  process, so the cache hit rate here is ~100% — real captures interleave
  processes and will see less, though bursts dominate in practice.
- Other phases unchanged within noise.

## 2026-06-10 — after per-evaluation column memo

`matches()`/`highlights()` keep a per-call [`ColumnMemo`]: each referenced
column is materialized at most once per event evaluation, however many rules
target it and in whatever order. Rule evaluation order and the exclude
short-circuit are unchanged, so columns past an early exclude hit are still
never derived.

```
phase             med(ms)    min(ms)        kev/s         allocs    allocMB     retainMB    peakMB
live/ingest           9.4        8.5        13563             24       22.0          0.0      11.0
live/columns         94.6       88.4         1348        956,172       28.2          0.0       0.0
pml/open              0.7        0.6            -          3,888        0.8          0.6       0.7
pml/parse            37.2       35.3         2491        214,360       76.9       48.5       48.5
pml/columns          49.5       48.3         1872        429,376       16.5          0.0       0.0
pml/filter           20.0       19.8         4629         92,512        4.8          0.0       0.0
```

- `pml/filter`: 27.4 → 20.0 ms, allocations exactly halved (185,024 →
  92,512 — the bench set's two `Path` rules now derive the path once).
  Cumulatively vs the original baseline: **82.6 → 20.0 ms (4.1×), 1,110,660 →
  92,512 allocations (−92%)**. Real-world GUI sets gain more: Procmon's
  default noise filter has 13 `Path` rules.
- Other phases unchanged within noise.

## 2026-07-08 — after mmap-borrowed PML records

`PmlReader::event_as_event` no longer copies each event's header + stack +
detail into a synthesized `Arc<[u8]>`. A `Record::PmlBorrowed` synthesizes the
52-byte `LogEntry` head and points `frames()`/`data()` straight into the
reader's `Arc<Mmap>` (the PML body layout `[stack][detail]` physically matches
a kernel record's `[frames][data]`; x64-only, and 32-bit PMLs are rejected at
open). The variant is **boxed**: an inline 52-byte head doubled `Record`'s
size and slowed **live** ingest ~45% (bigger moves through the scratch vec /
reorder heap / channel), so PML pays one small box per record instead — the
measured trade (inline: parse 15.4 ms / 16 allocs but live 14.2 ms; boxed:
parse 24.5 ms / 1.3 allocs/event and live back at baseline).

```
phase             med(ms)    min(ms)        kev/s         allocs    allocMB     retainMB    peakMB
live/ingest          10.8        8.9        11843             24       22.0          0.0      11.0
live/columns        105.2       89.3         1211        956,172       28.2          0.0       0.0
pml/open              0.8        0.7            -          3,889        0.8          0.6       0.7
pml/parse            24.5       18.4         3772        122,190       29.5       18.5       18.5
pml/columns          51.9       46.4         1782        429,376       16.5        0.0        0.0
pml/filter           19.6       18.5         4729         92,512        4.8        0.0        0.0
```

- `pml/parse`: 37.2 → 24.5 ms (**1.5×**), **214,360 → 122,190 allocations**
  (2.3 → 1.3 per event: the record-payload copies are gone; what remains is
  one `Box<PmlRec>` per PRE/POST record plus the events vec), allocated bytes
  76.9 → 29.5 MB, **retained 48.5 → 18.5 MB** (nothing of the payload is
  duplicated any more — pages stay in the OS-managed mmap).
- Consequence of borrowing: every `Event` produced from a PML pins the
  reader's mmap — the `.PML` file stays open (Windows: locked against
  delete/truncate) until the last row is dropped.
- `live/*` unchanged within noise (the boxed variant keeps `Record` at its
  previous size).

