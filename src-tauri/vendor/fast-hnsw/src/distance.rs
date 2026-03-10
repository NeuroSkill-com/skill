/// A distance (or dissimilarity) between two vectors.
/// Lower is closer.
pub trait Distance: Send + Sync + 'static {
    fn distance(&self, a: &[f32], b: &[f32]) -> f32;
}

// ─── Built-in metrics ────────────────────────────────────────────────────────

/// Squared Euclidean distance  (avoids a sqrt; preserves nearest-neighbour order).
#[derive(Clone, Copy, Debug, Default)]
pub struct SquaredEuclidean;

impl Distance for SquaredEuclidean {
    #[inline]
    fn distance(&self, a: &[f32], b: &[f32]) -> f32 {
        a.iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y) * (x - y))
            .sum()
    }
}

/// True Euclidean (L2) distance.
#[derive(Clone, Copy, Debug, Default)]
pub struct Euclidean;

impl Distance for Euclidean {
    #[inline]
    fn distance(&self, a: &[f32], b: &[f32]) -> f32 {
        a.iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y) * (x - y))
            .sum::<f32>()
            .sqrt()
    }
}

/// Cosine distance  = 1 − cosine_similarity ∈ [0, 2].
/// Works correctly only for non-zero vectors.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cosine;

impl Distance for Cosine {
    #[inline]
    fn distance(&self, a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if na == 0.0 || nb == 0.0 {
            return 1.0;
        }
        // clamp to [−1, 1] before subtracting to absorb float rounding
        let cos = (dot / (na * nb)).clamp(-1.0, 1.0);
        1.0 - cos
    }
}

/// Inner-product distance  = 1 − dot(a, b).
/// Useful for pre-normalised embeddings (equals cosine distance when ‖a‖=‖b‖=1).
#[derive(Clone, Copy, Debug, Default)]
pub struct DotProduct;

impl Distance for DotProduct {
    #[inline]
    fn distance(&self, a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        1.0 - dot
    }
}

/// Manhattan (L1) distance.
#[derive(Clone, Copy, Debug, Default)]
pub struct Manhattan;

impl Distance for Manhattan {
    #[inline]
    fn distance(&self, a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(x, y)| (x - y).abs()).sum()
    }
}
