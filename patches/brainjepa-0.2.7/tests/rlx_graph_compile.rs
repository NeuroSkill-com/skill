//! RLX encoder graph compile + zero-weight smoke test (no HF download).

#![cfg(feature = "rlx")]

use brainjepa::rlx::graph::{build_encoder_graph, EncoderSpec};
use brainjepa::{DataConfig, ModelConfig};

fn zero_fill_params(graph: &rlx::Graph, compiled: &mut rlx::CompiledGraph) {
    use rlx::Op;
    for node in graph.nodes() {
        let Op::Param { name } = &node.op else {
            continue;
        };
        let n = node
            .shape
            .num_elements()
            .expect("param shape must be static");
        compiled.set_param(name, &vec![0.0f32; n]);
    }
}

#[test]
fn encoder_graph_compiles_and_runs_vit_base_shape() {
    let model = ModelConfig::default();
    let data = DataConfig::default();
    let (h, w) = data.crop_size;
    let wp = w / model.patch_size;
    let spec = EncoderSpec {
        b: 1,
        h,
        w,
        patch: model.patch_size,
        w_p: wp,
        n: h * wp,
        dim: model.embed_dim,
        depth: model.depth,
        num_heads: model.num_heads,
        head_dim: model.head_dim(),
        hidden_dim: model.mlp_hidden_dim(),
        norm_eps: model.norm_eps as f32,
    };

    let graph = build_encoder_graph(&spec, brainjepa::rlx::graph::AttnLayout::Bsnh);
    let mut compiled = rlx::Session::new(rlx::Device::Cpu).compile(graph.clone());
    zero_fill_params(&graph, &mut compiled);

    let x = vec![0.01f32; 1 * 1 * h * w];
    let slots = compiled.run_slots(&[&x]);
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].1, spec.n * spec.dim);
}
