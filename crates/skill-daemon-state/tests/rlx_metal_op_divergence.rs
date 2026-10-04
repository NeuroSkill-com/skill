// SPDX-License-Identifier: GPL-3.0-only
//! Find the first op where Metal and CPU disagree on the text-embedding graph.
//!
//! `rlx_semantic` tells you the embeddings are wrong; it cannot tell you where.
//! This exposes every compute node of the real nomic-bert graph as a graph
//! output, runs the identical graph on CPU and on Metal, and reports the first
//! node whose tensors diverge — which is the op to go and read.
//!
//! Kept because this is the only tool in the tree that answers "which op" for a
//! backend numerics regression, and the rlx pin moves on every CI build (see
//! .github/actions/refresh-rlx-main), so the question recurs.
//!
//! Run:
//! ```sh
//! cargo test -p skill-daemon-state --release --test rlx_metal_op_divergence \
//!   --features text-embeddings-rlx-metal -- --nocapture
//! ```
#![cfg(feature = "text-embeddings-rlx")]

use rlx::ir::{Graph, NodeId};
use std::collections::HashMap;

/// Rebuild `graph` with every compute node also marked as an output, in node
/// order. Mirrors `rlx_opscope::parity::expose_all_ops`, inlined so this test
/// does not pull the tooling crate into skill's dependency graph.
fn expose_all_ops(graph: &Graph) -> (Graph, Vec<NodeId>) {
    let mut g = Graph::new(&graph.name);
    let mut id_map: HashMap<NodeId, NodeId> = HashMap::new();
    let mut exposed = Vec::new();
    let mut original = Vec::new();
    for node in graph.nodes() {
        let inputs: Vec<NodeId> = node.inputs.iter().map(|i| id_map[i]).collect();
        let new_id = g.add_node(node.op.clone(), inputs, node.shape.clone());
        id_map.insert(node.id, new_id);
        // Compute nodes only — Input/Param/Constant have no inputs and carry no
        // computation to compare.
        if !node.inputs.is_empty() {
            exposed.push(new_id);
            original.push(node.id);
        }
    }
    g.set_outputs(exposed);
    (g, original)
}

fn max_abs_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0f32, f32::max)
}

#[test]
fn report_first_metal_cpu_divergence() {
    if !rlx::runtime::is_available(rlx::Device::Metal) {
        eprintln!("skipping: Metal not available in this build");
        return;
    }

    let Some((graph, params, seq)) = build_embedding_graph() else {
        eprintln!("skipping: could not obtain the nomic-bert graph (network or cache?)");
        return;
    };

    let (exposed, original_ids) = expose_all_ops(&graph);
    println!("graph '{}': {} compute nodes exposed", graph.name, original_ids.len());

    // Same token ids on both devices; the ids themselves do not matter, only
    // that the two runs see identical input.
    let ids: Vec<f32> = (0..seq).map(|i| (i % 1000) as f32 + 1.0).collect();
    let mask: Vec<f32> = vec![1.0; seq];

    let run = |device: rlx::Device| -> Vec<Vec<f32>> {
        let session = rlx::runtime::Session::new_with_precision(device, rlx::runtime::Precision::F16);
        let mut compiled = session.compile(exposed.clone());
        for (name, data) in &params {
            compiled.set_param(name, data);
        }
        compiled.run(&[("input_ids", ids.as_slice()), ("attention_mask", mask.as_slice())])
    };

    let cpu = run(rlx::Device::Cpu);
    let metal = run(rlx::Device::Metal);

    assert_eq!(cpu.len(), metal.len(), "output count differs between devices");

    // f16 compute legitimately differs from f32 in the last couple of digits,
    // so report the first op that is wrong by more than rounding — scaled to
    // the magnitude of the tensor so a large activation is not flagged for a
    // relatively tiny absolute error.
    let mut first_bad: Option<(usize, f32, f32)> = None;
    let mut unverifiable = 0usize;
    for (i, (c, m)) in cpu.iter().zip(&metal).enumerate() {
        if c.len() != m.len() {
            println!("op #{i}: LENGTH differs — cpu {} vs metal {}", c.len(), m.len());
            first_bad = Some((i, f32::INFINITY, 0.0));
            break;
        }
        let peak = c.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
        // Exposing every node as an output is not free on the CPU backend: a
        // node it computes in place or folds away never materialises its own
        // slot, and reads back as all-zero. There is no reference to compare
        // against for those, and calling the difference a Metal defect would
        // point at the wrong op — count them and move on. A Metal-side zero
        // against a live CPU reference (the embedding-gather regression this
        // was written for) still reports, since that is this branch's inverse.
        if peak == 0.0 {
            unverifiable += 1;
            continue;
        }
        let diff = max_abs_diff(c, m);
        if diff / peak > 0.05 {
            first_bad = Some((i, diff, peak));
            break;
        }
    }
    if unverifiable > 0 {
        println!(
            "{unverifiable}/{} ops skipped: no CPU reference (slot not materialised)",
            cpu.len()
        );
    }

    match first_bad {
        None => println!("no divergence above 5% of peak — Metal matches CPU op by op"),
        Some((i, diff, peak)) => {
            let node = graph.node(original_ids[i]);
            println!("FIRST DIVERGENCE at exposed op #{i}");
            println!("  op     : {:?}", node.op);
            println!("  shape  : {:?}", node.shape);
            println!("  max|Δ| : {diff:.4e}  (peak {peak:.4e})");
            println!("  cpu[..8]   : {:?}", &cpu[i][..cpu[i].len().min(8)]);
            println!("  metal[..8] : {:?}", &metal[i][..metal[i].len().min(8)]);
            // Context: the few ops leading up to it.
            for back in (1..=3).rev() {
                if i >= back {
                    let n = graph.node(original_ids[i - back]);
                    println!("  (-{back}) {:?}", n.op);
                }
            }
            panic!("Metal diverges from CPU at op #{i}: {:?}", node.op);
        }
    }
}

/// Build the real nomic-bert graph from the cached HF weights, the same way
/// `text_embedder` does. Returns `None` when the model is not available.
fn build_embedding_graph() -> Option<(Graph, Vec<(String, Vec<f32>)>, usize)> {
    let repo_id = "nomic-ai/nomic-embed-text-v1.5";
    let repo = hf_hub::api::sync::ApiBuilder::new()
        .with_progress(false)
        .build()
        .ok()?
        .model(repo_id.to_string());
    let config_path = repo.get("config.json").ok()?;
    let weights_path = repo.get("model.safetensors").ok()?;

    let mut wm = rlx_models::WeightMap::from_file(weights_path.to_str()?).ok()?;
    let cfg = rlx_models::NomicBertConfig::from_file(&config_path).ok()?;
    let seq = 16;
    let (graph, params) = rlx_models::nomic::build_nomic_graph_sized(&cfg, &mut wm, 1, seq).ok()?;
    // `build_nomic_graph_sized` hands back a map; the run loop wants a stable
    // order, and set_param is order-independent anyway.
    Some((graph, params.into_iter().collect(), seq))
}
