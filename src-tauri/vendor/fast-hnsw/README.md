# hnsw

A pure-Rust, dependency-free implementation of **Hierarchical Navigable Small World** (HNSW) approximate nearest-neighbour (ANN) search.

> Malkov & Yashunin, *"Efficient and robust approximate nearest neighbor search using
> Hierarchical Navigable Small World graphs"*, IEEE TPAMI 2018.

---

## Features

- **Pure Rust** — zero C/C++ dependencies (includes `memmap2`, also pure Rust)
- **Full algorithmic fidelity** — heuristic (Algorithm 4) and simple (Algorithm 3) neighbour selection; `extendCandidates`; `keepPrunedConnections`
- **Two pruning strategies** — `PruneStrategy::Simple` (default, fastest) and `PruneStrategy::Heuristic` (full Algorithm 4 for all edges, opt-in)
- **Five built-in distance metrics** — Euclidean, Squared Euclidean, Cosine, Dot-product, Manhattan; add your own with a one-method trait
- **Persistence** — binary file format; `save` / `load` / `load_mmap`; vector section sits at a fixed offset so it can be memory-mapped as a `&[f32]` slice
- **Labeled index** — `LabeledIndex<D, L>` attaches a typed `Payload` to every vector (class label, text tag, secondary embedding, custom struct)
- **Paired index** — `PairedIndex<A, B>` builds two independent HNSW graphs over the same items (text+image, query+doc); search from either side, retrieve both embeddings per result
- **Custom payload** — implement two methods (`encode` / `decode`) to persist any type; fixed-stride types use a flat layout, variable-width types get an offset table
- **Capacity hint** — pre-allocate for expected index size to minimise reallocation churn
- **Ergonomic builder** — `.build(metric)` / `.build_labeled(metric)` / `.build_paired(ma, mb)`
- **Reproducible** — optional fixed RNG seed
- **Tested** — 41 unit tests + 14 doc-tests including recall regression, persistence round-trips, mmap loads, and paired-search correctness

---

## Quick start

```toml
[dependencies]
hnsw = { path = "." }
```

```rust
use hnsw::{Builder, Hnsw, SearchResult};
use hnsw::distance::Euclidean;

fn main() {
    let mut index: Hnsw<Euclidean> = Builder::new()
        .m(16)
        .ef_construction(200)
        .capacity(10_000)
        .seed(42)
        .build(Euclidean);

    index.insert(vec![1.0, 0.0]);
    index.insert(vec![0.0, 1.0]);
    index.insert(vec![0.5, 0.5]);

    let results: Vec<SearchResult> = index.search(&[0.9, 0.1], 1, 20);
    println!("nearest id={} distance={:.4}", results[0].id, results[0].distance);
    // nearest id=0 distance=0.1414
}
```

---

## API

### `Builder`

| Method | Default | Description |
|---|---|---|
| `.m(usize)` | 16 | Max bidirectional links per node per non-zero layer. Higher → better recall, more RAM. Must be ≥ 2. |
| `.m0(usize)` | `2 × M` | Max links at layer 0 specifically. |
| `.ef_construction(usize)` | 200 | Beam width during index build. Higher → better quality, slower inserts. |
| `.heuristic(bool)` | `true` | Use heuristic neighbour selection (Algorithm 4) for the new node's own M connections. Recommended. |
| `.extend_candidates(bool)` | `false` | Expand candidates with their neighbours during heuristic selection. |
| `.keep_pruned(bool)` | `true` | Pad with pruned candidates when heuristic selects fewer than M. Improves recall. |
| `.prune_strategy(PruneStrategy)` | `Simple` | How to shrink an existing neighbour's list when it overflows after a bidirectional edge is added. See below. |
| `.capacity(usize)` | 0 | Expected number of vectors. Pre-allocates internal buffers. |
| `.seed(u64)` | entropy | Fix RNG seed for reproducible index layouts. |
| `.build(metric)` | — | Consume the builder and return an empty `Hnsw<D>`. |

### `Hnsw<D>`

| Method | Description |
|---|---|
| `insert(Vec<f32>) -> usize` | Add a vector; returns its assigned id (0-based). |
| `search(&[f32], k, ef) -> Vec<SearchResult>` | Return the `k` approximate nearest neighbours. |
| `get_vector(id) -> &[f32]` | Retrieve a stored vector by id. |
| `len() / is_empty() / dim() / max_level()` | Index introspection. |
| `stats() -> IndexStats` | Layer-by-layer node and edge counts. |

