#!/usr/bin/env python3
"""What a long system prompt costs, in prefill AND in decode.

RELIABILITY, measured over four runs on one machine:

* **prefill is trustworthy** — monotonic in prompt length and stable run to
  run at ~1.5-2.3 ms/token.
* **decode throughput is NOT** at this granularity. rlx compiles decode at
  fixed context buckets, so throughput is step-wise rather than smooth, and
  the steps move between runs: `brief` (1,451 tok) measured 15.0 tok/s once
  and 42.3 another; `names_only` (331 tok) 27.8 then 20.6. The broad trend is
  real and repeatable — 40+ tok/s at small prompts against ~9 at 2.2k+ — but
  do not quote per-preset decode deltas or derive a crossover point from a
  single run.

A big skills block is charged twice: once to prefill it, and then again on
every generated token, because each one attends over the whole context. A
measurement that only reports time-to-first-token sees half the bill.

    python3 scripts/bench-prefill-vs-prompt.py <gguf> [port]
"""
import os, subprocess, sys, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts", "llm-daemon-e2e"))
from suite import Daemon  # noqa: E402

GGUF = sys.argv[1]
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 18492
SMART_BLOCK = ""  # filled in main() from the real selector
GEN = int(os.environ.get("GEN", 96))   # tokens of output per sample
REPS = int(os.environ.get("REPS", 3))


def tool_block():
    """The stable half of the system prompt — identical on every turn."""
    return subprocess.run(
        ["cargo", "run", "-q", "-p", "skill-tools", "--example", "dump_tool_block"],
        cwd=ROOT, capture_output=True, text=True, check=True).stdout


def block(mode):
    """Just the skills block.

    NOT the tool block too, even though production sends both. Inlining the
    tool definitions into a plain `system` message does not reproduce the
    daemon's tool path — the model starts emitting tool-call-shaped output and
    the decode figures collapse to a tenth of what the same prompt length
    measures without it (1.0 tok/s at 2,465 tokens against 8.7 at 2,214), with
    some replies producing no tokens at all. Those numbers describe the
    harness, not the product. Measure the tool half through the real
    orchestration path (`scripts/llm-daemon-e2e/`) instead; use
    `cargo run -p skill-tools --example dump_tool_block | wc -c` for its size.
    """
    return subprocess.run(
        ["cargo", "run", "-q", "-p", "skill-skills", "--example", "render_mode",
         "--", "skills/skills", mode],
        cwd=ROOT, capture_output=True, text=True, check=True).stdout


def sample(d, sys_prompt, max_tokens):
    """Fastest of REPS — least polluted by other work on the machine."""
    msgs = ([{"role": "system", "content": sys_prompt}] if sys_prompt else []) + [
        {"role": "user", "content": "Count slowly from one to forty in words."}
    ]
    best, toks, out = None, 0, 0
    for _ in range(REPS):
        t = time.time()
        r = d.chat(msgs, max_tokens=max_tokens)
        dt = time.time() - t
        if best is None or dt < best:
            best = dt
            toks = r["usage"].get("prompt_tokens", 0)
            out = r["usage"].get("completion_tokens", 0)
    return best, toks, out


def main():
    global SMART_BLOCK
    SMART_BLOCK = subprocess.run(
        ["cargo", "run", "-q", "--release", "-p", "skill-daemon-state",
         "--features", "text-embeddings-rlx-metal", "--bin", "eval_skill_retrieval",
         "--", "skills/skills", "--dump-selected", "--top-n", "8"],
        cwd=ROOT, capture_output=True, text=True, check=True).stdout
    import json as _json
    SMART_BLOCK = next(iter(_json.loads(SMART_BLOCK.strip().splitlines()[-1]).values()))

    d = Daemon("prefill-bench", GGUF, PORT)
    d.start()
    try:
        d.configure(tools=False)
        d.start_llm()
        cases = [("(no system prompt)", "")] + [
            (m, block(m)) for m in ["names_only", "brief", "grouped", "compact", "full"]
        ]
        # `smart` selects per message; stand in with a real selected block.
        cases.insert(1, ("smart (~8 skills)", SMART_BLOCK))

        rows = []
        for label, sp in cases:
            ttft, ptok, _ = sample(d, sp, 1)          # prefill only
            total, _, otok = sample(d, sp, GEN)        # prefill + decode
            decode_s = max(total - ttft, 1e-6)
            rows.append((label, ptok, ttft, otok, decode_s, total))

        base_ttft, base_ptok = rows[0][2], rows[0][1]
        print(f"\n  best of {REPS}; decode normalised to a {GEN}-token answer\n")
        print(f"  {'system prompt':<20} {'ptok':>6} {'prefill':>9} {'ms/ptok':>8} "
              f"{'tok/s':>8} {'decode':>9} {'turn':>8}")
        norm = []
        for label, ptok, ttft, otok, dec, _total in rows:
            # Throughput, not wall time: the model emits a different number of
            # tokens per case, so comparing decode seconds compares unequal
            # work. Normalise to a fixed-length answer instead.
            # A reply that stopped immediately gives no throughput sample;
            # reporting 0 tok/s (or dividing by it) would be worse than
            # admitting the row is missing.
            if otok == 0:
                print(f"  {label:<24} {ptok:>6} {ttft:>8.2f}s {'—':>7} {'—':>8} {'—':>9} {'(no output tokens)'}")
                continue
            tps = otok / dec
            dec_norm = GEN / tps
            marginal = (ttft - base_ttft) * 1000.0 / max(ptok - base_ptok, 1)
            norm.append((label, ptok, ttft, tps, dec_norm, ttft + dec_norm))
            print(f"  {label:<20} {ptok:>6} {ttft:>8.2f}s {marginal:>7.2f} "
                  f"{tps:>7.1f} {dec_norm:>8.2f}s {ttft + dec_norm:>7.2f}s")

        print()
        if len(norm) < 2:
            return
        b = norm[0]
        for label, _ptok, ttft, _tps, dec_norm, turn in norm[1:]:
            print(f"  {label:<20} +{ttft - b[2]:>5.2f}s prefill  "
                  f"+{dec_norm - b[4]:>5.2f}s decode  = +{turn - b[5]:>5.2f}s per turn")
    finally:
        d.stop()


main()
