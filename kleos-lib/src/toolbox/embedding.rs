//! Embedding blob encoding and similarity for the toolbox.
//!
//! The catalog is human-sized (hundreds, not millions, of tools), so the vector
//! channel is a plain linear cosine scan in Rust over the rows the caller owns.
//! No sqlite-vec column, no ANN index, nothing to keep in sync.
//!
//! The on-disk encoding is the same little-endian f32 sequence the memory tables
//! use. These two helpers are deliberately duplicated rather than lifted out of
//! `memory` into a shared public API: widening an upstream module's surface is a
//! rebase cost, four lines of arithmetic are not.

/// Encode an embedding as a little-endian f32 blob.
pub fn to_blob(embedding: &[f32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(embedding.len() * 4);
    for &f in embedding {
        buf.extend_from_slice(&f.to_le_bytes());
    }
    buf
}

/// Decode a little-endian f32 blob. Trailing bytes that do not complete a f32
/// are ignored (a truncated blob degrades to a shorter vector, which the
/// dimension check in [`cosine`] then rejects).
pub fn from_blob(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// Cosine similarity in `[-1, 1]`, or `None` when the vectors are empty, of
/// different dimensions, or one of them is all zeros.
pub fn cosine(a: &[f32], b: &[f32]) -> Option<f64> {
    if a.is_empty() || a.len() != b.len() {
        return None;
    }
    let mut dot = 0.0f64;
    let mut na = 0.0f64;
    let mut nb = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += (*x as f64) * (*y as f64);
        na += (*x as f64) * (*x as f64);
        nb += (*y as f64) * (*y as f64);
    }
    if na == 0.0 || nb == 0.0 {
        return None;
    }
    Some(dot / (na.sqrt() * nb.sqrt()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_roundtrip_preserves_values() {
        let v = vec![0.5f32, -1.25, 0.0, 3.75];
        assert_eq!(from_blob(&to_blob(&v)), v);
    }

    #[test]
    fn cosine_is_one_for_identical_vectors() {
        let v = vec![1.0f32, 2.0, 3.0];
        let s = cosine(&v, &v).unwrap();
        assert!((s - 1.0).abs() < 1e-9, "got {s}");
    }

    #[test]
    fn cosine_rejects_mismatched_or_degenerate_vectors() {
        assert!(cosine(&[1.0, 0.0], &[1.0, 0.0, 0.0]).is_none());
        assert!(cosine(&[], &[]).is_none());
        assert!(cosine(&[0.0, 0.0], &[1.0, 1.0]).is_none());
    }

    #[test]
    fn cosine_orders_orthogonal_below_aligned() {
        let q = vec![1.0f32, 0.0];
        assert!(cosine(&q, &[1.0, 0.0]).unwrap() > cosine(&q, &[0.0, 1.0]).unwrap());
    }
}
