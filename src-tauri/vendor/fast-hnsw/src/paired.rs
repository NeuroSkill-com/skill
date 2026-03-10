//! Dual-embedding index: two HNSW graphs over the same items.
//!
//! A [`PairedIndex`] stores every item as a pair `(embedding_a, embedding_b)`
//! and builds **two independent HNSW graphs** — one for each embedding space.
//! Because both graphs are indexed by the same integer ids (0, 1, 2, …),
//! a search in either graph immediately tells you the corresponding embedding
//! from the other side.
//!
//! ## Typical use-cases
//!
//! | Side A | Side B | Search goal |
//! |---|---|---|
//! | Text embedding | Image embedding | "find images closest to this text query" |
//! | Query embedding | Document embedding | "find documents closest to this query" |
//! | Audio embedding | Video-frame embedding | cross-modal retrieval |
//! | Anchor embedding | Augmented-view embedding | self-supervised nearest-neighbour lookup |
//!
//! ## Searching
//!
//! * [`search_by_a`] — query in the A space; each result carries the
//!   corresponding B embedding (useful when you want to retrieve B items
//!   ranked by A-space similarity).
//! * [`search_by_b`] — symmetric, queries in the B space.
//!
//! Both return [`PairedResult`] which carries both embeddings and both
//! distances (the queried distance is `distance`, the cross-space distance is
//! `cross_distance` — computed lazily on request via [`PairedResult::cross_distance`]).
//!
//! ## Persistence
//!
//! [`PairedIndex::save`] writes **two files**: `{path}_a.hnsw` and
//! `{path}_b.hnsw`.  [`PairedIndex::load`] reads both back.  The two files
//! are independent and can be memory-mapped separately.
//!
//! ## Example
//!
//! ```rust
//! use hnsw::paired::PairedIndex;
//! use hnsw::distance::Euclidean;
//!
//! // text_dim=4, image_dim=3 (toy example)
//! let mut idx: PairedIndex<Euclidean, Euclidean> = PairedIndex::new(
//!     Default::default(), Euclidean,
//!     Default::default(), Euclidean,
//! );
//!
//! idx.insert(vec![1.0, 0.0, 0.0, 0.0], vec![0.9, 0.1, 0.0]);  // item 0
//! idx.insert(vec![0.0, 1.0, 0.0, 0.0], vec![0.0, 0.8, 0.2]);  // item 1
//! idx.insert(vec![0.0, 0.0, 1.0, 0.0], vec![0.1, 0.1, 0.9]);  // item 2
//!
//! // Query in A-space → get B-space embeddings for the nearest items.
//! let text_query = vec![0.9, 0.1, 0.0, 0.0];
//! for hit in idx.search_by_a(&text_query, 2, 20) {
//!     println!("id={} text_dist={:.3} image_emb={:?}", hit.id, hit.distance, hit.emb_b);
//! }
//!
//! // Query in B-space → get A-space (text) embeddings for the nearest items.
//! let image_query = vec![0.2, 0.7, 0.1];
//! for hit in idx.search_by_b(&image_query, 2, 20) {
//!     println!("id={} image_dist={:.3} text_emb={:?}", hit.id, hit.distance, hit.emb_a);
//! }
//! ```

use std::io;
use std::path::Path;

use crate::Builder;
use crate::distance::Distance;
use crate::hnsw::{Config, Hnsw};
use crate::persist;

// ─── Result type ─────────────────────────────────────────────────────────────

/// One result from [`PairedIndex::search_by_a`] or [`search_by_b`](PairedIndex::search_by_b).
pub struct PairedResult<'a> {
    /// Zero-based insertion id (same across both sides of the pair).
    pub id:       usize,
    /// Distance from the query **in the space that was searched**.
    ///
    /// If you called `search_by_a`, this is the distance in the A metric
    /// space; if you called `search_by_b`, it is the B metric space distance.
    pub distance: f32,
    /// Borrowed slice of the A-side embedding.
    pub emb_a: &'a [f32],
    /// Borrowed slice of the B-side embedding.
    pub emb_b: &'a [f32],
}

// ─── PairedIndex ─────────────────────────────────────────────────────────────

