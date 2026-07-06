//! Smoke-compile RLX JEPA predictor graphs (no weights).

#![cfg(feature = "rlx-engine")]

use brainjepa::rlx::graph::{
    build_encoder_embed_graph, build_encoder_trunk_graph, build_predictor_graph, EncoderSpec,
    PredictorSpec,
};
use brainjepa::rlx::AttnLayout;
use rlx::Device;

#[test]
fn predictor_graphs_compile() {
    let spec = EncoderSpec {
        b: 1,
        h: 10,
        w: 160,
        patch: 16,
        w_p: 10,
        n: 100,
        dim: 64,
        depth: 2,
        num_heads: 4,
        head_dim: 16,
        hidden_dim: 256,
        norm_eps: 1e-6,
    };
    let attn = AttnLayout::Bsnh;
    let dev = Device::Cpu;
    let session = rlx::Session::new(dev);

    let mut embed = session.compile(build_encoder_embed_graph(&spec));
    embed.set_param("pos_embed", &vec![0.0; spec.n * spec.dim]);

    let trunk = session.compile(build_encoder_trunk_graph(&spec, attn, 80));
    let pred_spec = PredictorSpec {
        b: 1,
        n_patches: spec.n,
        n_ctx: 80,
        n_pred: 12,
        enc_dim: spec.dim,
        pred_dim: 32,
        depth: 2,
        num_heads: 4,
        head_dim: 8,
        hidden_dim: 128,
        norm_eps: 1e-6,
    };
    let _pred = session.compile(build_predictor_graph(&pred_spec, attn));
    let _ = trunk;
}
