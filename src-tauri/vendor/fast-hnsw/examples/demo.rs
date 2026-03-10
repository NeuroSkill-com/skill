//! A small end-to-end demo of the HNSW index.
//!
//! Run with:
//!   cargo run --example demo

use fast_hnsw::{Builder, Hnsw};
use fast_hnsw::distance::{Cosine, Euclidean};

fn main() {
    println!("╔══════════════════════════════════════════════════════╗");
    println!("║              HNSW Demo (pure Rust)                   ║");
    println!("╚══════════════════════════════════════════════════════╝\n");

    // ── Example 1: 2-D Euclidean ────────────────────────────────────────
    println!("▶  Example 1 — 2-D Euclidean nearest neighbours");
    {
        let mut index: Hnsw<Euclidean> = Builder::new().m(4).ef_construction(20).seed(0).build(Euclidean);

        // Insert a 10×10 grid.
        for x in 0..10_u32 {
            for y in 0..10_u32 {
                index.insert(vec![x as f32, y as f32]);
            }
        }

        let query = [4.6f32, 6.2];
        let results = index.search(&query, 5, 50);
        println!("   Index contains {} vectors (10×10 grid)", index.len());
        println!("   Query: {:?}", query);
        println!("   Top-5 nearest (id, dist):");
        for r in &results {
            let v = index.get_vector(r.id);
            println!("     id={:>3}  vec={:>6?}  dist={:.4}", r.id, v, r.distance);
        }
        println!();
    }

    // ── Example 2: cosine similarity on word-like unit vectors ──────────
    println!("▶  Example 2 — Cosine similarity");
    {
        let words: &[(&str, [f32; 4])] = &[
            ("king",   [0.9,  0.1,  0.05, 0.0]),
            ("queen",  [0.85, 0.1,  0.0,  0.15]),
            ("man",    [0.8,  0.05, 0.0,  0.0]),
            ("woman",  [0.75, 0.05, 0.0,  0.2]),
            ("cat",    [0.0,  0.9,  0.1,  0.0]),
            ("dog",    [0.0,  0.85, 0.15, 0.0]),
            ("kitten", [0.0,  0.95, 0.05, 0.0]),
        ];

        let mut index: Hnsw<Cosine> = Builder::new().seed(1).build(Cosine);
        for (_, v) in words {
            index.insert(v.to_vec());
        }

        let query = &[0.0f32, 0.88, 0.12, 0.0]; // close to "cat"/"dog"
        println!("   Query vector: {:?}", query);
        let results = index.search(query, 3, 20);
        println!("   Top-3 by cosine similarity:");
        for r in &results {
            println!(
                "     {:>8}  cosine_dist={:.4}",
                words[r.id].0, r.distance
            );
        }
        println!();
    }

    // ── Example 3: index statistics ─────────────────────────────────────
    println!("▶  Example 3 — Index statistics");
    {
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::SmallRng::seed_from_u64(99);
        let mut index: Hnsw<Euclidean> = Builder::new().m(16).ef_construction(200).seed(7).build(Euclidean);

        for _ in 0..2_000 {
            let v: Vec<f32> = (0..64).map(|_| rng.gen::<f32>()).collect();
            index.insert(v);
        }

        println!("{}", index.stats());
    }
}