---

---

## Persistence

Every index type can be saved to a single binary file and reloaded with or without memory-mapping.

```rust
use hnsw::{Builder, persist};
use hnsw::distance::Euclidean;

let mut index = Builder::new().m(16).ef_construction(200).build(Euclidean);
// … insert vectors …

// Save
persist::save(&index, "index.hnsw")?;

// Load (vectors copied into RAM)
let loaded = persist::load("index.hnsw", Euclidean)?;

// Load with memory-mapped vector section (zero RAM copy; OS manages pages)
// Ideal for indexes larger than available RAM.
// Insert into a mmap-backed index will panic.
let mmap = persist::load_mmap("index.hnsw", Euclidean)?;
```

### File format

```
[0 .. 256]      Fixed header   magic · version · n · dim · m · m0 · ef · config flags
[256 .. ]       Vectors        n × dim × 4 bytes (f32 LE, row-major) ← mmap-able
[after vecs]    Levels         n × u32 — layer count per node
[after levels]  Conn offsets   n × u64 — absolute byte offsets into conn data
[at offsets]    Conn data      per-node: per-layer u32 count + (u32,f32) pairs
[after graph]   Payload hdr    payload_count · stride (0 = variable)
                Payload data   [optional offset table] + raw encoded bytes
```

The vector section always begins at byte 256 — a fixed, known offset — so `mmap + pointer arithmetic` gives a `&[f32]` slice with zero reformatting.

---

## Labeled index

A `LabeledIndex<D, L>` stores one value of type `L` alongside every vector.  Results from `search()` carry both the distance and a reference to the payload.

```rust
use hnsw::{Builder, labeled::LabeledIndex};
use hnsw::distance::Euclidean;

// ── Classification label (u32) ────────────────────────────────────────────────
let mut idx: LabeledIndex<Euclidean, u32> = Builder::new()
    .m(16).ef_construction(200).capacity(10_000)
    .build_labeled(Euclidean);

idx.insert(vec![1.0, 0.0], 0_u32);   // class 0
idx.insert(vec![0.0, 1.0], 1_u32);   // class 1

for hit in idx.search(&[0.9, 0.1], 3, 50) {
    println!("id={} dist={:.3} class={}", hit.id, hit.distance, hit.payload);
}

// ── Text tag (String) ─────────────────────────────────────────────────────────
let mut idx: LabeledIndex<Euclidean, String> = Builder::new().build_labeled(Euclidean);
idx.insert(vec![1.0, 0.0], "cat".to_string());
idx.insert(vec![0.0, 1.0], "dog".to_string());

// ── Secondary embedding as label (Vec<f32>) ───────────────────────────────────
// Store a second (non-searchable) embedding alongside the primary one.
// For a fully searchable second space, use PairedIndex instead.
let mut idx: LabeledIndex<Euclidean, Vec<f32>> = Builder::new().build_labeled(Euclidean);
idx.insert(vec![1.0, 0.0], vec![0.9f32, 0.1, 0.0]);  // 3-D secondary

// ── Custom payload ────────────────────────────────────────────────────────────
// Any type that implements Payload.  Fixed-stride types (same byte size for
// every instance) use a flat on-disk layout with no offset table.
// Variable-width types get an offset table automatically.

// ── Save / load ───────────────────────────────────────────────────────────────
idx.save("my.hnsw")?;
let loaded = LabeledIndex::<Euclidean, Vec<f32>>::load("my.hnsw", Euclidean)?;
let mmap   = LabeledIndex::<Euclidean, Vec<f32>>::load_mmap("my.hnsw", Euclidean)?;
```

---

## Paired index

A `PairedIndex<A, B>` maintains **two HNSW graphs** over the same items — one per embedding space — allowing search from either side.

