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

```rust
if let Some(stats) = skill_gpu::read() {
    println!("{}: {:.0} MB free", stats.name, stats.free_memory_bytes as f64 / 1e6);
}
```

## Dependencies

- `llmfit-core` — cross-platform GPU detection
- `sysinfo` — memory fallback
- `libc` — macOS IOKit FFI

Zero Tauri dependencies.
