#!/usr/bin/env python3
"""Raspberry Pi smoke test: the installed daemon, its default LLM, on the CPU.

Run inside a Pi-sized arm64 container by scripts/test-raspberry-pi-docker.sh
(or on a real Pi with SKILL_ROOT pointing at a tree whose
src-tauri/target/release/skill-daemon is the installed binary).

Checks what a Pi user would hit first:
  * `auto` lands on the CPU (the Pi policy in skill-accel), not wgpu
  * the model loads and answers three prompts, non-empty, no panic
  * peak daemon RSS stays under RSS_BUDGET_MB (default: 3/4 of the memory limit)
and prints load time, per-answer latency and decode tokens/s.
"""
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from suite import Daemon

GGUF = sys.argv[1]
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 18490
MAXTOK = int(os.environ.get("MAXTOK", 64))
CTX = int(os.environ.get("CTX", 2048))
BUDGET_MB = int(os.environ.get("RSS_BUDGET_MB", 0))

PROMPTS = [
    "What is the capital of France? Answer in one short sentence.",
    "Name three prime numbers below 20.",
    "In one sentence, what does an EEG measure?",
]


def main():
    d = Daemon("pi-smoke", GGUF, PORT, "release")
    failures = []
    try:
        d.start()
        cfg = d.req("/v1/settings/llm-config")
        # `auto`: the point is to see where the Pi policy sends it.
        cfg.update(model_path=GGUF, enabled=True, rlx_device="auto", ctx_size=CTX)
        if os.environ.get("PREFILL"):
            cfg["rlx_prefill_seq"] = int(os.environ["PREFILL"])
        cfg.setdefault("tools", {})
        cfg["tools"]["enabled"] = False
        d.req("/v1/settings/llm-config", cfg)

        t = time.time()
        d.start_llm()
        print(f"model load        : {time.time() - t:.1f} s")

        for p in PROMPTS:
            r = d.chat([{"role": "user", "content": p}], max_tokens=MAXTOK)
            toks = (r["usage"] or {}).get("completion_tokens") or 0
            text = (r["content"] or "").strip().replace("\n", " ")
            print(f"{r['secs']:6.1f} s  {toks:3d} tok  {toks / max(r['secs'], 1e-9):5.2f} tok/s  | {text[:90]}")
            if r["error"] or not text:
                failures.append(f"empty answer or error for {p!r}: {r['error']}")

        log = d.log_text()
        for line in log.splitlines():
            if "[qwen35][bench]" in line:
                print("rlx               :", line.split("] ", 1)[-1].strip())
        load_lines = [l for l in log.splitlines() if "Qwen35 load device=" in l]
        print("load line         :", load_lines[-1].strip() if load_lines else "(none)")
        if not any("device=Cpu" in l for l in load_lines):
            failures.append("the LLM did not load on the CPU")
        if "panicked" in log:
            failures.append("panic in the daemon log")
        if not d.alive():
            failures.append("daemon died")
    except Exception as e:  # noqa: BLE001 — report, then fail
        failures.append(f"{type(e).__name__}: {e}")
    finally:
        peak = max((r for _, r in d.rss), default=0) / 2**20
        d.stop()

    print(f"peak daemon RSS   : {peak:.0f} MB" + (f" (budget {BUDGET_MB} MB)" if BUDGET_MB else ""))
    if BUDGET_MB and peak > BUDGET_MB:
        failures.append(f"peak RSS {peak:.0f} MB over the {BUDGET_MB} MB budget")
    if failures:
        print("\nFAIL\n  " + "\n  ".join(failures))
        print("\n--- daemon log tail ---\n" + "\n".join(d.log_text().splitlines()[-40:]))
        sys.exit(1)
    print("\nPASS")


if __name__ == "__main__":
    main()