```rust
use hnsw::{Builder, paired::PairedIndex};
use hnsw::distance::{Cosine, Euclidean};

// text_dim=4 (Cosine), image_dim=3 (Euclidean)
let mut idx: PairedIndex<Cosine, Euclidean> = Builder::new()
    .m(16).ef_construction(200)
    .build_paired(Cosine, Euclidean);

idx.insert(
    vec![1.0, 0.0, 0.0, 0.0],   // text embedding
    vec![0.9, 0.1, 0.0],         // image embedding
);
idx.insert(
    vec![0.0, 1.0, 0.0, 0.0],
    vec![0.1, 0.8, 0.1],
);

// Text query → find nearest items → also retrieve their image embeddings
for hit in idx.search_by_a(&[0.9, 0.1, 0.0, 0.0], 3, 50) {
    println!("id={} text_dist={:.3} image_emb={:?}",
             hit.id, hit.distance, hit.emb_b);
}

// Image query → find nearest items → also retrieve their text embeddings
for hit in idx.search_by_b(&[0.05, 0.85, 0.1], 3, 50) {
    println!("id={} image_dist={:.3} text_emb={:?}",
             hit.id, hit.distance, hit.emb_a);
}

// Persist: writes {base}_a.hnsw and {base}_b.hnsw
idx.save("my_index")?;
let loaded = PairedIndex::<Cosine, Euclidean>::load("my_index", Cosine, Euclidean)?;
let mmap   = PairedIndex::<Cosine, Euclidean>::load_mmap("my_index", Cosine, Euclidean)?;
```

### When to use which

| Use case | Recommended type |
|---|---|
| Plain ANN search, no labels | `Hnsw<D>` |
| Class label / score / text tag per vector | `LabeledIndex<D, u32/String/…>` |
| Secondary embedding (non-searchable, just retrieved) | `LabeledIndex<D, Vec<f32>>` |
| Two searchable embedding spaces (text+image, query+doc) | `PairedIndex<A, B>` |

---

## Custom `Payload`

Any type can be persisted alongside vectors by implementing two methods:

```rust
use hnsw::payload::{Payload, DecodeError};

#[derive(Clone)]
struct MyLabel { category: u16, score: f32 }

impl Payload for MyLabel {
    // Fixed size = 2 + 4 = 6 bytes → flat on-disk layout, no offset table.
    fn fixed_stride() -> Option<usize> { Some(6) }

    fn encode(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.category.to_le_bytes());
        buf.extend_from_slice(&self.score.to_le_bytes());
    }

    fn decode(data: &[u8]) -> Result<(Self, usize), DecodeError> {
        if data.len() < 6 { return Err(DecodeError("too short")); }
        Ok((MyLabel {
            category: u16::from_le_bytes(data[0..2].try_into().unwrap()),
            score:    f32::from_le_bytes(data[2..6].try_into().unwrap()),
        }, 6))
    }
}
```

Built-in implementations: `()`, `u32`, `u64`, `i32`, `i64`, `f32`, `f64`, `String`, `Vec<u8>`, `Vec<f32>`, and `(A, B)` for any two payload types.

---

## Distance metrics

| Type | Formula |
|---|---|
| `Euclidean` | `‖a − b‖₂` |
| `SquaredEuclidean` | `‖a − b‖₂²` (no `sqrt`, preserves NN order) |
| `Cosine` | `1 − cos(a, b)` |
| `DotProduct` | `1 − a·b` |
| `Manhattan` | `‖a − b‖₁` |

Custom metric: implement the `Distance` trait (one method: `fn distance(&self, a: &[f32], b: &[f32]) -> f32`).

---

## Pruning strategy

Every insert adds M bidirectional edges.  For each selected neighbour `nb`, the reverse edge `nb → q` is appended to `nb`'s list.  If `nb` already held M connections its list grows to M + 1 and must be pruned back.  Two strategies are available:

### `PruneStrategy::Simple` (default)

Sort the M + 1-entry list by the stored per-edge distance and truncate to M.

- **Zero new distance computations** — every connection stores `(neighbour_id: u32, dist: f32)`; the distance is recorded for free at edge-add time (symmetric metric).
- **Cost**: ~25 ns per prune — an in-register sort of M + 1 floats + a pointer update.
- **Recall**: beats hnsw_rs at every workload; ≈ 0–1 pp lower than `Heuristic` on very high-dimensional data.
- Equivalent to what faiss and hnsw_rs use for reverse-update pruning.

### `PruneStrategy::Heuristic` (opt-in)

Run the full paper Algorithm 4 diversity check, exploiting stored distances to eliminate M recomputations.

