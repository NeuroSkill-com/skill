#!/usr/bin/env python3
"""NeuroSkill's OWN inference: speed and output agreement, CPU vs GPU.

The rlx-level harnesses measure the runner. This measures what the app
actually ships — the daemon's HTTP path, including the chat template, the
sampler, stop handling and the SSE stream — and asks two questions a user
would care about:

  speed      TTFT and decode tok/s per device
  precision  does the SAME request at temperature 0 give the SAME text,
             (a) twice on one device (determinism) and
             (b) on CPU vs GPU (cross-device agreement)

(b) is the app-level analogue of the matrix's logit-delta column: a user does
not see logits, they see whether switching the device changed the answer.
"""
import os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from suite import Daemon

GGUF = sys.argv[1]
BASE_PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 8240
MAXTOK = int(os.environ.get("MAXTOK", 48))
CTX = int(os.environ.get("CTX", 2048))

PROMPTS = [
    "What is the capital of France? Answer in one short sentence.",
    "Name three prime numbers below 20.",
    "In one sentence, what does a mitochondrion do?",
]


class D(Daemon):
    """`Daemon.configure` pins metal; this needs the device as a parameter."""

    def configure_device(self, device):
        cfg = self.req("/v1/settings/llm-config")
        cfg.update(
            model_path=self.gguf,
            enabled=True,
            rlx_device=device,
            # skill resolves CPU from EITHER the device string or 0 gpu layers
            # (`resolve_rlx_device`); set both so the intent is unambiguous.
            n_gpu_layers=0 if device == "cpu" else 999,
            ctx_size=CTX,
        )
        cfg.setdefault("tools", {})
        cfg["tools"]["enabled"] = False
        self.req("/v1/settings/llm-config", cfg)


def measure(device, port):
    d = D(f"precision-{device}", GGUF, port, "release")
    runs = []
    try:
        d.start()
        d.configure_device(device)
        t = time.time()
        d.start_llm()
        load_s = time.time() - t
        for rep in range(2):
            for p in PROMPTS:
                # `chat_stream` already pins temperature=0.
                s = d.chat_stream([{"role": "user", "content": p}],
                                  max_tokens=MAXTOK)
                text = (s["text"] or "").strip()
                ttft = s["first_content"]
                decode = None
                if ttft is not None and s["chunks"] > 1:
                    dt = s["total"] - ttft
                    if dt > 0:
                        decode = (s["chunks"] - 1) / dt
                # One non-streaming call per prompt on rep 0, purely to check
                # that an SSE content delta is one token. If it is not, the
                # "tok/s" above is really "deltas/s" and must not be quoted as
                # throughput.
                usage_tokens = None
                if rep == 0:
                    a = d.chat([{"role": "user", "content": p}], max_tokens=MAXTOK)
                    usage_tokens = (a.get("usage") or {}).get("completion_tokens")
                runs.append({"rep": rep, "prompt": p, "text": text,
                             "ttft": ttft, "decode": decode,
                             "chunks": s["chunks"], "usage_tokens": usage_tokens})
        return load_s, runs, d.log_text()
    finally:
        d.stop()


def fmt(x, nd=2):
    return "   n/a" if x is None else f"{x:>6.{nd}f}"


def main():
    results = {}
    for i, device in enumerate(["metal", "cpu"]):
        print(f"\n######## {device} ########", flush=True)
        load_s, runs, log = measure(device, BASE_PORT + i)
        results[device] = runs
        fam = [l for l in log.splitlines() if "actor] ready" in l]
        ttfts = [r["ttft"] for r in runs if r["ttft"] is not None]
        decs = [r["decode"] for r in runs if r["decode"] is not None]
        print(f"  load {load_s:.1f}s  {fam[-1].split('—')[-1].strip() if fam else ''}")
        print(f"  TTFT   min {fmt(min(ttfts) if ttfts else None)}s  "
              f"median {fmt(sorted(ttfts)[len(ttfts)//2] if ttfts else None)}s")
        print(f"  decode max {fmt(max(decs) if decs else None,1)} tok/s  "
              f"median {fmt(sorted(decs)[len(decs)//2] if decs else None,1)} tok/s")
        # determinism: rep 0 vs rep 1, same device
        n = len(PROMPTS)
        same = sum(1 for j in range(n) if runs[j]["text"] == runs[j + n]["text"])
        print(f"  determinism (temp 0, same device, 2 runs): {same}/{n} identical")
        pairs = [(r["chunks"], r["usage_tokens"]) for r in runs
                 if r["usage_tokens"] is not None]
        if pairs:
            exact = sum(1 for c, u in pairs if c == u)
            print(f"  1 SSE delta == 1 token: {exact}/{len(pairs)}  "
                  f"(deltas vs usage: {pairs})")
        for j in range(n):
            if runs[j]["text"] != runs[j + n]["text"]:
                print(f"    DIFFERS: {runs[j]['prompt'][:40]!r}")
                print(f"      run1: {runs[j]['text'][:110]!r}")
                print(f"      run2: {runs[j+n]['text'][:110]!r}")

    print("\n######## CPU vs GPU agreement ########")
    agree = 0
    for j, p in enumerate(PROMPTS):
        a = results["metal"][j]["text"]
        b = results["cpu"][j]["text"]
        ok = a == b
        agree += ok
        print(f"  {'SAME' if ok else 'DIFF'}  {p[:46]!r}")
        if not ok:
            print(f"    metal: {a[:140]!r}")
            print(f"    cpu  : {b[:140]!r}")
    print(f"\n  {agree}/{len(PROMPTS)} prompts identical across devices")

    mt = [r["decode"] for r in results["metal"] if r["decode"]]
    ct = [r["decode"] for r in results["cpu"] if r["decode"]]
    if mt and ct:
        print(f"  speed: metal {max(mt):.1f} tok/s vs cpu {max(ct):.1f} tok/s "
              f"= {max(mt)/max(ct):.1f}x")


if __name__ == "__main__":
    main()
