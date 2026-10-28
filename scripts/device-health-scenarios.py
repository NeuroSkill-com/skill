#!/usr/bin/env python3
"""Device-health scenarios against a real skill-daemon and the virtual EEG source.

Walks one recording through the ways real sessions go wrong and checks that:
  - each problem is detected (`/v1/device/health`) with the right electrodes;
  - brain-state metrics keep going and leave bad electrodes out
    (`excluded_channels` in `/v1/activity/latest-bands`);
  - embeddings keep being written;
  - with a GGUF given, chat still answers the prompt and appends a device
    notice (full advice first, a short reminder after);
  - the brain-state templates move the dominant band as expected;
  - a 64-electrode cap with permanently faulty electrodes is reported, not
    blocked.

    python3 scripts/device-health-scenarios.py [--daemon PATH] [--gguf PATH] [--port N]

Runs isolated (own config/data dirs); never touches your profile. ~5 minutes.
"""
import argparse, json, os, shutil, subprocess, sys, tempfile, time, urllib.request

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
VIRTUAL = "lsl:SkillVirtualEEG"
results = []


def check(name, ok, detail=""):
    results.append((name, bool(ok)))
    print(f"  {'PASS' if ok else 'FAIL'}  {name}  {detail}", flush=True)


class Daemon:
    def __init__(self, exe, port, env):
        self.port = port
        self.dir = tempfile.mkdtemp(prefix="skill-health-")
        os.makedirs(f"{self.dir}/cfg"); os.makedirs(f"{self.dir}/data")
        e = dict(os.environ, SKILL_DAEMON_CONFIG_ROOT=f"{self.dir}/cfg", SKILL_DATA_DIR=f"{self.dir}/data",
                 SKILL_DAEMON_ADDR=f"127.0.0.1:{port}", **env)
        self.log = open(f"{self.dir}/daemon.log", "wb")
        self.proc = subprocess.Popen([exe], env=e, stdout=self.log, stderr=subprocess.STDOUT)
        for _ in range(60):
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{port}/healthz", timeout=1)
                break
            except Exception:
                time.sleep(1)
        else:
            raise RuntimeError("daemon did not come up")
        self.token = open(f"{self.dir}/cfg/auth.token").read().strip()

    def req(self, path, body=None, timeout=600):
        data = json.dumps(body).encode() if body is not None else None
        r = urllib.request.Request(f"http://127.0.0.1:{self.port}{path}", data=data,
                                   method="POST" if data is not None else "GET",
                                   headers={"Authorization": f"Bearer {self.token}",
                                            "Content-Type": "application/json"})
        return json.loads(urllib.request.urlopen(r, timeout=timeout).read())

    def stop(self):
        self.proc.terminate()
        try:
            self.proc.wait(10)
        except Exception:
            self.proc.kill()
        print(f"\n(daemon log: {self.dir}/daemon.log)")


def health(d):
    return d.req("/v1/device/health")


def codes(h):
    return {i["code"]: i for i in h.get("issues", [])}


def wait_for(d, pred, timeout=25, every=1.5):
    """Poll device health until `pred(health)` holds; returns the last health."""
    t = time.time()
    h = health(d)
    while time.time() - t < timeout:
        h = health(d)
        if pred(h):
            return h
        time.sleep(every)
    return h


def embeddings(d):
    now = int(time.time())
    return d.req("/v1/analysis/embedding-count", {"startUtc": now - 3600, "endUtc": now + 60}).get("embedded", 0)


def dominant_band(d):
    """Average relative power per band over the analysis (non-excluded) channels."""
    b = d.req("/v1/activity/latest-bands") or {}
    ex = set(b.get("excluded_channels", []))
    chans = [c for i, c in enumerate(b.get("channels", [])) if i not in ex]
    if not chans:
        return None, b
    bands = ["delta", "theta", "alpha", "beta", "gamma"]
    avg = {k: sum(c[f"rel_{k}"] for c in chans) / len(chans) for k in bands}
    return max(avg, key=avg.get), b


def configure(d, **cfg):
    r = d.req("/v1/lsl/virtual-source/config", cfg)
    assert r.get("ok"), r


def start_session(d, cfg):
    d.req("/v1/control/cancel-session", {})
    d.req("/v1/control/disable-reconnect", {})
    d.req("/v1/lsl/virtual-source/stop", {})
    time.sleep(1)
    r = d.req("/v1/lsl/virtual-source/start", cfg)
    assert r.get("running"), r
    time.sleep(2)
    d.req("/v1/control/start-session", {"target": VIRTUAL})
    for _ in range(30):
        s = d.req("/v1/status")
        if s.get("state") == "connected" and s.get("sample_count", 0) > 0:
            return s
        time.sleep(1)
    raise RuntimeError(f"session never connected: {s.get('state')}")


