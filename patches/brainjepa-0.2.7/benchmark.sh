#!/bin/bash
# benchmark.sh — Brain-JEPA RLX multi-backend benchmark
#
# Usage:
#   bash benchmark.sh                 # build & bench RLX backends (cpu, metal, mlx on macOS)
#   bash benchmark.sh --runs 5        # 5 iterations per backend
#   bash benchmark.sh --no-build      # skip cargo build (use existing binaries)
#   bash benchmark.sh --gpu-only        # skip CPU, only bench metal/mlx (macOS) or gpu (Linux)
#
# On macOS  -> RLX: cpu, metal, mlx, wgpu
# On Linux  -> RLX: cpu, gpu

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# ── Parse flags ───────────────────────────────────────────────────────────────
RUNS=3; NO_BUILD=0; GPU_ONLY=0
while [ $# -gt 0 ]; do
    case "$1" in
        --runs)      shift; RUNS="$1" ;;
        --runs=*)    RUNS="${1#--runs=}" ;;
        --no-build)  NO_BUILD=1 ;;
        --gpu-only)  GPU_ONLY=1 ;;
        -h|--help)
            printf 'Usage: bash %s [--runs N] [--no-build] [--gpu-only]\n' "$0"
            exit 0 ;;
        *) printf 'Unknown option: %s\n' "$1" >&2; exit 1 ;;
    esac
    shift
done

# ── Helpers ───────────────────────────────────────────────────────────────────
die()  { printf '\033[31m✗  %s\033[0m\n' "$*" >&2; exit 1; }
step() { printf '\n\033[1;34m━━━  %s\033[0m\n' "$*"; }
ok()   { printf '  \033[32m✓\033[0m  %s\n' "$*"; }
info() { printf '  %s\n' "$*"; }
warn() { printf '  \033[33m⚠\033[0m  %s\n' "$*"; }

# ── Platform detect ──────────────────────────────────────────────────────────
OS="$(uname -s)"
NCPUS="$(sysctl -n hw.logicalcpu 2>/dev/null || nproc 2>/dev/null || echo 4)"
export RAYON_NUM_THREADS="$NCPUS"

if [ "$OS" = "Darwin" ]; then
    PLATFORM="macOS"
else
    PLATFORM="Linux"
fi

# ── Cargo / Rust ─────────────────────────────────────────────────────────────
# shellcheck disable=SC1091
. "$HOME/.cargo/env" 2>/dev/null || true
command -v cargo >/dev/null 2>&1 || die "cargo not found — install Rust: https://rustup.rs"

# ── Data files ───────────────────────────────────────────────────────────────
WEIGHTS="$SCRIPT_DIR/data/brainjepa.safetensors"
GRADIENT="$SCRIPT_DIR/data/gradient_mapping_450.csv"
INPUT="$SCRIPT_DIR/data/test_fmri.safetensors"
OUTPUT="/tmp/brainjepa_bench_embeddings.safetensors"

[ -f "$WEIGHTS"  ] || die "Weights not found: $WEIGHTS"
[ -f "$GRADIENT" ] || die "Gradient mapping not found: $GRADIENT"
[ -f "$INPUT"    ] || die "Test fMRI not found: $INPUT"

# ── Define backends ──────────────────────────────────────────────────────────
# Each entry: label | cargo feature flags | binary suffix | infer --device arg
BACKENDS=()

if [ "$GPU_ONLY" = "0" ]; then
    BACKENDS+=("rlx-cpu|--no-default-features --features rlx-engine|rlx-cpu|cpu|infer")
    if [ "$PLATFORM" = "macOS" ]; then
        BACKENDS+=("rlx-accelerate|--no-default-features --features rlx-engine,rlx-blas-accelerate|rlx-accelerate|cpu|infer")
    fi
fi

if [ "$PLATFORM" = "macOS" ]; then
    BACKENDS+=("rlx-metal|--no-default-features --features rlx-engine,rlx-metal|rlx-metal|metal|infer")
    BACKENDS+=("rlx-mlx|--no-default-features --features rlx-engine,rlx-mlx|rlx-mlx|mlx|infer")
    BACKENDS+=("rlx-gpu|--no-default-features --features rlx-engine,rlx-gpu|rlx-gpu|gpu|infer")
else
    BACKENDS+=("rlx-gpu|--no-default-features --features rlx-engine,rlx-gpu|rlx-gpu|gpu|infer")
fi

# ── Build / results dirs (one target dir per backend — infer binary is shared name) ─
BENCH_ROOT=/tmp/brainjepa-bench
RESULTS_DIR="$BENCH_ROOT/results"

# ── Header ───────────────────────────────────────────────────────────────────
step "Brain-JEPA benchmark  —  $PLATFORM ($NCPUS threads)"
info "runs=$RUNS  no-build=$NO_BUILD  gpu-only=$GPU_ONLY"
info "weights  : $WEIGHTS"
info "gradient : $GRADIENT"
info "input    : $INPUT"

