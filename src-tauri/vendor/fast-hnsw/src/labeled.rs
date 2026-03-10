//! Single-embedding index with typed per-vector payload.
//!
//! A [`LabeledIndex`] wraps an [`Hnsw`] and stores one value of type `L`
//! alongside every inserted vector.  The payload can be:
//!
//! * a **class label** (`u32`, `i32`)
//! * a **text tag** (`String`)
//! * a **secondary embedding** (`Vec<f32>`) — for simple cross-modal lookup
//!   without a second index
//! * any other type that implements [`Payload`]
//!
//! For the case where the payload is itself a full searchable embedding in a
//! *different* space, use [`PairedIndex`](crate::paired::PairedIndex) instead,
//! which builds a second HNSW graph so you can search from either side.
//!
//! # Example — classification label
//!
//! ```rust
//! use hnsw::labeled::LabeledIndex;
//! use hnsw::distance::Euclidean;
//!
//! let mut idx: LabeledIndex<Euclidean, u32> = LabeledIndex::new(
//!     Default::default(), Euclidean,
//! );
//!
//! idx.insert(vec![1.0, 0.0], 0_u32);   // class 0
//! idx.insert(vec![0.0, 1.0], 1_u32);   // class 1
//! idx.insert(vec![0.8, 0.2], 0_u32);   // class 0
//!
//! let results = idx.search(&[0.9, 0.1], 2, 20);
//! for r in &results {
//!     println!("id={} dist={:.3} class={}", r.id, r.distance, r.payload);
//! }
//! ```
//!
//! # Example — text label + save/load
//!
//! ```rust,no_run
//! use hnsw::labeled::LabeledIndex;
//! use hnsw::distance::Euclidean;
//!
//! let mut idx: LabeledIndex<Euclidean, String> = LabeledIndex::new(
//!     Default::default(), Euclidean,
//! );
//! idx.insert(vec![1.0, 0.0], "cat".to_string());
//! idx.insert(vec![0.0, 1.0], "dog".to_string());
//!
//! idx.save("animals.hnsw").unwrap();
//!
//! let loaded = LabeledIndex::<Euclidean, String>::load("animals.hnsw", Euclidean).unwrap();
//! let hits = loaded.search(&[0.9, 0.1], 1, 10);
//! println!("nearest: {}", hits[0].payload);  // "cat"
//! ```

use std::io;
use std::path::Path;

use crate::Builder;
use crate::distance::Distance;
use crate::hnsw::{Config, Hnsw};
use crate::payload::Payload;
use crate::persist;

// ─── Result type ─────────────────────────────────────────────────────────────

/// One result returned by [`LabeledIndex::search`].
pub struct LabeledResult<'a, L> {
    /// Zero-based insertion id.
    pub id:        usize,
    /// Distance from the query in the metric space of the index.
    pub distance:  f32,
    /// Reference to the payload associated with this vector.
    pub payload:   &'a L,
    /// The stored vector (borrowed from the index).
    pub embedding: &'a [f32],
}

// ─── LabeledIndex ─────────────────────────────────────────────────────────────

/// HNSW index with a typed payload attached to every vector.
///
/// ## Type parameters
/// * `D` — distance metric (e.g. [`Euclidean`](crate::distance::Euclidean))
/// * `L` — payload type (any type implementing [`Payload`])
pub struct LabeledIndex<D: Distance, L: Payload> {
    /// The underlying HNSW graph.  Exposed as `pub` so you can call
    /// `index.inner.stats()`, `index.inner.dim()`, etc. directly.
    pub inner:    Hnsw<D>,
    payloads:     Vec<L>,
}

impl<D: Distance, L: Payload> LabeledIndex<D, L> {
    // ─── Constructors ─────────────────────────────────────────────────────

    /// Create a new, empty labeled index.
    ///
    /// For ergonomic construction with method chaining, use
    /// [`Builder::build_labeled`](LabeledBuilder).
    pub fn new(config: Config, metric: D) -> Self {
        Self {
            payloads: Vec::with_capacity(config.capacity),
            inner:    Hnsw::new(config, metric),
        }
    }

    /// Build from an existing [`Builder`].
    ///
    /// ```rust
    /// use hnsw::{Builder, labeled::LabeledIndex};
    /// use hnsw::distance::Euclidean;
    ///
    /// let mut idx: LabeledIndex<Euclidean, String> = Builder::new()
    ///     .m(16)
    ///     .ef_construction(200)
    ///     .capacity(10_000)
    ///     .build_labeled(Euclidean);
    /// ```
    pub fn from_builder(builder: Builder, metric: D) -> Self {
        Self::new(builder.into_config(), metric)
    }

