#!/bin/bash
# Build RLX benchmark binaries and run the suite.
set -euo pipefail

cd "$(dirname "$0")"

echo "=== Building RLX benchmark backends ==="

echo "CPU (rlx-cpu)..."
cargo build --release --example benchmark --features rlx-cpu
cp target/release/examples/benchmark target/release/examples/benchmark_cpu

echo "Metal (rlx-metal)..."
cargo build --release --example benchmark --features rlx-cpu,rlx-metal
cp target/release/examples/benchmark target/release/examples/benchmark_metal

echo "MLX (rlx-mlx)..."
cargo build --release --example benchmark --features rlx-cpu,rlx-mlx
cp target/release/examples/benchmark target/release/examples/benchmark_mlx

echo ""
echo "=== Smoke: infer (CPU + Metal + MLX) ==="
cargo build --release --bin infer --features rlx-cpu,rlx-metal,rlx-mlx
for dev in cpu metal mlx; do
  echo "--- infer --device $dev ---"
  cargo run --release --bin infer --features rlx-cpu,rlx-metal,rlx-mlx -- \
    --device "$dev" --config data/config.json --weights data/model.safetensors -v
done

echo ""
echo "=== Parity tests ==="
cargo test --release --features rlx-cpu,rlx-metal,rlx-mlx \
  --test rlx_backend_parity --test rlx_attn_parity -- --test-threads=1

echo ""
echo "=== Benchmarks ==="
python3 bench_rlx.py

echo ""
echo "Done."
