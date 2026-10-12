#!/usr/bin/env python3
"""Long end-to-end run: one model, the full set of realistic chat patterns."""
import json, os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from suite import Daemon, Report

LABEL, GGUF, PORT = sys.argv[1], sys.argv[2], int(sys.argv[3])
PROFILE = sys.argv[4] if len(sys.argv) > 4 else "release"
TOOLS   = (sys.argv[5] == "tools") if len(sys.argv) > 5 else False
ENV     = json.loads(sys.argv[6]) if len(sys.argv) > 6 else {}

TURNS      = int(os.environ.get("TURNS", 8))
LONG_CAP   = int(os.environ.get("LONG_CAP", 384))
SOAK_SECS  = int(os.environ.get("SOAK_SECS", 120))

r = Report()
print(f"\n################ {LABEL}  ({GGUF.split('/')[-1]}, {PROFILE}, tools={'on' if TOOLS else 'off'}) ################", flush=True)
d = Daemon(LABEL, GGUF, PORT, PROFILE, ENV)
try:
    d.start();                       r.check("daemon bound its port", True, f"pid {d.proc.pid} :{PORT}")
    d.configure(tools=TOOLS)
    d.mark("load"); t = time.time(); d.start_llm()
    r.check("LLM reached running", True, f"{time.time()-t:.1f}s to load")
    fam = [l for l in d.log_text().splitlines() if "actor] ready" in l]
    r.check("actor logged ready", bool(fam), fam[-1].split("—")[-1].strip() if fam else "")
    d.mark("ready")

    # 1 ── a real single turn with a realistic budget, and a quality signal.
    d.mark("single")
    a = d.chat([{"role": "user", "content": "What is the capital of France? Answer in one short sentence."}],
               max_tokens=LONG_CAP, thinking_budget=0)
    r.note(json.dumps({k: a[k] for k in ("finish", "usage", "error")}))
    r.note(f"content[:160]={a['content'][:160]!r}  ({a['secs']:.1f}s)")
    r.check("single turn: no error", not a["error"], str(a["error"] or ""))
    r.check("single turn: NON-EMPTY content", bool((a["content"] or "").strip()),
            f"{len(a['content'] or '')} chars for {a['usage'].get('completion_tokens')} tokens")
    r.check("single turn: answer mentions Paris", "paris" in (a["content"] or "").lower(),
            "quality signal, not a hard correctness bar")
    prompt_toks_nosys = a["usage"].get("prompt_tokens", 0)
    r.note(f"prompt_tokens for a 1-line question = {prompt_toks_nosys}")

    # 2 ── streaming: the path the UI consumes. Must arrive incrementally.
    d.mark("stream")
    s = d.chat_stream([{"role": "user", "content": "List three colours, one per line."}],
                      max_tokens=LONG_CAP, thinking_budget=0)
    fc = s["first_content"]; fa = s["first_any"]
    r.note(f"chunks={s['chunks']} phase={s['phases']} reasoning={s['reasoning']} "
           f"first_any={fa and round(fa,2)}s first_content={fc and round(fc,2)}s total={s['total']:.1f}s done={s['done']}")
    r.note(f"streamed[:160]={s['text'][:160]!r}")
    r.check("stream: got [DONE]", s["done"])
    r.check("stream: more than one content delta", s["chunks"] > 1, f"{s['chunks']} chunks")
    r.check("stream: non-empty streamed text", bool(s["text"].strip()))
    # The honest test of "is this really streaming": SOMETHING must reach the
    # client well before the response ends. With tools on, the first visible
    # content legitimately waits for a tool round-trip, so content-only timing
    # cannot distinguish buffering from tool latency — `first_any` can, because
    # the daemon emits phase deltas while it works.
    # Asserted with tools on OR off. It only became meaningful for tools-on once
    # the daemon started forwarding the tool lifecycle onto the `phase` channel:
    # before that a tool round-trip sent nothing at all (21.1s of a 21.6s
    # request), so this could not tell a silent tool call from a buffering
    # server. Now `phase: "tool"` lands immediately and the check has teeth in
    # both configurations.
    # Only meaningful when there is a wait worth covering. On a fast response
    # (prefill_seq tuned down, this drops to ~1.2s end to end) the first delta
    # legitimately lands in the last 10% simply because there is no gap to fill,
    # and asserting against that reports a problem that is not there.
    if s["total"] >= 2.0:
        r.check("stream: something reached the client before the end",
                fa is not None and fa < s["total"] * 0.9,
                f"first_any={fa and round(fa,2)}s of {s['total']:.1f}s")
    else:
        r.note(f"response finished in {s['total']:.1f}s — too fast for a "
               f"time-to-first-byte assertion to mean anything")
    if fc is not None and fa is not None and fc - fa > 2.0:
        r.note(f"first CONTENT delta at {fc:.1f}s vs first signal at {fa:.1f}s — "
               f"{s['phases']} phase delta(s) covered the gap")

    # 3 ── the chat page's actual pattern: a max_tokens=0 warm pass before every
    #      turn, with history accumulating. This is what context growth looks
    #      like in the product, and what broke at 256 tokens.
    d.mark("multiturn")
    history, ok_turns, growth = [], 0, []
    for i in range(TURNS):
        q = f"Turn {i+1}: name a country starting with the letter {chr(ord('A')+i)}. One short sentence."
        warm = d.chat(history + [{"role": "user", "content": q}],
                      max_tokens=0, top_k=1, top_p=1, thinking_budget=0)
        if warm["error"]:
            r.check(f"turn {i+1}: warm pass", False, str(warm["error"])); break
        t = d.chat(history + [{"role": "user", "content": q}],
                   max_tokens=LONG_CAP, thinking_budget=0)
        if t["error"] or not (t["content"] or "").strip():
            r.check(f"turn {i+1}", False, f"error={t['error']} content={t['content']!r}"); break
        history += [{"role": "user", "content": q},
                    {"role": "assistant", "content": t["content"]}]
        growth.append(t["usage"].get("prompt_tokens", 0)); ok_turns += 1
        r.note(f"turn {i+1}: prompt={growth[-1]} completion={t['usage'].get('completion_tokens')} "
               f"{t['secs']:.1f}s  {(t['content'] or '')[:70]!r}")
    r.check(f"all {TURNS} warm-pass + turn pairs succeeded", ok_turns == TURNS, f"{ok_turns}/{TURNS}")
    if growth:
        # Only meaningful with tools off. The tool injector sizes its system
        # prompt against `n_ctx` and TRIMS it as the conversation grows, so with
        # tools on the reported prompt can legitimately shrink turn over turn
        # (measured: 5384 → 5297) even though the history is getting longer.
        if TOOLS:
            r.note(f"tools on: prompt tokens {growth[0]} → {growth[-1]} "
                   f"(tool prompt is trimmed as history grows; not a growth signal)")
        elif len(growth) < 2:
            # One turn cannot demonstrate growth.
            r.note(f"single turn ({growth[0]} prompt tokens) — no growth to check")
        else:
            r.check("context actually grew across turns", growth[-1] > growth[0],
                    f"{growth[0]} → {growth[-1]} prompt tokens")

    # 4 ── sustained generation, for a throughput number and a longer decode.
    d.mark("long-gen")
    g = d.chat([{"role": "user", "content": "Write a short paragraph about the ocean."}],
               max_tokens=LONG_CAP, thinking_budget=0)
    ct = g["usage"].get("completion_tokens", 0)
    r.note(f"long gen: {ct} tokens in {g['secs']:.1f}s = {ct/max(g['secs'],1e-9):.1f} tok/s, finish={g['finish']}")
    r.check("long generation produced many tokens", ct >= 32, f"{ct} tokens")
    r.check("long generation non-empty", bool((g["content"] or "").strip()))

    # 5 ── concurrent requests. The actor is single-threaded, so these queue;
    #      none of them may error or be dropped.
    d.mark("concurrent")
    import concurrent.futures as cf
    with cf.ThreadPoolExecutor(3) as ex:
        futs = [ex.submit(d.chat, [{"role": "user", "content": f"Say the number {n}."}],
                          max_tokens=48, thinking_budget=0) for n in (1, 2, 3)]
        res = [f.result() for f in futs]
    r.check("3 concurrent requests all answered",
            all(not x["error"] and (x["content"] or "").strip() for x in res),
            " | ".join(f"{x['usage'].get('completion_tokens')}t" for x in res))

    # 6 ── idle soak, then prove the server is still usable.
    d.mark("soak")
    rss_before = d.rss[-1][1] if d.rss else 0
    time.sleep(SOAK_SECS)
    rss_after = d.rss[-1][1] if d.rss else 0
    r.note(f"idle {SOAK_SECS}s: RSS {rss_before/2**30:.2f} → {rss_after/2**30:.2f} GB")
    d.mark("post-soak")
    z = d.chat([{"role": "user", "content": "Still there? Reply with one word."}],
               max_tokens=48, thinking_budget=0)
    r.check("usable after the idle soak", not z["error"] and bool((z["content"] or "").strip()),
            f"{z['usage'].get('completion_tokens')} tokens")

    # 7 ── invariants over the whole run.
    log = d.log_text()
    r.check("daemon still alive", d.alive())
    r.check("status still running", d.req("/v1/llm/server/status").get("status") == "running")
    r.check("no 'weight not found' anywhere in the log", "weight not found" not in log)
    r.check("no panic in the log", "panicked" not in log)
    if "RLX generate panicked" in log:
        r.note("NOTE: the new guard CAUGHT a runner panic — see daemon.log")

finally:
    peak = max((v for _, v in d.rss), default=0)
    base = d.rss[0][1] if d.rss else 0
    print(f"\n  RSS: start {base/2**30:.2f} GB → peak {peak/2**30:.2f} GB", flush=True)
    for t, name in d.marks:
        near = min(d.rss, key=lambda s: abs(s[0] - t), default=(0, 0))
        print(f"        {name:<10} @{t:6.0f}s  rss {near[1]/2**30:5.2f} GB", flush=True)
    json.dump({"label": LABEL, "rss": d.rss, "marks": d.marks},
              open(f"{d.dir}/metrics.json", "w"))
    d.stop()
    r.summary()
    sys.exit(1 if r.failed else 0)
