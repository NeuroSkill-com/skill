//! RLX classification head smoke test (no checkpoint).

#![cfg(feature = "rlx-engine")]

use brainjepa::rlx::RlxClassificationHead;

#[test]
fn rlx_classification_head_forward_shape() {
    let dev = rlx::Device::Cpu;
    let mut head = RlxClassificationHead::new(100, 64, 3, 1e-6, &dev).expect("head");
    let emb = vec![0.01f32; 100 * 64];
    let logits = head.forward(&emb).expect("forward");
    assert_eq!(logits.len(), 3);
}
