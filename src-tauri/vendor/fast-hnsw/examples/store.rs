//! Demonstrates persistence, `LabeledIndex`, and `PairedIndex`.
//!
//! Run with:
//!   cargo run --release --example store

use std::path::PathBuf;

use fast_hnsw::distance::{Cosine, Euclidean};
use fast_hnsw::labeled::LabeledIndex;
use fast_hnsw::paired::PairedIndex;
use fast_hnsw::{Builder, persist};
use fast_hnsw::payload::Payload;

// ─── Custom payload ───────────────────────────────────────────────────────────

/// A structured label: a class id (u32) and a confidence score (f32).
/// Total size = 4 + 4 = 8 bytes → fixed_stride = Some(8).
#[derive(Clone, Debug, PartialEq)]
struct ClassLabel {
    class_id:   u32,
    confidence: f32,
}

impl Payload for ClassLabel {
    fn fixed_stride() -> Option<usize> { Some(8) }

    fn encode(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.class_id.to_le_bytes());
        buf.extend_from_slice(&self.confidence.to_le_bytes());
    }

    fn decode(data: &[u8]) -> Result<(Self, usize), fast_hnsw::payload::DecodeError> {
        if data.len() < 8 {
            return Err(fast_hnsw::payload::DecodeError("ClassLabel: too short"));
        }
        let class_id   = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let confidence = f32::from_le_bytes(data[4..8].try_into().unwrap());
        Ok((ClassLabel { class_id, confidence }, 8))
    }
}

fn main() {
    let tmp = std::env::temp_dir().join("hnsw_store_demo");
    std::fs::create_dir_all(&tmp).unwrap();

    demo_bare_persist(&tmp);
    demo_labeled_u32(&tmp);
    demo_labeled_string(&tmp);
    demo_labeled_custom(&tmp);
    demo_labeled_mmap(&tmp);
    demo_paired_index(&tmp);
    demo_paired_mmap(&tmp);

    println!("\n✓ All demos passed.");
}

// ─── 1. Bare Hnsw save / load ─────────────────────────────────────────────────

fn demo_bare_persist(tmp: &PathBuf) {
    println!("\n=== 1. Bare Hnsw persist (owned load) ===");

    let mut index = Builder::new().m(16).ef_construction(100).seed(1).build(Euclidean);
    for i in 0..100_u32 {
        index.insert(vec![i as f32, (i * i) as f32]);
    }

    let path = tmp.join("bare.hnsw");
    persist::save(&index, &path).expect("save failed");
    println!("  saved  {} bytes", std::fs::metadata(&path).unwrap().len());

    let loaded = persist::load(&path, Euclidean).expect("load failed");
    assert_eq!(index.len(), loaded.len());

    let q = vec![49.5f32, 2450.0];
    let r_orig   = index.search(&q, 3, 50);
    let r_loaded = loaded.search(&q, 3, 50);
    assert_eq!(r_orig.iter().map(|r| r.id).collect::<Vec<_>>(),
               r_loaded.iter().map(|r| r.id).collect::<Vec<_>>());
    println!("  nearest ids: {:?}", r_orig.iter().map(|r| r.id).collect::<Vec<_>>());
    println!("  round-trip: ✓");
}

// ─── 2. LabeledIndex<u32> save / load ────────────────────────────────────────

fn demo_labeled_u32(tmp: &PathBuf) {
    println!("\n=== 2. LabeledIndex<u32> (classification label) ===");

    let mut idx: LabeledIndex<Euclidean, u32> = Builder::new()
        .m(16).ef_construction(100).seed(2)
        .build_labeled(Euclidean);

    // Three clusters: class 0 = [0,0], class 1 = [10,10], class 2 = [20,0]
    let data = [
        ([0.1, 0.2], 0_u32),
        ([0.3, 0.1], 0),
        ([10.1, 9.9], 1),
        ([10.2, 10.1], 1),
        ([19.9, 0.1], 2),
        ([20.1, 0.2], 2),
    ];
    for (emb, label) in data {
        idx.insert(emb.to_vec(), label);
    }

    let path = tmp.join("class.hnsw");
    idx.save(&path).expect("save failed");

    let loaded = LabeledIndex::<Euclidean, u32>::load(&path, Euclidean).expect("load failed");
    for q in [[0.0f32, 0.0], [10.0, 10.0], [20.0, 0.0]] {
        let hits = loaded.search(&q, 1, 20);
        println!("  query {:?} → class {} (dist={:.3})", q, hits[0].payload, hits[0].distance);
    }
}

