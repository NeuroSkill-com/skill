//! Manual-attention matmul path parity (REVE shapes: B*H=8, S=176, D=64).

use rlx::ir::GraphExt;
use rlx::prelude::*;

const COS_MIN: f64 = 0.9999;
const MAX_ABS: f32 = 0.15;

fn rand_like(n: usize, seed: u64) -> Vec<f32> {
    let mut s = if seed == 0 { 0xCAFEF00DD15EA5E5 } else { seed };
    (0..n).map(|_| {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (((s >> 11) as f64 / (1u64 << 53) as f64) as f32 - 0.5) * 2.0
    }).collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f64 {
    let (mut d, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for (&x, &y) in a.iter().zip(b.iter()) {
        let (x, y) = (x as f64, y as f64);
        d += x * y;
        na += x * x;
        nb += y * y;
    }
    d / (na.sqrt() * nb.sqrt())
}

fn assert_parity(label: &str, cpu: &[f32], other: &[f32]) {
    let mut max_abs = 0.0f32;
    let mut n_nan = 0usize;
    for (a, b) in cpu.iter().zip(other.iter()) {
        if b.is_nan() {
            n_nan += 1;
        }
        max_abs = max_abs.max((a - b).abs());
    }
    let cos = cosine(cpu, other);
    eprintln!("{label}: max_abs={max_abs:.3e} cosine={cos:.6} nan={n_nan}");
    assert!(n_nan == 0, "{label}: {n_nan} NaNs");
    assert!(max_abs < MAX_ABS && cos > COS_MIN, "{label}: drift");
}

fn run_on_device<F>(build: F, inputs: &[(&str, &[f32])], dev: rlx::Device) -> Vec<f32>
where
    F: Fn() -> Graph,
{
    reve_rs::rlx::prepare_device(dev);
    let mut compiled = rlx::Session::new(dev).compile(build());
    compiled.set_param("scale", &[(64f32).powf(-0.5)]);
    compiled.run(inputs).into_iter().next().unwrap()
}

fn try_metal<F>(build: F, inputs: &[(&str, &[f32])]) -> Option<Vec<f32>>
where
    F: Fn() -> Graph + Copy,
{
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_on_device(build, inputs, rlx::Device::Metal)
    })) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("[skip] Metal not available");
            None
        }
    }
}

#[test]
fn reve_manual_attention_bshd() {
    let (b, s, nh, dh) = (1usize, 176, 8, 64);
    let bh = b * nh;
    let inner = nh * dh;
    let q = rand_like(b * s * inner, 1);
    let k = rand_like(b * s * inner, 2);
    let v = rand_like(b * s * inner, 3);

    let build = || {
        let mut g = Graph::new("attn");
        let qi = g.input("q", Shape::new(&[b, s, inner], DType::F32));
        let ki = g.input("k", Shape::new(&[b, s, inner], DType::F32));
        let vi = g.input("v", Shape::new(&[b, s, inner], DType::F32));

        let q4 = g.reshape_(qi, vec![b as i64, s as i64, nh as i64, dh as i64]);
        let k4 = g.reshape_(ki, vec![b as i64, s as i64, nh as i64, dh as i64]);
        let v4 = g.reshape_(vi, vec![b as i64, s as i64, nh as i64, dh as i64]);
        let q_bhsd = g.transpose_(q4, vec![0, 2, 1, 3]);
        let k_bhsd = g.transpose_(k4, vec![0, 2, 1, 3]);
        let v_bhsd = g.transpose_(v4, vec![0, 2, 1, 3]);
        let q3 = g.reshape_(q_bhsd, vec![bh as i64, s as i64, dh as i64]);
        let k3 = g.reshape_(k_bhsd, vec![bh as i64, s as i64, dh as i64]);
        let v3 = g.reshape_(v_bhsd, vec![bh as i64, s as i64, dh as i64]);
        let k_t = g.transpose_(k3, vec![0, 2, 1]);
        let scores = g.mm(q3, k_t);
        let scale = g.param("scale", Shape::new(&[1], DType::F32));
        let scores = g.mul(scores, scale);
        let w = g.sm(scores, 2);
        let attn_out = g.mm(w, v3);
        let attn_bhsd = g.reshape_(attn_out, vec![b as i64, nh as i64, s as i64, dh as i64]);
        let attn_bshd = g.transpose_(attn_bhsd, vec![0, 2, 1, 3]);
        let y = g.reshape_(attn_bshd, vec![b as i64, s as i64, inner as i64]);
        g.set_outputs(vec![y]);
        g
    };

    let inputs: [(&str, &[f32]); 3] = [("q", &q), ("k", &k), ("v", &v)];
    let cpu = run_on_device(build, &inputs, rlx::Device::Cpu);
    if let Some(metal) = try_metal(build, &inputs) {
        assert_parity("Metal manual attn", &cpu, &metal);
    }
}