/// Two HNSW indexes over the same set of items.
///
/// ## Type parameters
/// * `A` — distance metric for the first ("A") embedding space
/// * `B` — distance metric for the second ("B") embedding space
///
/// The two spaces can have different dimensions and different metrics.
pub struct PairedIndex<A: Distance, B: Distance> {
    /// The A-side HNSW graph.  Exposed as `pub` for direct access
    /// (e.g. `idx.index_a.stats()`).
    pub index_a: Hnsw<A>,
    /// The B-side HNSW graph.
    pub index_b: Hnsw<B>,
}

impl<A: Distance, B: Distance> PairedIndex<A, B> {
    // ─── Constructors ─────────────────────────────────────────────────────

    /// Create a new paired index with separate configs for each side.
    ///
    /// The two configs can specify different `M`, `ef_construction`, or
    /// pruning strategies if the embedding spaces have different
    /// characteristics (e.g. different dimensionality).
    pub fn new(
        config_a: Config, metric_a: A,
        config_b: Config, metric_b: B,
    ) -> Self {
        Self {
            index_a: Hnsw::new(config_a, metric_a),
            index_b: Hnsw::new(config_b, metric_b),
        }
    }

    /// Build both sides from a shared [`Builder`] config (same `M`, `ef`,
    /// etc. for both spaces).
    ///
    /// Use [`new`](Self::new) if you need different configs per side.
    ///
    /// ```rust
    /// use hnsw::{Builder, paired::PairedIndex};
    /// use hnsw::distance::{Euclidean, Cosine};
    ///
    /// let mut idx = PairedIndex::from_builder(
    ///     Builder::new().m(16).ef_construction(200),
    ///     Euclidean, Cosine,
    /// );
    /// ```
    pub fn from_builder(builder: Builder, metric_a: A, metric_b: B) -> Self {
        let cfg = builder.into_config();
        Self {
            index_a: Hnsw::new(cfg.clone(), metric_a),
            index_b: Hnsw::new(cfg, metric_b),
        }
    }

    // ─── Mutation ─────────────────────────────────────────────────────────

    /// Insert an (A, B) embedding pair.
    ///
    /// Both embeddings are added to their respective HNSW graphs and assigned
    /// the same id.  Returns that shared id.
    ///
    /// # Panics
    /// * Panics if `emb_a.len()` or `emb_b.len()` is inconsistent with
    ///   previously inserted vectors on the respective side.
    /// * Panics if either index was loaded with `load_mmap` (read-only).
    pub fn insert(&mut self, emb_a: Vec<f32>, emb_b: Vec<f32>) -> usize {
        let id_a = self.index_a.insert(emb_a);
        let id_b = self.index_b.insert(emb_b);
        debug_assert_eq!(id_a, id_b, "PairedIndex: side-A and side-B id mismatch");
        id_a
    }

    // ─── Query ────────────────────────────────────────────────────────────