# ── Step 0: Tests ────────────────────────────────────────────────────────────
step "[0/4] Tests"

info "cargo test --no-default-features --features rlx-engine"
cargo test --no-default-features --features rlx-engine 2>&1 \
    | grep -E "^(test |running |test result:|error)" || true
cargo test --no-default-features --features rlx-engine --quiet || die "RLX tests failed"
ok "RLX unit tests"

if [ -x "$SCRIPT_DIR/scripts/parity.sh" ]; then
    info "bash scripts/parity.sh"
    bash "$SCRIPT_DIR/scripts/parity.sh" || die "parity gate failed"
    ok "parity"
fi

rm -rf "$RESULTS_DIR"
mkdir -p "$RESULTS_DIR"

# ── Step 1: Build ────────────────────────────────────────────────────────────
step "[1/4] Build"

build_backend() {
    local label="$1" features="$2" suffix="$3" bin_name="${4:-infer}"
    local bin_out="/tmp/brainjepa-${suffix}"

    if [ "$NO_BUILD" = "1" ]; then
        if [ -f "$bin_out" ]; then
            ok "$label: skipped (--no-build), binary exists"
            return 0
        else
            warn "$label: --no-build but no binary at $bin_out — skipping"
            return 1
        fi
    fi

    local target_dir="$BENCH_ROOT/target-$suffix"
    local build_log
    build_log=$(mktemp "${TMPDIR:-/tmp}/brainjepa_build.XXXXXX")
    info "Building $label ..."
    info "  cargo build --release $features --bin $bin_name"

    local mlx_jobs=""
    if [ -n "${CARGO_BUILD_JOBS:-}" ]; then
        mlx_jobs="$CARGO_BUILD_JOBS"
    elif [ "$suffix" = "rlx-mlx" ]; then
        mlx_jobs=2
    fi

    if [ -n "$mlx_jobs" ]; then
        _build() {
            CARGO_TARGET_DIR="$target_dir" CARGO_BUILD_JOBS="$mlx_jobs" \
                cargo build --release $features --bin "$bin_name"
        }
    else
        _build() {
            CARGO_TARGET_DIR="$target_dir" \
                cargo build --release $features --bin "$bin_name"
        }
    fi

    if ! _build >"$build_log" 2>&1; then
        grep -E "^(error|warning\[|   Compiling|    Finished)" "$build_log" || true
        warn "$label: build failed — skipping (see $build_log)"
        rm -f "$build_log"
        return 1
    fi
    grep -E "^(error|warning\[|   Compiling|    Finished)" "$build_log" || true
    rm -f "$build_log"

    if [ ! -f "$target_dir/release/$bin_name" ]; then
        warn "$label: binary missing after build — skipping"
        return 1
    fi

    cp "$target_dir/release/$bin_name" "$bin_out"
    chmod +x "$bin_out"
    ok "$label  ->  $bin_out"
    return 0
}

BUILT_BACKENDS=()
for entry in "${BACKENDS[@]}"; do
    IFS='|' read -r label features suffix _device bin_name <<< "$entry"
    bin_name="${bin_name:-infer}"
    if build_backend "$label" "$features" "$suffix" "$bin_name"; then
        BUILT_BACKENDS+=("$entry")
    fi
done

