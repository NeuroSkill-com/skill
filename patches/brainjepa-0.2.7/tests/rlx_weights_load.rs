//! Verify RLX encoder params load from the real HuggingFace checkpoint.

#![cfg(feature = "rlx")]

use std::path::PathBuf;

use brainjepa::rlx::graph::build_encoder_graph;
use brainjepa::rlx::weights::{build_encoder_params, load_safetensors};
use brainjepa::{DataConfig, ModelConfig};
use rlx::Op;

fn locate_weights() -> Option<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    std::env::var("BRAINJEPA_WEIGHTS")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            let p = manifest.join("data/brainjepa.safetensors");
            p.exists().then_some(p)
        })
        .or_else(|| {
            brainjepa::hf_download::scan_cache(brainjepa::DEFAULT_REPO, None)
                .map(|r| r.weights_path)
        })
}

#[test]
fn encoder_params_match_graph_and_checkpoint_prefix() {
    let weights = match locate_weights() {
        Some(p) => p,
        None => {
            eprintln!("[SKIP] rlx_weights_load — no checkpoint on disk");
            return;
        }
    };

    let model = ModelConfig::default();
    let data = DataConfig::default();
    let (h, w) = data.crop_size;
    let wp = w / model.patch_size;

    let mut raw = load_safetensors(weights.to_str().unwrap()).expect("load safetensors");
    let has_target = raw.contains_key("target_encoder.blocks.0.norm1.weight");
    assert!(has_target, "expected target_encoder.* keys in checkpoint");

    let (params, grad_proj) = build_encoder_params(&mut raw, &model).expect("build params");
    assert!(
        grad_proj.is_some(),
        "vit_base mapping mode needs grad projection weights"
    );

    let spec = brainjepa::rlx::graph::EncoderSpec {
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

    let mut graph_params = 0usize;
    for node in graph.nodes() {
        if let Op::Param { name } = &node.op {
            graph_params += 1;
            let n = node.shape.num_elements().expect("static param shape");
            if name == "pos_embed" {
                compiled.set_param(name, &vec![0.0f32; n]);
                continue;
            }
            let buf = params
                .get(name)
                .unwrap_or_else(|| panic!("graph param {name} missing from loader map"));
            assert_eq!(
                buf.data.len(),
                n,
                "param {name}: buffer len {} != shape {:?}",
                buf.data.len(),
                node.shape
            );
            compiled.set_param(name, &buf.data);
        }
    }

    assert!(graph_params > 100, "expected many encoder params in graph");
    eprintln!("loaded {graph_params} graph params from checkpoint (prefix=target_encoder)");
}