- Candidate `e` is kept only if `d(node, e) ≤ d(e, s)` for every already-selected neighbour `s` — ensuring the final M connections are *diverse* rather than all pointing the same direction.
- **Stored-distance shortcut**: `d(node, e)` is read from the stored `f32` — no recomputation.  Only the O(M²/2) pairwise `d(e, s)` checks are computed fresh; ~60% are eliminated by the triangle-inequality shortcut.
- **Cost**: ~1–11 µs per prune depending on cache state.  × M prunes/insert ≈ 170 µs overhead per insert at n = 10k, dim = 128.
- **Recall**: full Algorithm 4 quality; recovers the ≈ 1 pp gap vs `Simple` on high-dimensional data.

```rust
use hnsw::{Builder, PruneStrategy};
use hnsw::distance::Euclidean;

// Default — fastest, beats hnsw_rs on both speed and recall:
let fast = Builder::new()
    .prune_strategy(PruneStrategy::Simple)
    .build(Euclidean);

// Opt-in — maximum recall quality, full Algorithm 4 for every edge:
let quality = Builder::new()
    .prune_strategy(PruneStrategy::Heuristic)
    .build(Euclidean);
```

---

## Tuning guide

| Goal | Lever |
|---|---|
| Higher recall | Increase `ef` at query time and/or `ef_construction` at build time |
| Faster search | Reduce `ef` |
| Lower memory | Reduce `M` |
| Maximum insert speed | `PruneStrategy::Simple` (default) |
| Maximum recall quality | `PruneStrategy::Heuristic` |
| Fastest possible (simple greedy) | `heuristic(false)`, `keep_pruned(false)` |

---

## Internal optimisations

### Visited-node set: generation counter instead of `HashSet`

The inner loop of `search_layer` marks each visited node.  A `HashSet` costs ~1 703 ns to clear per call.  A flat stamp array with a generation counter costs 105 ns (no clearing — mismatched generations are treated as unvisited):

```
search_layer call N:  stamp[node] == N  → visited
                      stamp[node] != N  → unvisited (never cleared)
```

### Flat vector store: `id × dim` indexing

Vectors are stored in a single `Vec<f32>`.  Address of vector `id` = `&data[id * dim]` — one multiply, guaranteed single cache line, no pointer chase.

### Reusable heaps

Candidate and result heaps live in a `Scratch` struct owned by the `Hnsw`.  They are cleared (`Vec::clear`, keeping capacity) rather than dropped and reallocated on each `search_layer` call.

### `(u32, f32)` connection pairs — the key insight

Every connection list stores `(neighbour_id: u32, dist_from_this_node: f32)`.  The distance is symmetric and always known at edge-add time, so storing it is free.  This enables:

1. **`PruneStrategy::Simple`**: sort 17 floats + truncate — zero vector loads.
2. **`PruneStrategy::Heuristic`**: the M distance recomputations that a naïve heuristic prune would need are completely eliminated — only the pairwise diversity checks remain.

The storage overhead is 4 extra bytes per edge (8 bytes total vs 4 for a bare `u32`), equal to what you'd pay for an `Arc` or box pointer.

---

## Benchmark — ours vs. `hnsw_rs v0.3.3` and `hnsw v0.11` (rust-cv)

> **Setup:** M = 16 · ef\_construction = 200 · K = 10 · 500 queries · metric = L2(f32)
> Single-threaded · release build · ground truth = brute-force exact L2.
>
> Three libraries compared:
> - **ours** — this repo (pure Rust, `PruneStrategy::Simple` default)
> - **hnsw\_rs v0.3.3** — Jean-Pierre Both (Rayon + `parking_lot::RwLock`, inserts serialised)
> - **hnsw v0.11** (rust-cv) — Geordon Worley (const-generic M/M0, external `Searcher`, owns `Vec<f32>` per item)

### Optimisation journey and quality

![Optimisation stages — insert speed and recall](figures/fig6_before_after.png)

The deep blue bars (`Simple` default) consistently beat hnsw\_rs on insert speed.  The amber bars (`Heuristic`, opt-in) show the quality gain from full Algorithm 4 pruning at the cost of slower inserts on high-dimensional data.

### Insert throughput

`PruneStrategy::Simple` (the default) is **1.42–3.06× faster** than both competitors — sort+truncate of M stored floats is ~80× cheaper than hnsw\_rs's equivalent, and we clone no heap data per insert unlike hnsw v0.11.

![Insert throughput — 3 libraries](figures/fig1_insert_throughput.png)