    /// Find the `k` nearest items by **A-space similarity**.
    ///
    /// Each [`PairedResult`] carries:
    /// * `id` — shared item id
    /// * `distance` — A-space distance from `query`
    /// * `emb_a` — stored A-side embedding
    /// * `emb_b` — stored B-side embedding (retrieved "for free" by id)
    ///
    /// Use this when you have an A-space query and want to retrieve both
    /// the A and B embeddings of the nearest items.
    pub fn search_by_a<'a>(
        &'a self,
        query: &[f32],
        k:     usize,
        ef:    usize,
    ) -> Vec<PairedResult<'a>> {
        self.index_a
            .search(query, k, ef)
            .into_iter()
            .map(|sr| PairedResult {
                id:       sr.id,
                distance: sr.distance,
                emb_a:    self.index_a.get_vector(sr.id),
                emb_b:    self.index_b.get_vector(sr.id),
            })
            .collect()
    }

    /// Find the `k` nearest items by **B-space similarity**.
    ///
    /// Symmetric to [`search_by_a`](Self::search_by_a).  Each result carries
    /// the B-space distance and both embeddings.
    pub fn search_by_b<'a>(
        &'a self,
        query: &[f32],
        k:     usize,
        ef:    usize,
    ) -> Vec<PairedResult<'a>> {
        self.index_b
            .search(query, k, ef)
            .into_iter()
            .map(|sr| PairedResult {
                id:       sr.id,
                distance: sr.distance,
                emb_a:    self.index_a.get_vector(sr.id),
                emb_b:    self.index_b.get_vector(sr.id),
            })
            .collect()
    }

    // ─── Direct access ────────────────────────────────────────────────────

    /// Retrieve the A-side embedding for a specific id.
    pub fn get_emb_a(&self, id: usize) -> &[f32] { self.index_a.get_vector(id) }

    /// Retrieve the B-side embedding for a specific id.
    pub fn get_emb_b(&self, id: usize) -> &[f32] { self.index_b.get_vector(id) }

    /// Number of items in the index (same for both sides by construction).
    pub fn len(&self) -> usize { self.index_a.len() }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool { self.index_a.is_empty() }

    // ─── Persistence ──────────────────────────────────────────────────────

    /// Save both sides of the index.
    ///
    /// Writes two files:
    /// * `{path}_a.hnsw` — the A-side HNSW index
    /// * `{path}_b.hnsw` — the B-side HNSW index
    ///
    /// The two files are independent — they can be loaded separately or
    /// memory-mapped independently.
    ///
    /// # Example
    /// ```no_run
    /// # use hnsw::paired::PairedIndex; use hnsw::distance::Euclidean;
    /// # let idx: PairedIndex<Euclidean, Euclidean> = PairedIndex::new(Default::default(), Euclidean, Default::default(), Euclidean);
    /// idx.save("my_index").unwrap();
    /// // Writes: my_index_a.hnsw  my_index_b.hnsw
    /// ```
    pub fn save(&self, base_path: impl AsRef<Path>) -> io::Result<()> {
        let base = base_path.as_ref();
        let path_a = side_path(base, 'a');
        let path_b = side_path(base, 'b');
        persist::save(&self.index_a, &path_a)?;
        persist::save(&self.index_b, &path_b)?;
        Ok(())
    }

    /// Load both sides, copying all vector data into RAM.
    ///
    /// # Example
    /// ```no_run
    /// # use hnsw::paired::PairedIndex; use hnsw::distance::Euclidean;
    /// let idx = PairedIndex::<Euclidean, Euclidean>::load(
    ///     "my_index", Euclidean, Euclidean
    /// ).unwrap();
    /// ```
    pub fn load(
        base_path: impl AsRef<Path>,
        metric_a:  A,
        metric_b:  B,
    ) -> io::Result<Self> {
        let base   = base_path.as_ref();
        let index_a = persist::load(side_path(base, 'a'), metric_a)?;
        let index_b = persist::load(side_path(base, 'b'), metric_b)?;
        if index_a.len() != index_b.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "side-A has {} vectors but side-B has {} — mismatched files?",
                    index_a.len(), index_b.len()
                ),
            ));
        }
        Ok(Self { index_a, index_b })
    }

    /// Load both sides with vector sections **memory-mapped**.
    ///
    /// Neither side copies vector data into RAM; the OS page cache manages
    /// residency.  Graph structures are still deserialized into heap memory.
    /// Inserts into a mmap-backed index will panic.
    pub fn load_mmap(
        base_path: impl AsRef<Path>,
        metric_a:  A,
        metric_b:  B,
    ) -> io::Result<Self> {
        let base    = base_path.as_ref();
        let index_a = persist::load_mmap(side_path(base, 'a'), metric_a)?;
        let index_b = persist::load_mmap(side_path(base, 'b'), metric_b)?;
        Ok(Self { index_a, index_b })
    }
}

// ─── Builder extension ────────────────────────────────────────────────────────

impl Builder {
    /// Consume the builder and create an empty [`PairedIndex`] with the same
    /// config for both sides.
    ///
    /// ```rust
    /// use hnsw::{Builder, paired::PairedIndex};
    /// use hnsw::distance::{Euclidean, Cosine};
    ///
    /// let mut idx = Builder::new()
    ///     .m(16)
    ///     .ef_construction(200)
    ///     .build_paired(Euclidean, Cosine);
    ///
    /// idx.insert(vec![1.0, 0.0], vec![0.0, 0.0, 1.0]);
    /// ```
    pub fn build_paired<A: Distance, B: Distance>(
        self,
        metric_a: A,
        metric_b: B,
    ) -> PairedIndex<A, B> {
        PairedIndex::from_builder(self, metric_a, metric_b)
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn side_path(base: &Path, side: char) -> std::path::PathBuf {
    let mut s = base.as_os_str().to_owned();
    s.push(format!("_{side}.hnsw"));
    std::path::PathBuf::from(s)
}
