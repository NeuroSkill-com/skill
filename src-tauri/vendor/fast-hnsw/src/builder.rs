//! Ergonomic builder for [`Hnsw`].
//!
//! ```
//! use hnsw::{Builder, Hnsw};
//! use hnsw::distance::Cosine;
//!
//! let index: Hnsw<Cosine> = Builder::new()
//!     .m(32)
//!     .ef_construction(400)
//!     .build(Cosine);
//! ```

use crate::distance::Distance;
use crate::hnsw::{Config, Hnsw, PruneStrategy};

/// Builder for [`Hnsw`] indexes.
#[derive(Clone, Debug, Default)]
pub struct Builder {
    config: Config,
    seed: Option<u64>,
}

impl Builder {
    /// Start with default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of bidirectional links per node per layer (`M`).
    ///
    /// - Higher `M` → better recall and higher memory use.
    /// - Typical values: 8–64.
    /// - Must be ≥ 2.
    pub fn m(mut self, m: usize) -> Self {
        self.config.m = m;
        self
    }

    /// Override the maximum links at layer 0 (default: `2 * M`).
    pub fn m0(mut self, m0: usize) -> Self {
        self.config.m0 = Some(m0);
        self
    }

    /// Set the beam width used during index construction (`ef_construction`).
    ///
    /// - Higher value → better recall at the cost of slower inserts.
    /// - Must be ≥ `M`.  Typical values: 100–500.
    pub fn ef_construction(mut self, ef: usize) -> Self {
        self.config.ef_construction = ef;
        self
    }

    /// Use the heuristic neighbour-selection strategy (default: `true`).
    ///
    /// The heuristic selects more *diverse* neighbours and is recommended
    /// for most use-cases.  Setting this to `false` uses the simpler greedy
    /// strategy (Algorithm 3 in the paper).
    pub fn heuristic(mut self, yes: bool) -> Self {
        self.config.use_heuristic = yes;
        self
    }

    /// Enable/disable the *extendCandidates* option of the heuristic
    /// (default: `false`).  Usually not needed.
    pub fn extend_candidates(mut self, yes: bool) -> Self {
        self.config.extend_candidates = yes;
        self
    }

    /// Enable/disable the *keepPrunedConnections* option of the heuristic
    /// (default: `true`).
    pub fn keep_pruned(mut self, yes: bool) -> Self {
        self.config.keep_pruned = yes;
        self
    }

    /// Set the strategy used to prune an existing node's connection list when
    /// it overflows `M` after a bidirectional edge is added during insertion.
    ///
    /// | Strategy | Insert speed | Recall |
    /// |---|---|---|
    /// | [`PruneStrategy::Simple`] (default) | fastest | ≈ 0–1 pp lower on high-dim data |
    /// | [`PruneStrategy::Heuristic`] | slower | full Algorithm 4 quality |
    ///
    /// ## `Simple` (default)
    ///
    /// Sorts the neighbour list by the stored per-edge distance and truncates
    /// to `M`.  **Zero new distance computations** — the `f32` distance in
    /// each `(u32, f32)` connection entry was recorded at edge-add time and
    /// is always current (symmetric metric).  Approximately 80× faster per
    /// prune than `Heuristic`; equivalent to what faiss and hnsw_rs use for
    /// reverse-update pruning.
    ///
    /// ## `Heuristic`
    ///
    /// Runs the full paper diversity check for every reverse-update prune:
    /// candidate `e` is kept only if `d(node, e) ≤ d(e, s)` for all already-
    /// selected neighbours `s`.  Stored distances eliminate the M
    /// recomputations; only the O(M²/2) pairwise inter-neighbour checks
    /// remain (≈ 60% skipped by the triangle-inequality shortcut).
    ///
    /// Use `Heuristic` when:
    /// - Recall quality is non-negotiable.
    /// - You want a true like-for-like implementation of Algorithm 4.
    /// - High-dimensional data (dim ≥ 64) where neighbour diversity matters.
    ///
    /// # Example
    ///
    /// ```
    /// use hnsw::{Builder, PruneStrategy};
    /// use hnsw::distance::Euclidean;
    ///
    /// // Fastest inserts (default):
    /// let fast = Builder::new()
    ///     .prune_strategy(PruneStrategy::Simple)
    ///     .build(Euclidean);
    ///
    /// // Full Algorithm 4 quality:
    /// let quality = Builder::new()
    ///     .prune_strategy(PruneStrategy::Heuristic)
    ///     .build(Euclidean);
    /// ```
    pub fn prune_strategy(mut self, strategy: PruneStrategy) -> Self {
        self.config.prune_strategy = strategy;
        self
    }

    /// Pre-allocate for `n` vectors (capacity hint — no effect on correctness).
    ///
    /// Providing an accurate hint avoids incremental `Vec` growth and reduces
    /// the total number of allocator calls during bulk loading.
    pub fn capacity(mut self, n: usize) -> Self {
        self.config.capacity = n;
        self
    }

    /// Fix the RNG seed for reproducible indexes (useful in tests).
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Consume the builder and create an empty [`Hnsw`] index.
    pub fn build<D: Distance>(self, metric: D) -> Hnsw<D> {
        match self.seed {
            Some(s) => Hnsw::new_with_seed(self.config, metric, s),
            None    => Hnsw::new(self.config, metric),
        }
    }

    /// Consume the builder and return the resolved [`Config`].
    ///
    /// Used internally by [`build_labeled`] and [`build_paired`]; also
    /// useful when you need to share the same config across multiple indexes.
    pub fn into_config(self) -> Config {
        self.config
    }
}
