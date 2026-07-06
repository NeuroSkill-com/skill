#!/usr/bin/env python3
"""Benchmark full REVE (RLX) inference — CPU / Metal / MLX.

Build backends first:
  ./bench.sh

Or manually:
  cargo build --release --example benchmark --features rlx-cpu
"""

from __future__ import annotations

import json
import os
import platform
import subprocess
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent
WARMUP = int(os.environ.get("BENCH_WARMUP", "2"))
REPEATS = int(os.environ.get("BENCH_REPEATS", "10"))

CONFIGS = [
    (22, 1000, "22ch×1000t"),
    (22, 2000, "22ch×2000t"),
]

BACKENDS = [
    ("cpu", "target/release/examples/benchmark_cpu"),
    ("metal", "target/release/examples/benchmark_metal"),
    ("mlx", "target/release/examples/benchmark_mlx"),
]


def bench_rust(device: str, binary: Path, n_chans: int, n_times: int) -> dict | None:
    if not binary.exists():
        return None
    try:
        proc = subprocess.run(
            [
                str(binary),
                "--device",
                device,
                str(n_chans),
                str(n_times),
                str(WARMUP),
                str(REPEATS),
            ],
            capture_output=True,
            text=True,
            timeout=600,
            cwd=ROOT,
        )
    except (subprocess.TimeoutExpired, OSError) as e:
        print(f"  [{device}] error: {e}", file=sys.stderr)
        return None
    if proc.returncode != 0:
        print(f"  [{device}] failed (exit {proc.returncode})", file=sys.stderr)
        if proc.stderr.strip():
            print(proc.stderr.strip(), file=sys.stderr)
        return None
    line = proc.stdout.strip().splitlines()[-1]
    data = json.loads(line)
    if not data.get("ok"):
        print(f"  [{device}] not ok: {data}", file=sys.stderr)
        return None
    times = data["times_ms"]
    return {
        "backend": data.get("backend", device),
        "times_ms": times,
        "mean_ms": float(np.mean(times)),
        "std_ms": float(np.std(times)),
        "output_dim": data.get("output_dim"),
    }


def main() -> int:
    os.makedirs(ROOT / "figures", exist_ok=True)
    print(f"Platform: {platform.platform()}")
    print(f"Warmup={WARMUP} repeats={REPEATS}")
    print()

    results = {"meta": {"warmup": WARMUP, "repeats": REPEATS}, "benchmarks": []}

    for n_chans, n_times, label in CONFIGS:
        print(f"── {label} ──")
        entry = {"label": label, "n_chans": n_chans, "n_times": n_times}
        for device, binary in BACKENDS:
            stats = bench_rust(device, ROOT / binary, n_chans, n_times)
            if stats:
                print(
                    f"  {stats['backend']:12s}  {stats['mean_ms']:8.2f} ± {stats['std_ms']:.2f} ms"
                    f"  (dim={stats['output_dim']})"
                )
                entry[device] = stats
            else:
                print(f"  {device:12s}  (skipped)")
                entry[device] = None
        results["benchmarks"].append(entry)
        print()

    out_path = ROOT / "figures" / "benchmark_rlx_results.json"
    out_path.write_text(json.dumps(results, indent=2))
    print(f"Saved {out_path}")

    failed = [d for b in results["benchmarks"] for d, s in b.items() if d in ("cpu", "metal", "mlx") and s is None]
    # cpu must succeed
    cpu_ok = all(b.get("cpu") is not None for b in results["benchmarks"])
    return 0 if cpu_ok else 1


if __name__ == "__main__":
    sys.exit(main())