def chat(d, text):
    r = d.req("/v1/chat/completions", {"messages": [{"role": "user", "content": text}],
                                        "params": {"max_tokens": 200, "temperature": 0, "thinking_budget": 0}})
    return ((r.get("choices") or [{}])[0].get("message") or {}).get("content") or "", r.get("error")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--daemon", default=f"{ROOT}/src-tauri/target/release/skill-daemon")
    ap.add_argument("--gguf", help="enable chat checks with this model")
    ap.add_argument("--port", type=int, default=18571)
    args = ap.parse_args()

    d = Daemon(args.daemon, args.port, {"SKILL_VIRTUAL_EMBED": "1", "SKILL_PERSISTENT_FAULT_SECS": "20"})
    try:
        time.sleep(6)  # let boot-time auto-connect settle before taking over
        # Embedding checks need the EEG encoder weights; fetch them the way the
        # app does if this machine doesn't have them yet.
        if not d.req("/v1/models/status").get("weights_found"):
            print("  (downloading EEG encoder weights …)", flush=True)
            d.req("/v1/models/trigger-weights-download", {})
            for _ in range(300):
                st = d.req("/v1/models/status")
                if st.get("weights_found") and not st.get("downloading_weights"):
                    break
                time.sleep(2)
            check("EEG encoder weights available", d.req("/v1/models/status").get("weights_found"))
        if args.gguf:
            cfg = d.req("/v1/settings/llm-config")
            cfg.update(model_path=args.gguf, enabled=True)
            cfg.setdefault("tools", {})["enabled"] = False
            d.req("/v1/settings/llm-config", cfg)
            d.req("/v1/llm/server/start", {})

        print("\n### 32-electrode cap, clean relaxed signal")
        start_session(d, {"channels": 32, "template": "relaxed"})
        h = wait_for(d, lambda h: h.get("connected"), 15)
        time.sleep(8)
        h = health(d)
        check("healthy device has no issues", not h["issues"], json.dumps(h["issues"])[:200])
        check("all 32 electrodes usable", len(h["usable_channels"]) == 32, f"{len(h['usable_channels'])}")

        print("\n### brain-state templates move the dominant band")
        for template, band in [("relaxed", "alpha"), ("focused", "beta"), ("drowsy", "theta"), ("sleep", "delta")]:
            configure(d, template=template)
            time.sleep(7)  # 2 s analysis window + margin
            dom, _ = dominant_band(d)
            check(f"{template} → {band} dominant", dom == band, f"got {dom}")
        configure(d, template="relaxed")

        print("\n### one dead electrode (Fp1 flat)")
        emb0 = embeddings(d)
        configure(d, template="relaxed", faults=[{"channel": "Fp1", "kind": "flat"}])
        h = wait_for(d, lambda h: "no_signal" in codes(h))
        ns = codes(h).get("no_signal", {})
        check("no_signal reported on Fp1", ns.get("channels") == ["Fp1"], json.dumps(ns)[:220])
        check("advice is specific", "forehead" in ns.get("advice", "").lower(), ns.get("advice", ""))
        check("31 of 32 electrodes usable", len(h["usable_channels"]) == 31)
        _, bands = dominant_band(d)
        check("Fp1 left out of brain-state metrics", 0 in bands.get("excluded_channels", []),
              f"excluded={bands.get('excluded_channels')}")
        if args.gguf:
            for _ in range(60):
                if d.req("/v1/llm/server/status").get("status") == "running":
                    break
                time.sleep(2)
            ans, err = chat(d, "In one sentence, what is alpha brain activity?")
            check("chat still answers the prompt", not err and len(ans.split("---")[0].strip()) > 20, ans[:120])
            check("chat reply carries the device notice with advice",
                  "Device check" in ans and "Fp1" in ans and "forehead" in ans.lower(), ans[-260:].replace("\n", " "))
            ans2, _ = chat(d, "Name one benefit of meditation in one sentence.")
            check("second reply: short reminder, not the full advice",
                  "Fp1" in ans2 and "forehead" not in ans2.split("---")[-1].lower(), ans2[-200:].replace("\n", " "))

        print("\n### the same electrode stays dead → persistent fault (20 s in this test)")
        h = wait_for(d, lambda h: "faulty_electrodes" in codes(h), 35)
        fe = codes(h).get("faulty_electrodes", {})
        check("faulty_electrodes reported", fe.get("channels") == ["Fp1"], fe.get("message", ""))
        time.sleep(6)
        check("embeddings keep being written with a dead electrode", embeddings(d) > emb0,
              f"{emb0} → {embeddings(d)}")

        print("\n### several bad contacts at once")
        configure(d, template="relaxed", faults=[{"channel": "T7", "kind": "noisy"},
                                                 {"channel": "O1", "kind": "line_noise"},
                                                 {"channel": "Cz", "kind": "railed"}])
        h = wait_for(d, lambda h: set(codes(h).get("poor_contact", {}).get("channels", [])) >= {"T7", "O1", "Cz"})
        pc = codes(h).get("poor_contact", {})
        check("poor_contact on T7, O1, Cz", set(pc.get("channels", [])) >= {"T7", "O1", "Cz"}, pc.get("message", ""))
        check("Fp1 recovered once its fault was removed", "Fp1" in h["usable_channels"])

        print("\n### most electrodes fail together → check the reference")
        labels = d.req("/v1/status")["channel_names"]
        configure(d, template="relaxed", faults=[{"channel": l, "kind": "noisy"} for l in labels[:20]])
        h = wait_for(d, lambda h: "reference_suspect" in codes(h))
        rs = codes(h).get("reference_suspect", {})
        check("reference_suspect raised", bool(rs), rs.get("advice", ""))

        print("\n### battery drains (simulated: 25%, −60%/min)")
        configure(d, template="relaxed", battery={"percent": 25, "drain_per_min": 60})
        h = wait_for(d, lambda h: "battery_low" in codes(h), 20)
        check("battery_low", "battery_low" in codes(h), codes(h).get("battery_low", {}).get("message", ""))
        h = wait_for(d, lambda h: "battery_critical" in codes(h), 20)
        check("battery_critical", "battery_critical" in codes(h), codes(h).get("battery_critical", {}).get("message", ""))
        if args.gguf:
            ans, _ = chat(d, "Say hello in one short sentence.")
            check("chat warns about the battery", "battery" in ans.lower(), ans[-200:].replace("\n", " "))

        print("\n### blinks / jaw clenches + a loose electrode that keeps cutting out (Fp2)")
        configure(d, template="artifacts", faults=[{"channel": "Fp2", "kind": "intermittent"}])
        h = wait_for(d, lambda h: "intermittent_contact" in codes(h), 45)
        ic = codes(h).get("intermittent_contact", {})
        check("intermittent_contact on Fp2", ic.get("channels") == ["Fp2"], ic.get("message", ""))
        check("battery simulation removed → no battery issue",
              not any(c.startswith("battery") for c in codes(h)), str(list(codes(h))))
        others = [c for c in codes(h) if c != "intermittent_contact"]
        check("blinks/clenches alone raise no alarm", not others, str(others))

        print("\n### normal-amplitude noise the amplitude rules miss: mains hum (O1) and muscle noise (Fp1)")
        configure(d, template="relaxed", faults=[{"channel": "O1", "kind": "hum"}, {"channel": "Fp1", "kind": "emg"}])
        h = wait_for(d, lambda h: set(codes(h).get("fair_contact", {}).get("channels", [])) >= {"O1", "Fp1"}, 40)
        fc = codes(h).get("fair_contact", {})
        check("fair_contact on O1 (hum) and Fp1 (emg)", set(fc.get("channels", [])) >= {"O1", "Fp1"}, fc.get("message", ""))
        check("fair electrodes stay in the analysis", {"O1", "Fp1"} <= set(h["usable_channels"]))
        check("fair_contact is info, not an alarm", fc.get("severity") == "info")

        print("\n### everything fixed → issues clear")
        configure(d, template="relaxed")
        h = wait_for(d, lambda h: not h["issues"], 25)
        check("issues clear after recovery", not h["issues"], json.dumps(h["issues"])[:200])

        print("\n### 64-electrode cap with three permanently faulty electrodes")
        start_session(d, {"channels": 64, "template": "relaxed",
                          "faults": [{"channel": "T7", "kind": "flat"}, {"channel": "P8", "kind": "noisy"},
                                     {"channel": "Iz", "kind": "flat"}]})
        h = wait_for(d, lambda h: "faulty_electrodes" in codes(h), 45)
        fe = codes(h).get("faulty_electrodes", {})
        check("64-ch: faults reported", set(fe.get("channels", [])) == {"T7", "P8", "Iz"}, fe.get("message", ""))
        check("64-ch: 61 of 64 usable", len(h["usable_channels"]) == 61 and h["total_channels"] == 64,
              f"{len(h['usable_channels'])}/{h['total_channels']}")
        # T7 (index 11) is among the 32 analysed channels; P8 is index 26; Iz is 63.
        check("64-ch: analysis count is honest (first 32, 30 usable)", h.get("analysed_channels") == 30,
              f"analysed={h.get('analysed_channels')} limit={h.get('analysis_limit')}")
        emb1 = embeddings(d)
        time.sleep(12)
        check("64-ch: embeddings keep flowing", embeddings(d) > emb1)
        if args.gguf:
            ans, err = chat(d, "In one sentence, what does a 64-electrode EEG cap measure?")
            check("64-ch: chat answers + reports faulty electrodes",
                  not err and "faulty" in ans.lower() and len(ans.split("---")[0].strip()) > 20,
                  ans[-220:].replace("\n", " "))
    finally:
        d.stop()

    failed = [n for n, ok in results if not ok]
    print(f"\n{len(results) - len(failed)} passed, {len(failed)} failed")
    if failed:
        print("failed: " + "; ".join(failed))
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
