use brainjepa::EmbeddingResult;
use std::io::Read;

fn fake_embedding_result(n_rois: usize, n_time: usize, embed_dim: usize) -> EmbeddingResult {
    let n_patches = n_rois * n_time;
    let embeddings: Vec<f32> = (0..n_patches * embed_dim).map(|i| i as f32 * 0.1).collect();
    EmbeddingResult {
        embeddings,
        shape: vec![n_patches, embed_dim],
        n_rois,
        n_time_patches: n_time,
        ms_encode: 42.0,
    }
}

fn read_file(path: &str) -> String {
    let mut s = String::new();
    std::fs::File::open(path)
        .unwrap()
        .read_to_string(&mut s)
        .unwrap();
    s
}

// ── save_embeddings_csv ──────────────────────────────────────────────────────

#[test]
fn csv_export_header_and_row_count() {
    let result = fake_embedding_result(3, 2, 4);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.csv");
    let path_str = path.to_str().unwrap();

    brainjepa::save_embeddings_csv(&result, path_str).unwrap();

    let content = read_file(path_str);
    let lines: Vec<&str> = content.lines().collect();

    // Header
    assert_eq!(lines[0], "dim_0,dim_1,dim_2,dim_3");

    // 3 ROIs * 2 time patches = 6 data rows + 1 header
    assert_eq!(lines.len(), 7);
}

#[test]
fn csv_export_values_correct() {
    let result = fake_embedding_result(2, 1, 3);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vals.csv");
    let path_str = path.to_str().unwrap();

    brainjepa::save_embeddings_csv(&result, path_str).unwrap();

    let content = read_file(path_str);
    let lines: Vec<&str> = content.lines().collect();

    // First data row: 0*0.1, 1*0.1, 2*0.1
    let row1: Vec<f32> = lines[1].split(',').map(|s| s.parse().unwrap()).collect();
    assert_eq!(row1.len(), 3);
    assert!((row1[0] - 0.0).abs() < 1e-6);
    assert!((row1[1] - 0.1).abs() < 1e-6);
    assert!((row1[2] - 0.2).abs() < 1e-6);
}

#[test]
fn csv_export_single_patch() {
    let result = fake_embedding_result(1, 1, 2);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("single.csv");
    let path_str = path.to_str().unwrap();

    brainjepa::save_embeddings_csv(&result, path_str).unwrap();

    let content = read_file(path_str);
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2); // header + 1 row
}

// ── save_embeddings_csv_with_metadata ────────────────────────────────────────

#[test]
fn csv_with_metadata_has_roi_and_time_columns() {
    let result = fake_embedding_result(3, 2, 4);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("meta.csv");
    let path_str = path.to_str().unwrap();

    brainjepa::csv_export::save_embeddings_csv_with_metadata(&result, path_str, 3, 2).unwrap();

    let content = read_file(path_str);
    let lines: Vec<&str> = content.lines().collect();

    // Header should start with roi_idx,time_idx
    assert!(
        lines[0].starts_with("roi_idx,time_idx,"),
        "header missing metadata columns: {}",
        lines[0]
    );
    assert_eq!(lines[0], "roi_idx,time_idx,dim_0,dim_1,dim_2,dim_3");

    // 6 data rows + 1 header
    assert_eq!(lines.len(), 7);
}

#[test]
fn csv_with_metadata_roi_time_values() {
    let result = fake_embedding_result(2, 3, 2);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("meta2.csv");
    let path_str = path.to_str().unwrap();

    brainjepa::csv_export::save_embeddings_csv_with_metadata(&result, path_str, 2, 3).unwrap();

    let content = read_file(path_str);
    let lines: Vec<&str> = content.lines().collect();

    // Expected row order: (roi=0,t=0), (roi=0,t=1), (roi=0,t=2), (roi=1,t=0), ...
    let expected_pairs = vec![(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2)];
    for (i, &(roi, time)) in expected_pairs.iter().enumerate() {
        let cols: Vec<&str> = lines[i + 1].split(',').collect();
        assert_eq!(
            cols[0].parse::<usize>().unwrap(),
            roi,
            "row {i}: expected roi_idx={roi}"
        );
        assert_eq!(
            cols[1].parse::<usize>().unwrap(),
            time,
            "row {i}: expected time_idx={time}"
        );
    }
}

#[test]
fn csv_with_metadata_column_count() {
    let embed_dim = 5;
    let result = fake_embedding_result(2, 2, embed_dim);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cols.csv");
    let path_str = path.to_str().unwrap();

    brainjepa::csv_export::save_embeddings_csv_with_metadata(&result, path_str, 2, 2).unwrap();

    let content = read_file(path_str);
    let lines: Vec<&str> = content.lines().collect();

    // Each row should have embed_dim + 2 columns (roi_idx, time_idx, dim_0..dim_4)
    for line in &lines {
        let ncols = line.split(',').count();
        assert_eq!(ncols, embed_dim + 2, "wrong column count in: {line}");
    }
}
