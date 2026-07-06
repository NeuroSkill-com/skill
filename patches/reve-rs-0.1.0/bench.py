#!/usr/bin/env python3
"""Benchmark REVE: Python (PyTorch) vs Rust (Burn) inference latency.

Generates JSON results and PNG charts in figures/ directory.
"""

import sys, types, time, json, os, subprocess, platform
import torch
import numpy as np

# ── Mock braindecode deps ────────────────────────────────────────────────────
class EEGModuleMixin:
    def __init__(self, n_outputs=None, n_chans=None, chs_info=None, n_times=None,
                 input_window_seconds=None, sfreq=None, **kwargs):
        super().__init__()
        self.n_outputs = n_outputs; self.n_chans = n_chans
        self.chs_info = chs_info; self.n_times = n_times; self.sfreq = sfreq

bmmb = types.ModuleType('braindecode.models.base')
bmmb.EEGModuleMixin = EEGModuleMixin
sys.modules['braindecode'] = types.ModuleType('braindecode')
sys.modules['braindecode.models'] = types.ModuleType('braindecode.models')
sys.modules['braindecode.models.base'] = bmmb

import importlib.util
spec = importlib.util.spec_from_file_location('reve', '/Users/Shared/braindecode/braindecode/models/reve.py')
reve_mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reve_mod)
REVE = reve_mod.REVE

# ── Config ───────────────────────────────────────────────────────────────────
EMBED_DIM = 512
DEPTH = 2
HEADS = 8
HEAD_DIM = 64
N_OUTPUTS = 4
PATCH_SIZE = 200
PATCH_OVERLAP = 20
WARMUP = 5
REPEATS = 30

CONFIGS = [
    (4,   400,  "4ch×400t"),
    (8,   1000, "8ch×1000t"),
    (16,  1000, "16ch×1000t"),
    (22,  1000, "22ch×1000t"),
    (32,  1000, "32ch×1000t"),
    (64,  1000, "64ch×1000t"),
    (22,  2000, "22ch×2000t"),
    (22,  4000, "22ch×4000t"),
]

RUST_BACKENDS = [
    ("ndarray",    "target/release/examples/benchmark_ndarray"),
    ("accelerate", "target/release/examples/benchmark_accelerate"),
    ("metal",      "target/release/examples/benchmark_metal"),
]


def bench_python(n_chans, n_times):
    torch.manual_seed(42)
    model = REVE(
        n_outputs=N_OUTPUTS, n_chans=n_chans, n_times=n_times, sfreq=200,
        embed_dim=EMBED_DIM, depth=DEPTH, heads=HEADS, head_dim=HEAD_DIM,
        mlp_dim_ratio=2.66, use_geglu=True, freqs=4,
        patch_size=PATCH_SIZE, patch_overlap=PATCH_OVERLAP,
        attention_pooling=True,
    )
    model.eval()
    eeg = torch.randn(1, n_chans, n_times)
    pos = torch.randn(1, n_chans, 3)

    with torch.no_grad():
        for _ in range(WARMUP):
            _ = model(eeg, pos=pos)
    times = []
    with torch.no_grad():
        for _ in range(REPEATS):
            t0 = time.perf_counter()
            _ = model(eeg, pos=pos)
            times.append((time.perf_counter() - t0) * 1000)
    return times


def bench_rust(binary, n_chans, n_times):
    if not os.path.exists(binary):
        return None
    try:
        result = subprocess.run(
            [binary, str(n_chans), str(n_times), str(WARMUP), str(REPEATS)],
            capture_output=True, text=True, timeout=120
        )
        if result.returncode != 0:
            return None
        data = json.loads(result.stdout)
        return data["times_ms"]
    except Exception:
        return None


def main():
    os.makedirs("figures", exist_ok=True)

    results = {
        "meta": {
            "platform": platform.platform(),
            "processor": platform.processor(),
            "machine": platform.machine(),
            "python_version": platform.python_version(),
            "torch_version": torch.__version__,
            "embed_dim": EMBED_DIM,
            "depth": DEPTH,
            "heads": HEADS,
            "warmup": WARMUP,
            "repeats": REPEATS,
        },
        "benchmarks": []
    }

    print(f"Platform: {platform.platform()}")
    print(f"Machine: {platform.machine()}")
    print(f"PyTorch: {torch.__version__}")
    print(f"Config: embed_dim={EMBED_DIM}, depth={DEPTH}, warmup={WARMUP}, repeats={REPEATS}")
    print()

    for n_chans, n_times, label in CONFIGS:
        print(f"── {label} ──")

        py_times = bench_python(n_chans, n_times)
        py_mean = np.mean(py_times)
        py_std = np.std(py_times)
        print(f"  Python (PyTorch):     {py_mean:7.2f} ± {py_std:.2f} ms")

        entry = {
            "label": label,
            "n_chans": n_chans,
            "n_times": n_times,
            "python_times_ms": py_times,
            "python_mean_ms": float(py_mean),
            "python_std_ms": float(py_std),
        }

        for backend_name, binary in RUST_BACKENDS:
            rs_times = bench_rust(binary, n_chans, n_times)
            if rs_times:
                rs_mean = np.mean(rs_times)
                rs_std = np.std(rs_times)
                speedup = py_mean / rs_mean
                print(f"  Rust ({backend_name:12s}): {rs_mean:7.2f} ± {rs_std:.2f} ms  ({speedup:.2f}x)")
            else:
                rs_mean = rs_std = speedup = None
                rs_times = []

            entry[f"rust_{backend_name}_times_ms"] = rs_times
            entry[f"rust_{backend_name}_mean_ms"] = float(rs_mean) if rs_mean else None
            entry[f"rust_{backend_name}_std_ms"] = float(rs_std) if rs_std else None
            entry[f"rust_{backend_name}_speedup"] = float(speedup) if speedup else None

        results["benchmarks"].append(entry)
        print()

    with open("figures/benchmark_results.json", "w") as f:
        json.dump(results, f, indent=2)
    print(f"Saved results to figures/benchmark_results.json")

    generate_charts(results)


