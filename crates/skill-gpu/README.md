# skill-gpu

Cross-platform GPU utilisation and memory stats for NeuroSkill.

## Platforms

| Platform | Method | Utilisation | Memory |
|---|---|---|---|
| macOS (Apple Silicon) | IOKit `PerformanceStatistics` | Yes (EWMA) | Yes (unified) |
| macOS (Intel+discrete) | IOKit `IOAccelerator` | Yes | Yes (VRAM) |
| Linux (NVIDIA) | `nvidia-smi` / sysfs | No | Yes |
| Linux (AMD) | `rocm-smi` / sysfs | No | Yes |
| Windows | WMI / PowerShell | No | Yes |

## API

GPU utilisation + memory:

```rust
if let Some(stats) = skill_gpu::read() {
    println!("{}: {:.0} MB free", stats.name, stats.free_memory_bytes as f64 / 1e6);
}
```

Runtime **system-resource probe** — RAM, swap, free disk, CPU count, and power
state — for adapting model/runtime config to the actual hardware:

```rust
let r = skill_gpu::system_resources(Some(std::path::Path::new("/models")));
// r.free_ram_bytes, r.used_swap_bytes / r.total_swap_bytes (r.swap_used_fraction()),
// r.free_disk_bytes (statvfs on the given path), r.cpu_count,
// r.on_battery / r.low_power (best-effort: macOS IOKit / Linux sysfs)
```

Consumed by `skill-llm`'s `hardware_adaptive_plan` to shed the resident warm-up
graph / clamp context under memory or swap pressure, or on battery.

## Dependencies

- `llmfit-core` — cross-platform GPU detection
- `sysinfo` — memory fallback
- `libc` — macOS IOKit FFI

Zero Tauri dependencies.