// ─── 3. LabeledIndex<String> save / load ─────────────────────────────────────

fn demo_labeled_string(tmp: &PathBuf) {
    println!("\n=== 3. LabeledIndex<String> (text tag) ===");

    let mut idx: LabeledIndex<Cosine, String> = Builder::new()
        .m(16).ef_construction(100).seed(3)
        .build_labeled(Cosine);

    let items = [
        ([1.0f32, 0.0, 0.0, 0.0], "technology"),
        ([0.0,    1.0, 0.0, 0.0], "science"),
        ([0.0,    0.0, 1.0, 0.0], "sports"),
        ([0.0,    0.0, 0.0, 1.0], "politics"),
        ([0.8,    0.2, 0.0, 0.0], "tech startup"),
    ];
    for (emb, tag) in items {
        idx.insert(emb.to_vec(), tag.to_string());
    }

    let path = tmp.join("tags.hnsw");
    idx.save(&path).expect("save failed");
    let loaded = LabeledIndex::<Cosine, String>::load(&path, Cosine).expect("load failed");

    let q = vec![0.9f32, 0.1, 0.0, 0.0];
    let hits = loaded.search(&q, 2, 20);
    print!("  query [tech-like] → ");
    for h in &hits { print!("{:?} (dist={:.3})  ", h.payload, h.distance); }
    println!();
}

// ─── 4. LabeledIndex<ClassLabel> (custom payload) ────────────────────────────

fn demo_labeled_custom(tmp: &PathBuf) {
    println!("\n=== 4. LabeledIndex<ClassLabel> (custom fixed-stride payload) ===");

    let mut idx: LabeledIndex<Euclidean, ClassLabel> = Builder::new()
        .m(16).ef_construction(100).seed(4)
        .build_labeled(Euclidean);

    idx.insert(vec![1.0, 0.0], ClassLabel { class_id: 0, confidence: 0.95 });
    idx.insert(vec![0.0, 1.0], ClassLabel { class_id: 1, confidence: 0.88 });
    idx.insert(vec![0.5, 0.5], ClassLabel { class_id: 2, confidence: 0.72 });

    let path = tmp.join("custom.hnsw");
    idx.save(&path).expect("save failed");
    let loaded = LabeledIndex::<Euclidean, ClassLabel>::load(&path, Euclidean)
        .expect("load failed");

    let hits = loaded.search(&[0.9f32, 0.1], 1, 20);
    let lbl = hits[0].payload;
    println!("  nearest: class={} confidence={:.2}", lbl.class_id, lbl.confidence);
    assert_eq!(lbl.class_id, 0);
}

// ─── 5. LabeledIndex mmap load ───────────────────────────────────────────────

fn demo_labeled_mmap(tmp: &PathBuf) {
    println!("\n=== 5. LabeledIndex::load_mmap (zero-copy vector data) ===");

    let mut idx: LabeledIndex<Euclidean, String> = Builder::new()
        .m(16).ef_construction(100).seed(5)
        .build_labeled(Euclidean);
    for i in 0..200_u32 {
        idx.insert(vec![i as f32, (i % 10) as f32], format!("item-{i}"));
    }

    let path = tmp.join("mmap.hnsw");
    idx.save(&path).expect("save failed");

    // Load with mmap: vector section stays page-cache backed, not copied.
    let mmap = LabeledIndex::<Euclidean, String>::load_mmap(&path, Euclidean)
        .expect("mmap load failed");
    let hits = mmap.search(&[100.0f32, 0.0], 3, 50);
    print!("  nearest labels: ");
    for h in &hits { print!("{} ", h.payload); }
    println!();
    println!("  (vectors accessed via OS page cache — no RAM copy)");
}

