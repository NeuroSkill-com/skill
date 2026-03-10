//! Typed payload stored alongside each vector in a [`LabeledIndex`] or
//! [`PairedIndex`].
//!
//! # Implementing `Payload`
//!
//! Any `Clone + Send + Sync + 'static` type can be made into a payload by
//! implementing [`Payload`]:
//!
//! ```rust
//! use hnsw::payload::{Payload, DecodeError};
//!
//! #[derive(Clone)]
//! struct MyLabel { category: u16, score: f32 }
//!
//! impl Payload for MyLabel {
//!     fn fixed_stride() -> Option<usize> { Some(6) }  // 2 + 4 bytes
//!
//!     fn encode(&self, buf: &mut Vec<u8>) {
//!         buf.extend_from_slice(&self.category.to_le_bytes());
//!         buf.extend_from_slice(&self.score.to_le_bytes());
//!     }
//!
//!     fn decode(data: &[u8]) -> Result<(Self, usize), DecodeError> {
//!         if data.len() < 6 { return Err(DecodeError("too short")); }
//!         let category = u16::from_le_bytes(data[0..2].try_into().unwrap());
//!         let score    = f32::from_le_bytes(data[2..6].try_into().unwrap());
//!         Ok((MyLabel { category, score }, 6))
//!     }
//! }
//! ```
//!
//! # Built-in implementations
//!
//! | Type | Wire format | `fixed_stride` |
//! |---|---|---|
//! | `()` | 0 bytes | `Some(0)` |
//! | `u32` / `i32` / `f32` | 4 bytes LE | `Some(4)` |
//! | `u64` / `i64` / `f64` | 8 bytes LE | `Some(8)` |
//! | `String` | `u32` length prefix + UTF-8 bytes | `None` |
//! | `Vec<u8>` | `u32` length prefix + raw bytes | `None` |
//! | `Vec<f32>` | `u32` count prefix + `f32` LE values | `None` |
//! | `(A, B)` where A, B: Payload | A bytes ++ B bytes | `Some(a+b)` if both fixed |

use std::fmt;

// ─── Error type ───────────────────────────────────────────────────────────────

/// Error returned by [`Payload::decode`] when the byte slice is malformed.
#[derive(Clone, Debug)]
pub struct DecodeError(pub &'static str);

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "payload decode error: {}", self.0)
    }
}
impl std::error::Error for DecodeError {}

// ─── Payload trait ────────────────────────────────────────────────────────────

/// Trait for values that can be stored alongside vectors in a
/// [`LabeledIndex`](crate::labeled::LabeledIndex) or
/// [`PairedIndex`](crate::paired::PairedIndex) and persisted to disk.
///
/// ## Implementing for fixed-size types
///
/// If every instance of your type encodes to exactly `N` bytes, override
/// `fixed_stride` to return `Some(N)`.  This enables a flat, seekable
/// on-disk layout with no offset table.
///
/// ## Implementing for variable-size types
///
/// Leave `fixed_stride` returning `None`.  The persistence layer will write
/// an offset table (n × 8 bytes) before the payload data, enabling O(1)
/// random access to any entry.
pub trait Payload: Clone + Send + Sync + 'static {
    /// Append the wire representation of `self` to `buf`.
    ///
    /// When [`fixed_stride`](Self::fixed_stride) is `Some(n)`, exactly `n`
    /// bytes must be appended.
    fn encode(&self, buf: &mut Vec<u8>);

    /// Parse a payload from the front of `data`.
    ///
    /// Returns `(value, bytes_consumed)`.  `bytes_consumed` is ignored for
    /// fixed-stride types (the caller already knows the width).
    fn decode(data: &[u8]) -> Result<(Self, usize), DecodeError>;

    /// If every instance of this type always encodes to the same number of
    /// bytes, return `Some(n)`.  Otherwise return `None` (default).
    fn fixed_stride() -> Option<usize> { None }
}

// ─── Built-in implementations ─────────────────────────────────────────────────

/// Unit — stores no bytes.  Useful for unlabeled indexes where you only want
/// the search functionality.
impl Payload for () {
    fn encode(&self, _buf: &mut Vec<u8>) {}
    fn decode(_data: &[u8]) -> Result<(Self, usize), DecodeError> { Ok(((), 0)) }
    fn fixed_stride() -> Option<usize> { Some(0) }
}

/// Single-precision float — 4 bytes LE.
impl Payload for f32 {
    fn encode(&self, buf: &mut Vec<u8>) { buf.extend_from_slice(&self.to_le_bytes()); }
    fn decode(data: &[u8]) -> Result<(Self, usize), DecodeError> {
        if data.len() < 4 { return Err(DecodeError("f32: too short")); }
        Ok((f32::from_le_bytes(data[..4].try_into().unwrap()), 4))
    }
    fn fixed_stride() -> Option<usize> { Some(4) }
}

/// Double-precision float — 8 bytes LE.
impl Payload for f64 {
    fn encode(&self, buf: &mut Vec<u8>) { buf.extend_from_slice(&self.to_le_bytes()); }
    fn decode(data: &[u8]) -> Result<(Self, usize), DecodeError> {
        if data.len() < 8 { return Err(DecodeError("f64: too short")); }
        Ok((f64::from_le_bytes(data[..8].try_into().unwrap()), 8))
    }
    fn fixed_stride() -> Option<usize> { Some(8) }
}

