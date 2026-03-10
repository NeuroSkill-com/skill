//! Priority-queue helpers used by the HNSW search routines.
#![allow(dead_code)]
//!
//! HNSW needs two kinds of heaps that operate on (distance, node-id) pairs:
//!
//!  * **`CandidateHeap`** – a *min*-heap (pop returns the closest element).
//!    Used for the candidate set `C`.
//!  * **`ResultHeap`** – a *max*-heap (pop returns the *farthest* element).
//!    Used for the dynamic nearest-neighbour list `W`.
//!
//! Rust's `BinaryHeap` is a max-heap, so we get a min-heap via `Reverse`.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

// ─── Ordered pair ────────────────────────────────────────────────────────────

/// A (distance, node-id) pair with a total order on `distance`.
/// NaN distances are treated as larger than any finite value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistId {
    pub dist: f32,
    pub id: usize,
}

impl DistId {
    #[inline]
    pub fn new(dist: f32, id: usize) -> Self {
        Self { dist, id }
    }
}

impl Eq for DistId {}

impl PartialOrd for DistId {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// Max-heap order: larger distance = higher priority.
impl Ord for DistId {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        // total_cmp gives a proper total order including NaN (NaN > everything).
        self.dist
            .total_cmp(&other.dist)
            .then_with(|| self.id.cmp(&other.id))
    }
}

// ─── Min-heap wrapper (candidates) ───────────────────────────────────────────

/// Min-heap of `(dist, id)` – the *nearest* candidate is at the top.
#[derive(Default)]
pub struct CandidateHeap(BinaryHeap<std::cmp::Reverse<DistId>>);

impl CandidateHeap {
    pub fn new() -> Self {
        Self(BinaryHeap::new())
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self(BinaryHeap::with_capacity(cap))
    }

    #[inline]
    pub fn push(&mut self, dist: f32, id: usize) {
        self.0.push(std::cmp::Reverse(DistId::new(dist, id)));
    }

    /// Pop the closest element.
    #[inline]
    pub fn pop(&mut self) -> Option<DistId> {
        self.0.pop().map(|std::cmp::Reverse(x)| x)
    }

    /// Peek at the closest element without removing it.
    #[inline]
    pub fn peek_dist(&self) -> Option<f32> {
        self.0.peek().map(|std::cmp::Reverse(x)| x.dist)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

// ─── Max-heap wrapper (results) ──────────────────────────────────────────────

/// Max-heap of `(dist, id)` – the *farthest* element is at the top.
/// Bounded to at most `capacity` entries; dropping the farthest when full.
pub struct ResultHeap {
    inner: BinaryHeap<DistId>,
    capacity: usize,
}

impl ResultHeap {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: BinaryHeap::with_capacity(capacity + 1),
            capacity,
        }
    }

    /// Push a candidate, evicting the farthest if the heap exceeds `capacity`.
    #[inline]
    pub fn push(&mut self, dist: f32, id: usize) {
        self.inner.push(DistId::new(dist, id));
        if self.inner.len() > self.capacity {
            self.inner.pop(); // remove farthest
        }
    }

    /// Peek at the *worst* (farthest) distance currently in the result set.
    #[inline]
    pub fn peek_worst_dist(&self) -> Option<f32> {
        self.inner.peek().map(|x| x.dist)
    }

    /// Pop the farthest element.
    #[inline]
    pub fn pop_worst(&mut self) -> Option<DistId> {
        self.inner.pop()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Drain all results into a `Vec`, sorted closest-first.
    pub fn into_sorted_vec(self) -> Vec<DistId> {
        // `BinaryHeap::into_sorted_vec` runs heapsort and returns elements in
        // ascending order according to `Ord`.  For `DistId`, smaller dist is
        // Less, so ascending = closest first — exactly what we want.
        self.inner.into_sorted_vec()
    }

    /// Iterate without consuming (unspecified order).
    pub fn iter(&self) -> impl Iterator<Item = &DistId> {
        self.inner.iter()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_heap_min_order() {
        let mut h = CandidateHeap::new();
        h.push(3.0, 3);
        h.push(1.0, 1);
        h.push(2.0, 2);
        assert_eq!(h.pop().unwrap().dist, 1.0);
        assert_eq!(h.pop().unwrap().dist, 2.0);
        assert_eq!(h.pop().unwrap().dist, 3.0);
        assert!(h.pop().is_none());
    }

    #[test]
    fn result_heap_bounded() {
        let mut h = ResultHeap::new(3);
        for i in 0..6_u32 {
            h.push(i as f32, i as usize);
        }
        // Should keep the 3 *closest* (0, 1, 2)
        assert_eq!(h.len(), 3);
        let v = h.into_sorted_vec();
        assert_eq!(v[0].dist, 0.0);
        assert_eq!(v[1].dist, 1.0);
        assert_eq!(v[2].dist, 2.0);
    }
}