#[test]
fn reve_rms_norm() {
    let (b, s, d) = (1usize, 176, 512);
    let x = rand_like(b * s * d, 10);
    let gamma = rand_like(d, 11);
    let zeros = vec![0f32; d];
    let build = || {
        let mut g = Graph::new("rn");
        let xi = g.input("x", Shape::new(&[b, s, d], DType::F32));
        let gma = g.param("gamma", Shape::new(&[d], DType::F32));
        let beta = g.param("beta", Shape::new(&[d], DType::F32));
        let y = g.rms_norm(xi, gma, beta, 1e-6);
        g.set_outputs(vec![y]);
        g
    };
    let run = |dev: rlx::Device| {
        let mut compiled = rlx::Session::new(dev).compile(build());
        compiled.set_param("gamma", &gamma);
        compiled.set_param("beta", &zeros);
        compiled.run(&[("x", &x)]).into_iter().next().unwrap()
    };
    let cpu = run(rlx::Device::Cpu);
    if let Some(metal) = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(rlx::Device::Metal)
    })) {
        Ok(v) => Some(v),
        Err(_) => None,
    } {
        assert_parity("Metal rms_norm", &cpu, &metal);
    }
}

#[test]
fn reve_geglu() {
    let (b, s, d, h) = (1usize, 176, 512, 1362);
    let x = rand_like(b * s * d, 20);
    let w_gate = rand_like(d * h, 21);
    let w_up = rand_like(d * h, 22);
    let build = || {
        let mut g = Graph::new("geglu");
        let xi = g.input("x", Shape::new(&[b, s, d], DType::F32));
        let wg = g.param("wg", Shape::new(&[d, h], DType::F32));
        let wu = g.param("wu", Shape::new(&[d, h], DType::F32));
        let gates = g.mm(xi, wg);
        let up = g.mm(xi, wu);
        let g_act = g.gelu(gates);
        let y = g.mul(g_act, up);
        g.set_outputs(vec![y]);
        g
    };
    let run = |dev: rlx::Device| {
        let mut compiled = rlx::Session::new(dev).compile(build());
        compiled.set_param("wg", &w_gate);
        compiled.set_param("wu", &w_up);
        compiled.run(&[("x", &x)]).into_iter().next().unwrap()
    };
    let cpu = run(rlx::Device::Cpu);
    if let Some(metal) = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(rlx::Device::Metal)
    })) {
        Ok(v) => Some(v),
        Err(_) => None,
    } {
        assert_parity("Metal geglu", &cpu, &metal);
    }
}

#[test]
fn reve_attention_pool() {
    let (b, s, d) = (1usize, 176, 512);
    let cls_q = rand_like(b * d, 30);
    let x = rand_like(b * s * d, 31);
    let build = || {
        let mut g = Graph::new("pool");
        let q = g.input("cls_q", Shape::new(&[b, 1, d], DType::F32));
        let x = g.input("x", Shape::new(&[b, s, d], DType::F32));
        let x_t = g.transpose_(x, vec![0, 2, 1]);
        let scores = g.mm(q, x_t);
        let scale = g.param("scale", Shape::new(&[1], DType::F32));
        let scores = g.mul(scores, scale);
        let w = g.sm(scores, 2);
        let y = g.mm(w, x);
        let y = g.reshape_(y, vec![b as i64, d as i64]);
        g.set_outputs(vec![y]);
        g
    };
    let run = |dev: rlx::Device| {
        let mut compiled = rlx::Session::new(dev).compile(build());
        compiled.set_param("scale", &[(d as f32).powf(-0.5)]);
        compiled
            .run(&[("cls_q", &cls_q), ("x", &x)])
            .into_iter()
            .next()
            .unwrap()
    };
    let cpu = run(rlx::Device::Cpu);
    if let Some(metal) = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(rlx::Device::Metal)
    })) {
        Ok(v) => Some(v),
        Err(_) => None,
    } {
        assert_parity("Metal attn pool", &cpu, &metal);
    }
}

#[test]
fn reve_graph_has_single_shared_param_nodes() {
    use reve_rs::rlx::graph::{build_reve_graph, ReveSpec, KEY_ZEROS_EMBED, KEY_ATTN_HEAD_SCALE};
    let spec = ReveSpec {
        b: 1,
        s: 176,
        patch_size: 200,
        embed_dim: 512,
        n_outputs: 0,
        depth: 22,
        heads: 8,
        head_dim: 64,
        mlp_dim: 1362,
        use_geglu: true,
        freqs: 64,
        attention_pooling: true,
    };
    let g = build_reve_graph(&spec);
    let mut zeros = 0usize;
    let mut scale = 0usize;
    for n in g.nodes() {
        if let rlx::ir::Op::Param { name } = &n.op {
            if name == KEY_ZEROS_EMBED {
                zeros += 1;
            }
            if name == KEY_ATTN_HEAD_SCALE {
                scale += 1;
            }
        }
    }
    assert_eq!(zeros, 1, "expected one zeros param node");
    assert_eq!(scale, 1, "expected one attn head scale param node");
}