// ─── 6. PairedIndex (text + image embeddings) ─────────────────────────────────

fn demo_paired_index(tmp: &PathBuf) {
    println!("\n=== 6. PairedIndex<Cosine, Euclidean> (text ↔ image) ===");

    // Side A: 4-D text embeddings (Cosine similarity)
    // Side B: 3-D image embeddings (Euclidean distance)
    let mut idx: PairedIndex<Cosine, Euclidean> = Builder::new()
        .m(16).ef_construction(100).seed(6)
        .build_paired(Cosine, Euclidean);

    let pairs = [
        // (text_emb,                     image_emb,           description)
        ([1.0f32, 0.0, 0.0, 0.0],  [0.9, 0.1, 0.0],   "cat"),
        ([0.0,    1.0, 0.0, 0.0],  [0.1, 0.8, 0.1],   "dog"),
        ([0.0,    0.0, 1.0, 0.0],  [0.0, 0.2, 0.9],   "fish"),
        ([0.0,    0.0, 0.0, 1.0],  [0.5, 0.5, 0.0],   "bird"),
        ([0.7,    0.3, 0.0, 0.0],  [0.8, 0.2, 0.0],   "kitten"),
    ];
    let names = pairs.map(|(_, _, n)| n);
    for (ta, ib, _) in pairs {
        idx.insert(ta.to_vec(), ib.to_vec());
    }

    let path = tmp.join("paired");
    idx.save(&path).expect("save failed");
    let loaded = PairedIndex::<Cosine, Euclidean>::load(&path, Cosine, Euclidean)
        .expect("load failed");

    // Use a text query → find nearest items → also get their image embeddings.
    let text_q = vec![0.8f32, 0.2, 0.0, 0.0]; // "something cat-like"
    println!("  Text query [cat-like]:");
    for hit in loaded.search_by_a(&text_q, 3, 20) {
        println!("    id={} text_dist={:.3} name={:?} img_emb={:?}",
                 hit.id, hit.distance, names[hit.id], hit.emb_b);
    }

    // Use an image query → find nearest items → also get their text embeddings.
    let img_q = vec![0.05f32, 0.85, 0.1]; // "dog-like image"
    println!("  Image query [dog-like]:");
    for hit in loaded.search_by_b(&img_q, 2, 20) {
        println!("    id={} img_dist={:.3} name={:?} text_emb={:?}",
                 hit.id, hit.distance, names[hit.id], hit.emb_a);
    }
}

// ─── 7. PairedIndex mmap load ─────────────────────────────────────────────────

fn demo_paired_mmap(tmp: &PathBuf) {
    println!("\n=== 7. PairedIndex::load_mmap ===");

    let mut idx: PairedIndex<Euclidean, Euclidean> = Builder::new()
        .m(16).ef_construction(50).seed(7)
        .build_paired(Euclidean, Euclidean);
    for i in 0..100_u32 {
        idx.insert(vec![i as f32], vec![i as f32, (i % 5) as f32]);
    }

    let path = tmp.join("paired_mmap");
    idx.save(&path).expect("save failed");

    let m = PairedIndex::<Euclidean, Euclidean>::load_mmap(&path, Euclidean, Euclidean)
        .expect("mmap load failed");
    println!("  len={}, both sides mmap'd", m.len());

    let hits = m.search_by_a(&[50.0f32], 1, 30);
    assert_eq!(hits[0].id, 50);
    println!("  search_by_a(50.0) → id={} emb_b={:?}", hits[0].id, hits[0].emb_b);

    let hits = m.search_by_b(&[50.0f32, 0.0], 1, 30);
    assert_eq!(hits[0].id, 50);
    println!("  search_by_b([50,0]) → id={} emb_a={:?}", hits[0].id, hits[0].emb_a);
}
