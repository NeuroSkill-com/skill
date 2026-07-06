//! CSV export utilities for Brain-JEPA embeddings.

use crate::EmbeddingResult;
use std::io::Write;

/// Write embeddings to a CSV file.
///
/// Rows correspond to patches, columns to embedding dimensions.
/// The header row is: `dim_0,dim_1,...,dim_{embed_dim-1}`.
///
/// # Errors
///
/// Returns an error if the file cannot be created or written.
pub fn save_embeddings_csv(result: &EmbeddingResult, path: &str) -> anyhow::Result<()> {
    let embed_dim = result.embed_dim();
    let n_patches = result.n_patches();

    let mut file = std::fs::File::create(path)?;

    // Header
    let header: Vec<String> = (0..embed_dim).map(|i| format!("dim_{i}")).collect();
    writeln!(file, "{}", header.join(","))?;

    // Data rows
    for row in 0..n_patches {
        let start = row * embed_dim;
        let end = start + embed_dim;
        let vals: Vec<String> = result.embeddings[start..end]
            .iter()
            .map(|v| v.to_string())
            .collect();
        writeln!(file, "{}", vals.join(","))?;
    }

    Ok(())
}

/// Write embeddings to a CSV file with additional `roi_idx` and `time_idx` columns.
///
/// Patches are ordered as `roi_idx * n_time_patches + time_idx` (row-major).
/// The header row is: `roi_idx,time_idx,dim_0,dim_1,...,dim_{embed_dim-1}`.
///
/// # Errors
///
/// Returns an error if the file cannot be created or written.
pub fn save_embeddings_csv_with_metadata(
    result: &EmbeddingResult,
    path: &str,
    n_rois: usize,
    n_time_patches: usize,
) -> anyhow::Result<()> {
    let embed_dim = result.embed_dim();

    let mut file = std::fs::File::create(path)?;

    // Header
    let dim_cols: Vec<String> = (0..embed_dim).map(|i| format!("dim_{i}")).collect();
    writeln!(file, "roi_idx,time_idx,{}", dim_cols.join(","))?;

    // Data rows
    let mut row = 0;
    for roi in 0..n_rois {
        for t in 0..n_time_patches {
            let start = row * embed_dim;
            let end = start + embed_dim;
            let vals: Vec<String> = result.embeddings[start..end]
                .iter()
                .map(|v| v.to_string())
                .collect();
            writeln!(file, "{roi},{t},{}", vals.join(","))?;
            row += 1;
        }
    }

    Ok(())
}
