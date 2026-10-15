#!/usr/bin/env python3
"""Can the model still pick the right skill from each rendering preset?

`eval_skill_retrieval` (Rust) scores the *selection* half — whether the right
skill survives the top-N cut. This scores the other half: given a preset's
rendering of the skills, does the model actually choose correctly?

That is what the preset pros/cons in Settings assert, and it cannot be
answered without a model in the loop.

    python3 scripts/eval-skill-presets.py <gguf> [port]

Reuses the daemon driver from `llm-daemon-e2e/suite.py`, so it launches and
configures the real daemon exactly as the working e2e suite does.
"""
import json, os, re, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts", "llm-daemon-e2e"))
from suite import Daemon  # noqa: E402

GGUF = sys.argv[1]
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 18488

# The hard tier from the Rust selection eval, so the two halves are
# comparable: natural phrasing that avoids each skill's own vocabulary.
QUERIES = [
    ("help me wind down before bed", "neuroskill-protocols-sleep"),
    ("i cannot stop picking up my phone", "neuroskill-protocols-digital"),
    ("my neck aches after sitting all day", "neuroskill-protocols-body"),
    ("i am furious with my colleague", "neuroskill-protocols-emotions"),
    ("my nose is blocked, is there another way", "neuroskill-protocols-breathfree"),
    ("what should i eat to avoid the afternoon crash", "neuroskill-protocols-nutrition"),
    ("my heart is pounding and i feel wound up", "neuroskill-protocols-stress"),
    ("i want to pay attention while i eat", "neuroskill-protocols-mindfulness"),
    ("what should i do first thing after waking", "neuroskill-protocols-routines"),
    ("my teenager cannot concentrate on homework", "neuroskill-protocols-life"),
    ("find pictures from around three in the afternoon", "neuroskill-screenshots"),
    ("read it out to me instead", "neuroskill-streaming"),
    ("when have i felt like this before", "neuroskill-search"),
    ("give me a copy-pasteable shell example", "neuroskill-recipes"),
    ("how do i tell whether this is helping me", "neuroskill-evidence"),
]
MODES = ["full", "compact", "grouped", "brief", "names_only"]

# The configuration that actually ships: semantic top-8, rendered compact.
# Measured alongside the whole-catalogue presets because the product never
# shows all 30 — comparing only the full-catalogue renderings would score a
# pipeline nobody runs.
SELECTED = "semantic top-8 + compact"

ASK = ("\n\nReply with ONLY the name of the single most relevant skill from the "
       "list above, exactly as written. No explanation.")


def selected_blocks(top_n=8):
    """Per-query block from the Rust side: semantic top-N, rendered compact."""
    out = subprocess.run(
        ["cargo", "run", "-q", "--release", "-p", "skill-daemon-state",
         "--features", "text-embeddings-rlx-metal", "--bin", "eval_skill_retrieval",
         "--", "skills/skills", "--dump-selected", "--top-n", str(top_n)],
        cwd=ROOT, capture_output=True, text=True, check=True)
    return json.loads(out.stdout.strip().splitlines()[-1])


def render(mode):
    """The exact block the daemon would inject, from the Rust renderer."""
    return subprocess.run(
        ["cargo", "run", "-q", "-p", "skill-skills", "--example", "render_mode",
         "--", "skills/skills", mode],
        cwd=ROOT, capture_output=True, text=True, check=True).stdout


def clean(txt):
    return re.sub(r"<think>.*?</think>", "", txt or "", flags=re.S).strip()


def main():
    d = Daemon("preset-eval", GGUF, PORT)
    d.start()
    try:
        d.configure(tools=False)
        d.start_llm()
        print(f"\n{len(QUERIES)} queries x {len(MODES)} presets, model={os.path.basename(GGUF)}\n")
        print(f"  {'configuration':<24} {'correct':>10}  {'accuracy':>9}  {'block tok':>10}")
        for mode in MODES:
            block = render(mode)
            hits, wrong = 0, []
            for q, want in QUERIES:
                try:
                    out = clean(d.chat(
                        [{"role": "system", "content": block + ASK},
                         {"role": "user", "content": q}],
                        max_tokens=32)["content"])
                except Exception as e:
                    out = f"<error {e}>"
                if want in out:
                    hits += 1
                else:
                    wrong.append((q, want, out[:48]))
            print(f"  {mode:<24} {hits:>5}/{len(QUERIES):<4} {100*hits/len(QUERIES):>8.0f}%  {len(block)//4:>10}")
            if os.environ.get("VERBOSE"):
                for q, want, got in wrong:
                    print(f"       ✗ {q!r}\n         want {want}  got {got!r}")

        # The shipping path: a different, shorter block per query.
        blocks = selected_blocks()
        hits, toks = 0, []
        for q, want in QUERIES:
            block = blocks[q]
            toks.append(len(block) // 4)
            try:
                out = clean(d.chat(
                    [{"role": "system", "content": block + ASK},
                     {"role": "user", "content": q}],
                    max_tokens=32)["content"])
            except Exception as e:
                out = f"<error {e}>"
            if want in out:
                hits += 1
        print(f"  {SELECTED:<24} {hits:>5}/{len(QUERIES):<4} {100*hits/len(QUERIES):>8.0f}%  "
              f"{sum(toks)//len(toks):>10}")
    finally:
        d.stop()


main()