    // ─── Mutation ─────────────────────────────────────────────────────────

    /// Insert a vector with its associated payload.
    ///
    /// Returns the zero-based id assigned to this vector (same as `self.len()
    /// - 1` after the call).  Both the vector and the payload are stored
    /// internally and retrievable by id.
    ///
    /// # Panics
    /// Panics if `embedding.len()` differs from previously inserted vectors,
    /// or if the index was loaded with `load_mmap` (read-only).
    pub fn insert(&mut self, embedding: Vec<f32>, payload: L) -> usize {
        let id = self.inner.insert(embedding);
        debug_assert_eq!(id, self.payloads.len());
        self.payloads.push(payload);
        id
    }

    // ─── Query ────────────────────────────────────────────────────────────

    /// Find the `k` approximate nearest neighbours of `query`.
    ///
    /// `ef` controls the recall/speed trade-off (`ef ≥ k`; larger → better
    /// recall).  Each result carries:
    /// * `id` — the vector's insertion index
    /// * `distance` — distance from `query` in the index's metric space
    /// * `payload` — borrowed reference to the stored payload
    /// * `embedding` — borrowed slice of the stored vector
    pub fn search<'a>(
        &'a self,
        query: &[f32],
        k:     usize,
        ef:    usize,
    ) -> Vec<LabeledResult<'a, L>> {
        self.inner
            .search(query, k, ef)
            .into_iter()
            .map(|sr| LabeledResult {
                id:        sr.id,
                distance:  sr.distance,
                payload:   &self.payloads[sr.id],
                embedding: self.inner.get_vector(sr.id),
            })
            .collect()
    }

    // ─── Direct access ────────────────────────────────────────────────────

    /// Retrieve the payload for a specific id.
    ///
    /// # Panics
    /// Panics if `id >= self.len()`.
    pub fn get_payload(&self, id: usize) -> &L { &self.payloads[id] }

    /// Retrieve the stored embedding for a specific id.
    ///
    /// # Panics
    /// Panics if `id >= self.len()`.
    pub fn get_embedding(&self, id: usize) -> &[f32] { self.inner.get_vector(id) }

    /// Number of vectors in the index.
    pub fn len(&self) -> usize { self.inner.len() }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool { self.inner.is_empty() }

    // ─── Persistence ──────────────────────────────────────────────────────

    /// Serialize the index and all payloads to a single binary file.
    ///
    /// The file can be reloaded with [`load`](Self::load) or
    /// [`load_mmap`](Self::load_mmap).
    ///
    /// ## File format
    /// The file uses the standard HNSW binary format (see [`persist`]) with a
    /// payload section appended after the graph.
    ///
    /// [`persist`]: crate::persist
    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        persist::save_with_payload(&self.inner, &self.payloads, path)
    }

    /// Load a labeled index from a file, copying vector data into RAM.
    ///
    /// Use this for indexes that fit comfortably in memory.
    pub fn load(path: impl AsRef<Path>, metric: D) -> io::Result<Self> {
        let (inner, payloads) = persist::load_with_payload(path, metric)?;
        Ok(Self { inner, payloads })
    }

    /// Load a labeled index with the vector section **memory-mapped**.
    ///
    /// Vector data is not copied into RAM — the OS page cache handles
    /// residency.  Ideal for indexes larger than available RAM.
    ///
    /// ## Constraints
    /// * Inserting new vectors into a mmap-backed index will **panic**.
    /// * The file must remain on disk while the index is in use.
    pub fn load_mmap(path: impl AsRef<Path>, metric: D) -> io::Result<Self> {
        let (inner, payloads) = persist::load_mmap_with_payload(path, metric)?;
        Ok(Self { inner, payloads })
    }
}

// ─── Builder extension ────────────────────────────────────────────────────────

impl Builder {
    /// Consume the builder and create an empty [`LabeledIndex`].
    ///
    /// ```rust
    /// use hnsw::{Builder, labeled::LabeledIndex};
    /// use hnsw::distance::Euclidean;
    ///
    /// let mut idx: LabeledIndex<Euclidean, u32> = Builder::new()
    ///     .m(16)
    ///     .ef_construction(200)
    ///     .build_labeled(Euclidean);
    /// idx.insert(vec![1.0, 2.0], 42_u32);
    /// ```
    pub fn build_labeled<D: Distance, L: Payload>(self, metric: D) -> LabeledIndex<D, L> {
        LabeledIndex::from_builder(self, metric)
    }
}