/// Unsigned 32-bit integer — 4 bytes LE.
impl Payload for u32 {
    fn encode(&self, buf: &mut Vec<u8>) { buf.extend_from_slice(&self.to_le_bytes()); }
    fn decode(data: &[u8]) -> Result<(Self, usize), DecodeError> {
        if data.len() < 4 { return Err(DecodeError("u32: too short")); }
        Ok((u32::from_le_bytes(data[..4].try_into().unwrap()), 4))
    }
    fn fixed_stride() -> Option<usize> { Some(4) }
}

/// Signed 32-bit integer — 4 bytes LE.
impl Payload for i32 {
    fn encode(&self, buf: &mut Vec<u8>) { buf.extend_from_slice(&self.to_le_bytes()); }
    fn decode(data: &[u8]) -> Result<(Self, usize), DecodeError> {
        if data.len() < 4 { return Err(DecodeError("i32: too short")); }
        Ok((i32::from_le_bytes(data[..4].try_into().unwrap()), 4))
    }
    fn fixed_stride() -> Option<usize> { Some(4) }
}

/// Unsigned 64-bit integer — 8 bytes LE.
impl Payload for u64 {
    fn encode(&self, buf: &mut Vec<u8>) { buf.extend_from_slice(&self.to_le_bytes()); }
    fn decode(data: &[u8]) -> Result<(Self, usize), DecodeError> {
        if data.len() < 8 { return Err(DecodeError("u64: too short")); }
        Ok((u64::from_le_bytes(data[..8].try_into().unwrap()), 8))
    }
    fn fixed_stride() -> Option<usize> { Some(8) }
}

/// Signed 64-bit integer — 8 bytes LE.
impl Payload for i64 {
    fn encode(&self, buf: &mut Vec<u8>) { buf.extend_from_slice(&self.to_le_bytes()); }
    fn decode(data: &[u8]) -> Result<(Self, usize), DecodeError> {
        if data.len() < 8 { return Err(DecodeError("i64: too short")); }
        Ok((i64::from_le_bytes(data[..8].try_into().unwrap()), 8))
    }
    fn fixed_stride() -> Option<usize> { Some(8) }
}

/// UTF-8 string — `u32` length prefix (4 bytes LE) + UTF-8 bytes.
///
/// Strings up to 4 GiB are supported.
impl Payload for String {
    fn encode(&self, buf: &mut Vec<u8>) {
        let bytes = self.as_bytes();
        buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(bytes);
    }
    fn decode(data: &[u8]) -> Result<(Self, usize), DecodeError> {
        if data.len() < 4 { return Err(DecodeError("String: too short for length")); }
        let len = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
        if data.len() < 4 + len { return Err(DecodeError("String: truncated data")); }
        let s = std::str::from_utf8(&data[4..4 + len])
            .map_err(|_| DecodeError("String: invalid UTF-8"))?
            .to_owned();
        Ok((s, 4 + len))
    }
}

/// Raw byte buffer — `u32` length prefix (4 bytes LE) + raw bytes.
impl Payload for Vec<u8> {
    fn encode(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&(self.len() as u32).to_le_bytes());
        buf.extend_from_slice(self);
    }
    fn decode(data: &[u8]) -> Result<(Self, usize), DecodeError> {
        if data.len() < 4 { return Err(DecodeError("Vec<u8>: too short for length")); }
        let len = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
        if data.len() < 4 + len { return Err(DecodeError("Vec<u8>: truncated data")); }
        Ok((data[4..4 + len].to_vec(), 4 + len))
    }
}

/// Float vector — `u32` element-count prefix (4 bytes LE) + `f32` LE values.
///
/// Useful as a second embedding paired alongside a primary one, when the
/// second embedding has variable dimension.  For fixed-dimension embeddings
/// prefer `[f32; N]`.
impl Payload for Vec<f32> {
    fn encode(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&(self.len() as u32).to_le_bytes());
        for &v in self {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    fn decode(data: &[u8]) -> Result<(Self, usize), DecodeError> {
        if data.len() < 4 { return Err(DecodeError("Vec<f32>: too short for count")); }
        let count = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
        let needed = 4 + count * 4;
        if data.len() < needed { return Err(DecodeError("Vec<f32>: truncated data")); }
        let floats = data[4..needed]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        Ok((floats, needed))
    }
}

/// Pair of payloads — encodes A then B sequentially.
///
/// `fixed_stride` returns `Some(a + b)` only when both `A` and `B` are
/// fixed-stride.  Otherwise returns `None` (variable-width).
impl<A: Payload, B: Payload> Payload for (A, B) {
    fn encode(&self, buf: &mut Vec<u8>) {
        self.0.encode(buf);
        self.1.encode(buf);
    }
    fn decode(data: &[u8]) -> Result<(Self, usize), DecodeError> {
        let (a, na) = A::decode(data)?;
        let (b, nb) = B::decode(&data[na..])?;
        Ok(((a, b), na + nb))
    }
    fn fixed_stride() -> Option<usize> {
        match (A::fixed_stride(), B::fixed_stride()) {
            (Some(a), Some(b)) => Some(a + b),
            _ => None,
        }
    }
}