### Search throughput

**1.5–3.7× faster** than hnsw\_rs and **1.5–3.9× faster** than hnsw v0.11 across all workloads and ef values.  hnsw\_rs acquires a `parking_lot::RwLock` on every graph-layer access; we have zero locking overhead.

![Search throughput — 3 libraries](figures/fig2_search_throughput.png)

### Recall@10

**+0.3 to +2.8 pp** higher recall than hnsw\_rs at every workload.  hnsw v0.11 applies full Algorithm 4 diversity pruning to *all* edges (including reverse-edge updates), giving it a quality edge at large n / high dim at the cost of 2–3× slower inserts.

![Recall@10 — 3 libraries](figures/fig3_recall.png)

### Recall vs. throughput tradeoff (per-library)

Our curves sit to the right of hnsw\_rs's on every workload — better recall at the same QPS, or the same recall at higher QPS.

![Recall vs QPS tradeoff](figures/fig4_recall_vs_qps.png)

### All three libraries on one chart

Colour = workload (n/dim), line style = library.  At small n every library reaches near-perfect recall; the separation grows with n and dim.

![All-library recall vs QPS overlay](figures/fig7_all_tradeoff.png)

### Speedup summary

Rows split by competitor.  Blue = ours faster, red = ours slower.

![Speedup heatmap](figures/fig5_speedup_heatmap.png)

### Numerical summary

**Insert throughput** (`PruneStrategy::Simple`, vectors / second)

| Workload | ours | hnsw\_rs | vs rs | hnsw v0.11 | vs v0 |
|---|---:|---:|:---:|---:|:---:|
| n=1k,  dim=32  | 18 451 | 10 248 | **▲1.80×** | 11 452 | **▲1.61×** |
| n=1k,  dim=128 |  8 856 |  6 069 | **▲1.46×** |  6 245 | **▲1.42×** |
| n=10k, dim=32  |  9 612 |  3 861 | **▲2.49×** |  3 587 | **▲2.68×** |
| n=10k, dim=128 |  3 640 |  2 030 | **▲1.79×** |  1 804 | **▲2.02×** |
| n=50k, dim=128 |  2 241 |  1 045 | **▲2.14×** |    733 | **▲3.06×** |

**Search throughput at ef=200** (queries / second)

| Workload | ours | hnsw\_rs | vs rs | hnsw v0.11 | vs v0 |
|---|---:|---:|:---:|---:|:---:|
| n=1k,  dim=32  | 15 194 |  8 139 | **▲1.87×** |  8 537 | **▲1.78×** |
| n=1k,  dim=128 |  8 018 |  5 192 | **▲1.54×** |  5 403 | **▲1.48×** |
| n=10k, dim=32  |  7 999 |  2 990 | **▲2.68×** |  2 588 | **▲3.09×** |
| n=10k, dim=128 |  3 281 |  1 561 | **▲2.10×** |  1 250 | **▲2.62×** |
| n=50k, dim=128 |  1 836 |    864 | **▲2.12×** |    491 | **▲3.74×** |

**Recall@10 at ef=200** (`PruneStrategy::Simple`)

| Workload | ours | hnsw\_rs | Δ vs rs | hnsw v0.11 | Δ vs v0 |
|---|---:|---:|:---:|---:|:---:|
| n=1k,  dim=32  | **100.0%** | 98.6% | +1.4 pp | 98.7% | +1.3 pp |
| n=1k,  dim=128 | **100.0%** | 98.1% | +1.9 pp | 98.7% | +1.3 pp |
| n=10k, dim=32  |  **99.9%** | 97.4% | +2.5 pp | 99.1% | +0.8 pp |
| n=10k, dim=128 |      95.6% | 93.6% | +2.0 pp | 98.7% | **−3.1 pp** |
| n=50k, dim=128 |      78.0% | 75.2% | +2.8 pp | 93.2% | **−15.2 pp** |

> hnsw v0.11 wins on recall at large n / high dim because it runs full Algorithm 4
> on *every* reverse-edge prune.  Switching our index to `PruneStrategy::Heuristic`
> closes most of the gap while keeping a 2–3× insert-speed advantage.

**`PruneStrategy::Heuristic` recall gain vs Simple**