def generate_charts(results):
    try:
        import matplotlib
        matplotlib.use('Agg')
        import matplotlib.pyplot as plt
    except ImportError:
        print("matplotlib not installed, skipping charts")
        return

    benchmarks = results["benchmarks"]
    labels = [b["label"] for b in benchmarks]
    py_means = [b["python_mean_ms"] for b in benchmarks]
    py_stds = [b["python_std_ms"] for b in benchmarks]

    backend_colors = {
        "python":     "#4C72B0",
        "ndarray":    "#DD8452",
        "accelerate": "#55A868",
        "metal":      "#C44E52",
    }
    backend_labels = {
        "python":     "Python (PyTorch)",
        "ndarray":    "Rust (NdArray)",
        "accelerate": "Rust (Accelerate)",
        "metal":      "Rust (Metal GPU)",
    }

    rust_backends = ["ndarray", "accelerate", "metal"]
    active_backends = [b for b in rust_backends
                       if any(entry.get(f"rust_{b}_mean_ms") is not None for entry in benchmarks)]

    n_bars = 1 + len(active_backends)
    width = 0.8 / n_bars

    # ── Chart 1: Inference Latency ───────────────────────────────────────────
    fig, ax = plt.subplots(figsize=(14, 6))
    x = np.arange(len(labels))

    ax.bar(x - width * (n_bars - 1) / 2, py_means, width, yerr=py_stds,
           label=backend_labels["python"], color=backend_colors["python"], capsize=2, alpha=0.85)

    for i, bk in enumerate(active_backends):
        means = [b.get(f"rust_{bk}_mean_ms") or 0 for b in benchmarks]
        stds = [b.get(f"rust_{bk}_std_ms") or 0 for b in benchmarks]
        offset = -width * (n_bars - 1) / 2 + width * (i + 1)
        ax.bar(x + offset, means, width, yerr=stds,
               label=backend_labels[bk], color=backend_colors[bk], capsize=2, alpha=0.85)

    ax.set_xlabel('Configuration', fontsize=12)
    ax.set_ylabel('Latency (ms)', fontsize=12)
    ax.set_title('REVE Inference Latency', fontsize=14, fontweight='bold')
    ax.set_xticks(x)
    ax.set_xticklabels(labels, rotation=30, ha='right', fontsize=10)
    ax.legend(fontsize=10, loc='upper left')
    ax.grid(axis='y', alpha=0.3)
    plt.tight_layout()
    plt.savefig('figures/inference_latency.png', dpi=150)
    plt.close()
    print("Saved figures/inference_latency.png")

    # ── Chart 2: Speedup ────────────────────────────────────────────────────
    fig, ax = plt.subplots(figsize=(14, 6))
    x = np.arange(len(labels))
    n_sp = len(active_backends)
    sp_width = 0.8 / max(n_sp, 1)

    for i, bk in enumerate(active_backends):
        speedups = [b.get(f"rust_{bk}_speedup") or 0 for b in benchmarks]
        offset = -sp_width * (n_sp - 1) / 2 + sp_width * i
        colors = [backend_colors[bk] if s > 0 else '#cccccc' for s in speedups]
        bars = ax.bar(x + offset, speedups, sp_width, color=colors, alpha=0.85,
                      label=backend_labels[bk])
        for j, (bar, sp) in enumerate(zip(bars, speedups)):
            if sp > 0:
                ax.text(bar.get_x() + bar.get_width()/2., bar.get_height() + 0.01,
                        f'{sp:.2f}x', ha='center', va='bottom', fontsize=7, fontweight='bold')

    ax.axhline(y=1.0, color='gray', linestyle='--', linewidth=1, label='Parity (1.0x)')
    ax.set_xlabel('Configuration', fontsize=12)
    ax.set_ylabel('Speedup (vs Python)', fontsize=12)
    ax.set_title('Rust Speedup over Python (PyTorch)', fontsize=14, fontweight='bold')
    ax.set_xticks(x)
    ax.set_xticklabels(labels, rotation=30, ha='right', fontsize=10)
    ax.legend(fontsize=9, loc='upper left')
    ax.grid(axis='y', alpha=0.3)
    plt.tight_layout()
    plt.savefig('figures/speedup.png', dpi=150)
    plt.close()
    print("Saved figures/speedup.png")

    # ── Chart 3: Channel Scaling ─────────────────────────────────────────────
    chan_benchmarks = [b for b in benchmarks if b["n_times"] == 1000]
    if len(chan_benchmarks) > 1:
        fig, ax = plt.subplots(figsize=(9, 5))
        chans = [b["n_chans"] for b in chan_benchmarks]
        py_lat = [b["python_mean_ms"] for b in chan_benchmarks]
        ax.plot(chans, py_lat, 'o-', color=backend_colors["python"],
                label=backend_labels["python"], linewidth=2, markersize=7)
        for bk in active_backends:
            lat = [b.get(f"rust_{bk}_mean_ms") for b in chan_benchmarks]
            if any(v is not None for v in lat):
                ch = [c for c, v in zip(chans, lat) if v is not None]
                la = [v for v in lat if v is not None]
                ax.plot(ch, la, 's-', color=backend_colors[bk],
                        label=backend_labels[bk], linewidth=2, markersize=7)

        ax.set_xlabel('Number of Channels', fontsize=12)
        ax.set_ylabel('Latency (ms)', fontsize=12)
        ax.set_title('Latency vs Channel Count (T=1000)', fontsize=14, fontweight='bold')
        ax.legend(fontsize=10)
        ax.grid(alpha=0.3)
        plt.tight_layout()
        plt.savefig('figures/channel_scaling.png', dpi=150)
        plt.close()
        print("Saved figures/channel_scaling.png")

    # ── Chart 4: Time Scaling ────────────────────────────────────────────────
    time_benchmarks = [b for b in benchmarks if b["n_chans"] == 22]
    if len(time_benchmarks) > 1:
        fig, ax = plt.subplots(figsize=(9, 5))
        times_list = [b["n_times"] for b in time_benchmarks]
        py_lat = [b["python_mean_ms"] for b in time_benchmarks]
        ax.plot(times_list, py_lat, 'o-', color=backend_colors["python"],
                label=backend_labels["python"], linewidth=2, markersize=7)
        for bk in active_backends:
            lat = [b.get(f"rust_{bk}_mean_ms") for b in time_benchmarks]
            if any(v is not None for v in lat):
                t = [c for c, v in zip(times_list, lat) if v is not None]
                la = [v for v in lat if v is not None]
                ax.plot(t, la, 's-', color=backend_colors[bk],
                        label=backend_labels[bk], linewidth=2, markersize=7)

        ax.set_xlabel('Number of Time Samples', fontsize=12)
        ax.set_ylabel('Latency (ms)', fontsize=12)
        ax.set_title('Latency vs Signal Length (C=22)', fontsize=14, fontweight='bold')
        ax.legend(fontsize=10)
        ax.grid(alpha=0.3)
        plt.tight_layout()
        plt.savefig('figures/time_scaling.png', dpi=150)
        plt.close()
        print("Saved figures/time_scaling.png")

    # ── Chart 5: Latency Distribution ────────────────────────────────────────
    fig, ax = plt.subplots(figsize=(14, 6))
    group_width = 2 + len(active_backends)
    all_positions = []
    all_data = []
    all_colors = []
    tick_positions = []
    tick_labels = []

    for i, b in enumerate(benchmarks):
        base = i * group_width
        tick_positions.append(base + (1 + len(active_backends)) / 2)
        tick_labels.append(b["label"])

        all_positions.append(base)
        all_data.append(b["python_times_ms"])
        all_colors.append(backend_colors["python"])

        for j, bk in enumerate(active_backends):
            ts = b.get(f"rust_{bk}_times_ms") or [0]
            all_positions.append(base + j + 1)
            all_data.append(ts)
            all_colors.append(backend_colors[bk])

    bp = ax.boxplot(all_data, positions=all_positions, widths=0.7,
                    patch_artist=True, medianprops=dict(color='white', linewidth=1.5))
    for patch, color in zip(bp['boxes'], all_colors):
        patch.set_facecolor(color)
        patch.set_alpha(0.75)

    # Legend
    from matplotlib.patches import Patch
    legend_items = [Patch(facecolor=backend_colors["python"], alpha=0.75, label=backend_labels["python"])]
    for bk in active_backends:
        legend_items.append(Patch(facecolor=backend_colors[bk], alpha=0.75, label=backend_labels[bk]))
    ax.legend(handles=legend_items, fontsize=9, loc='upper left')

    ax.set_xticks(tick_positions)
    ax.set_xticklabels(tick_labels, rotation=30, ha='right', fontsize=10)
    ax.set_ylabel('Latency (ms)', fontsize=12)
    ax.set_title('Latency Distribution', fontsize=14, fontweight='bold')
    ax.grid(axis='y', alpha=0.3)
    plt.tight_layout()
    plt.savefig('figures/latency_distribution.png', dpi=150)
    plt.close()
    print("Saved figures/latency_distribution.png")


if __name__ == "__main__":
    main()