if [ ${#BUILT_BACKENDS[@]} -eq 0 ]; then
    die "No backends were built successfully."
fi

# ── Step 2: Benchmark ────────────────────────────────────────────────────────
step "[2/4] Benchmark  ($RUNS iterations each)"

result_file() { echo "$RESULTS_DIR/$1.$2"; }

run_backend() {
    local label="$1" suffix="$2" device="$3"
    local bin="/tmp/brainjepa-${suffix}"

    if [ ! -x "$bin" ]; then
        warn "$label: binary not found at $bin — skipping"
        return
    fi

    info ""
    info "--- $label ---"
    local best_encode=999999999
    local best_weights=999999999
    local best_total=999999999
    local all_encodes=""

    local device_args=()
    if [ -n "$device" ]; then
        device_args=(--device "$device")
    fi

    for i in $(seq 1 "$RUNS"); do
        info "  Run $i/$RUNS ..."

        local stderr_file
        stderr_file=$(mktemp /tmp/brainjepa_bench_stderr.XXXXXX)

        "$bin" \
            "${device_args[@]}" \
            --weights "$WEIGHTS" \
            --gradient "$GRADIENT" \
            --input "$INPUT" \
            --output "$OUTPUT" \
            2>"$stderr_file" || {
                warn "  Run $i failed"
                cat "$stderr_file" >&2 || true
                rm -f "$stderr_file"
                continue
            }

        local timing_line
        timing_line=$(grep '^TIMING ' "$stderr_file" 2>/dev/null || true)
        rm -f "$stderr_file"

        if [ -z "$timing_line" ]; then
            warn "  No TIMING line found in stderr"
            continue
        fi

        local w_ms e_ms t_ms
        w_ms=$(echo "$timing_line" | sed -n 's/.*weights=\([0-9.]*\)ms.*/\1/p')
        e_ms=$(echo "$timing_line" | sed -n 's/.*encode=\([0-9.]*\)ms.*/\1/p')
        t_ms=$(echo "$timing_line" | sed -n 's/.*total=\([0-9.]*\)ms.*/\1/p')

        info "  weights=${w_ms}ms  encode=${e_ms}ms  total=${t_ms}ms"

        if [ -n "$all_encodes" ]; then
            all_encodes="${all_encodes},${e_ms}"
        else
            all_encodes="${e_ms}"
        fi

        best_encode=$(awk "BEGIN { print ($e_ms < $best_encode) ? $e_ms : $best_encode }")
        best_weights=$(awk "BEGIN { print ($w_ms < $best_weights) ? $w_ms : $best_weights }")
        best_total=$(awk "BEGIN { print ($t_ms < $best_total) ? $t_ms : $best_total }")
    done

    if [ "$best_encode" = "999999999" ]; then
        warn "$label: no successful runs"
        return
    fi

    echo "$best_encode"  > "$(result_file "$suffix" encode)"
    echo "$best_weights" > "$(result_file "$suffix" weights)"
    echo "$best_total"   > "$(result_file "$suffix" total)"
    echo "$all_encodes"  > "$(result_file "$suffix" all)"

    ok "$label  best-of-${RUNS}:  encode=${best_encode}ms  weights=${best_weights}ms  total=${best_total}ms"
}

has_result() {
    [ -f "$(result_file "$1" encode)" ]
}

read_result() {
    cat "$(result_file "$1" "$2")" 2>/dev/null || echo ""
}

for entry in "${BUILT_BACKENDS[@]}"; do
    IFS='|' read -r label features suffix device <<< "$entry"
    run_backend "$label" "$suffix" "$device"
done

# ── Step 3: Summary table ────────────────────────────────────────────────────
step "[3/4] Summary"

any_results=0
for entry in "${BUILT_BACKENDS[@]}"; do
    IFS='|' read -r _label _features suffix _device <<< "$entry"
    if has_result "$suffix"; then any_results=1; break; fi
done
[ "$any_results" = "1" ] || die "No successful benchmark results."

baseline_encode=0
baseline_label=""
for entry in "${BUILT_BACKENDS[@]}"; do
    IFS='|' read -r label _features suffix _device <<< "$entry"
    if has_result "$suffix"; then
        enc=$(read_result "$suffix" encode)
        is_slower=$(awk "BEGIN { print ($enc > $baseline_encode) }")
        if [ "$is_slower" = "1" ]; then
            baseline_encode="$enc"
            baseline_label="$label"
        fi
    fi
done

info "Baseline (slowest): $baseline_label @ ${baseline_encode}ms"
info ""

printf '  \033[1m%-24s  %10s  %10s  %10s  %10s\033[0m\n' \
    "Backend" "Weights" "Encode" "Total" "Speedup"
printf '  %-24s  %10s  %10s  %10s  %10s\n' \
    "------------------------" "----------" "----------" "----------" "----------"

for entry in "${BUILT_BACKENDS[@]}"; do
    IFS='|' read -r label _features suffix _device <<< "$entry"
    if ! has_result "$suffix"; then
        printf '  %-24s  %10s  %10s  %10s  %10s\n' \
            "$label" "FAIL" "FAIL" "FAIL" "-"
        continue
    fi

    local_encode=$(read_result "$suffix" encode)
    local_weights=$(read_result "$suffix" weights)
    local_total=$(read_result "$suffix" total)
    speedup=$(awk "BEGIN { printf \"%.2f\", $baseline_encode / $local_encode }")

    if [ "$speedup" = "1.00" ]; then
        printf '  %-24s  %8sms  %8sms  %8sms  %9sx\n' \
            "$label" "$local_weights" "$local_encode" "$local_total" "$speedup"
    else
        printf '  \033[32m%-24s  %8sms  %8sms  %8sms  %9sx\033[0m\n' \
            "$label" "$local_weights" "$local_encode" "$local_total" "$speedup"
    fi
done

info ""
info "Per-run encode times (ms):"
for entry in "${BUILT_BACKENDS[@]}"; do
    IFS='|' read -r label _features suffix _device <<< "$entry"
    if has_result "$suffix"; then
        info "  $label: $(read_result "$suffix" all)"
    fi
done

step "[4/4] Done"
info "Platform : $PLATFORM  ($NCPUS threads)"
info "Runs     : $RUNS per backend"
info "Binary   : /tmp/brainjepa-{backend}"
ok "Benchmark complete."