| Workload | Simple | Heuristic | gain |
|---|---:|---:|---|
| n=1k,  dim=32  | 100.0% | 100.0% | — |
| n=1k,  dim=128 | 100.0% | 100.0% | — |
| n=10k, dim=32  |  99.9% | 100.0% | +0.1 pp |
| n=10k, dim=128 |  95.6% |  96.6% | **+1.0 pp** |
| n=50k, dim=128 |  78.0% |  78.7% | **+0.7 pp** |

---

## Standalone timing bench (ours only)

Single-library wall-clock measurements; no recall computation, no competitors.
Run with:

```bash
cargo bench --bench bench               # default workloads (≤ 50k)
cargo bench --bench bench -- --full     # scale to 1 M in 100k steps
python3 figures/plot_bench.py
```

### Insert throughput and latency

![Bench insert throughput](figures/bench_fig1_insert_throughput.png)

![Bench insert latency](figures/bench_fig2_insert_latency.png)

### Search throughput

Grouped by beam width (ef).  All searches return K = 10 results.

![Bench search QPS](figures/bench_fig3_search_qps.png)

### Scaling with index size (--full only)

Rendered only when `bench.jsonl` contains ≥ 3 distinct n values at dim = 128
(i.e. after `cargo bench --bench bench -- --full`).

![Bench scaling](figures/bench_fig4_scaling.png)

---

---

## Persistence benchmark

Run:

```bash
cargo bench --bench persist
```

### What is measured

For every combination of **workload** (n=1k/10k/50k × dim=32/128) and **index type**:

| Metric | Description |
|---|---|
| File size | Total bytes written to disk |
| Save time | Wall-clock time to serialize (median of 7/3 reps for small/large n) |
| Save MB/s | `file_size / save_time` |
| Load time | Wall-clock time to read all bytes into RAM |
| Load MB/s | `file_size / load_time` |
| mmap time | Wall-clock time to map the file + deserialize graph (vector bytes **not** read) |
| mmap speedup | `load_time / mmap_time` |

### Save throughput

Fixed-payload types (`Hnsw`, `+u32`, `PairedIndex`) save at **530–1130 MB/s** because they write sequentially with no offset table.  Variable-width payloads (`String`, `Vec<f32>`) are slower (350–680 MB/s) due to a two-pass write: placeholder offsets → data → seek back to patch offsets.

![Save throughput](figures/fig7_save_throughput.png)

### mmap-load speedup

The mmap load skips reading the vector section entirely — the OS faults vector pages into RAM only when they are first accessed during `search()`.  The speedup vs owned load depends on how large the vector section is relative to the rest of the file.

![mmap speedup](figures/fig8_mmap_speedup.png)

| Index type | Speedup (typical) | Why |
|---|---|---|
| Bare `Hnsw` | **76–91×** | Nearly all bytes are vector data; skipping them is almost all the work |
| `+u32` | **38–40×** | u32 payload is negligible; most savings still from vectors |
| `+String` | **15–17×** | Payload offset table + data must still be read; reduces the vector-skip fraction |
| `+Vec<f32>` (32-dim) | **15–16×** | Similar — 132 bytes/entry payload competes with ~512 bytes/entry vectors at dim=128 |
| `PairedIndex` | **68–88×** | Double the vector data (two HNSW graphs); mmap saves twice as much |

The speedup stays remarkably **constant across n** (same row across columns in the heatmap) because the ratio of vector bytes to graph bytes is fixed by `n × dim` and `M`.

### Load latency comparison

![Load latency](figures/fig9_load_latency.png)

At n=50k/dim=128: owned load takes **865 ms**, mmap load takes **11 ms**.  For a `PairedIndex` (75 MiB total) the contrast is even sharper: **1 700 ms owned vs 25 ms mmap**.

### File sizes

All types scale **linearly** with n (straight lines on log-log = linear growth).

![File sizes](figures/fig10_file_sizes.png)

**Breakdown at n=50k, dim=128:**

| Type | File size | Overhead vs bare |
|---|---:|---|
| Bare `Hnsw` | 37.8 MiB | — |
| `+u32` | 38.0 MiB | +0.2 MiB (4 bytes/entry) |
| `+String` ("item-NNNNN") | 38.8 MiB | +1.0 MiB (14 bytes/entry) |
| `+Vec<f32>` (32-dim) | 44.5 MiB | +6.7 MiB (132 bytes/entry) |
| `PairedIndex` | 75.6 MiB | +37.8 MiB (full second HNSW copy) |

The vector section (`n × dim × 4 bytes = 25.6 MiB`) dominates in all cases.  The graph structure (levels + connection lists) adds ~12 MiB at n=50k/M=16.

---

## Running the benchmarks

```bash
cargo build --release

# Standalone wall-clock timing (ours only)
cargo bench --bench bench                  # default workloads (≤ 50k)
cargo bench --bench bench -- --full        # scale to 1 M in 100k steps

# 3-library ANN quality + speed comparison
cargo bench --bench compare                # default workloads (≤ 50k)
cargo bench --bench compare -- --full      # scale to 1 M  (~hours)

# Persistence: save / load / mmap-load timing
cargo bench --bench persist                # default workloads (≤ 50k)
cargo bench --bench persist -- --full      # scale to 1 M  (~hours)

# Regenerate all figures
python3 figures/plot_bench.py              # bench_fig1–4
python3 figures/plot_benchmarks.py         # fig1–7 (3-library comparison)
python3 figures/plot_persist.py            # fig7_save – fig10
```

Each bench streams results to its output file as workloads complete, so
partial data is available immediately if a long `--full` run is interrupted.

---

## File layout

```
hnsw/
├── src/
│   ├── lib.rs          Public API, re-exports, 55 tests (unit + doc)
│   ├── hnsw.rs         Core HNSW + PruneStrategy + MmapBacking + all optimisations
│   ├── persist.rs      Binary file format · save / load / load_mmap
│   ├── labeled.rs      LabeledIndex<D, L: Payload>
│   ├── paired.rs       PairedIndex<A, B>  — two searchable embedding spaces
│   ├── payload.rs      Payload trait + built-in impls
│   ├── distance.rs     Distance trait + 5 built-in metrics
│   ├── heap.rs         DistId ordered pair, candidate/result heaps
│   └── builder.rs      Ergonomic builder (.prune_strategy, .build_labeled, .build_paired)
├── benches/
│   ├── bench.rs        Standalone wall-clock timing (ours only); writes bench.jsonl
│   ├── compare.rs      3-library comparison (ours/hnsw_rs/hnsw v0.11); writes compare.jsonl
│   └── persist.rs      Save / load / mmap-load timing + file sizes; writes persist.csv
├── examples/
│   ├── demo.rs         Core HNSW walkthrough
│   └── store.rs        Persistence + LabeledIndex + PairedIndex demos
└── figures/
    ├── plot_bench.py              Plotter for bench.jsonl → bench_fig1–4
    ├── plot_benchmarks.py         Plotter for compare.jsonl → fig1–7
    ├── plot_persist.py            Plotter for persist.csv → fig7_save–fig10
    │
    ├── bench_fig1_insert_throughput.png   Ours: insert vecs/s by workload
    ├── bench_fig2_insert_latency.png      Ours: µs/insert by workload
    ├── bench_fig3_search_qps.png          Ours: search QPS by workload × ef
    ├── bench_fig4_scaling.png             Ours: throughput vs n line chart (--full)
    │
    ├── fig1_insert_throughput.png         3-lib: insert throughput grouped bars
    ├── fig2_search_throughput.png         3-lib: search throughput grouped bars
    ├── fig3_recall.png                    3-lib: recall@10 grouped bars
    ├── fig4_recall_vs_qps.png             3-lib: recall vs QPS tradeoff curves
    ├── fig5_speedup_heatmap.png           3-lib: ours vs each competitor speedup
    ├── fig6_before_after.png              Optimisation journey + strategy comparison
    ├── fig7_all_tradeoff.png              All 3 libs on one recall-vs-QPS overlay
    │
    ├── fig7_save_throughput.png           Persist: save MB/s by type and workload
    ├── fig8_mmap_speedup.png              Persist: mmap-load speedup heatmap
    ├── fig9_load_latency.png              Persist: owned vs mmap load time
    └── fig10_file_sizes.png               Persist: file size breakdown by type
```

---

## License

MIT

## Citations

Please quote this work if you found it helpful.

```
@software{fasthnsw2026,
  author = {Kosmyna, Nataliya},
  title  = {fast-hnwf: Pure-Rust implementation of Hierarchical Navigable Small World (HNSW) approximate nearest-neighbour search},
  year   = {2026},
  url    = {https://github.com/nataliyakosmyna/fast-hnsw}
}
```