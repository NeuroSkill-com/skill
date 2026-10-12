#!/usr/bin/env python3
"""Long-running end-to-end suite against the REAL skill-daemon over HTTP.

Drives the daemon the way the app does — OpenAI-compatible POSTs, the chat
page's max_tokens=0 warm pass, SSE streaming, growing multi-turn history —
and samples RSS throughout so memory growth is attributable to a phase
rather than guessed at.
"""
import json, os, shutil, signal, socket, subprocess, sys, threading, time
import urllib.request, urllib.error

ROOT = os.environ.get("SKILL_ROOT", os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..")))
RUNS = os.environ.get("E2E_RUNS", "/tmp/skill-llm-daemon-e2e")

class Daemon:
    def __init__(self, label, gguf, port, profile="release", env=None):
        self.label, self.gguf, self.port, self.profile = label, gguf, port, profile
        self.dir = f"{RUNS}/{label}"
        self.extra_env = env or {}
        self.rss = []          # (elapsed_s, rss_bytes)
        self.marks = []        # (elapsed_s, name)
        self.proc = None
        self._stop = threading.Event()

    # ── lifecycle ──────────────────────────────────────────────────────────
    def start(self):
        exe = f"{ROOT}/src-tauri/target/{self.profile}/skill-daemon"
        if not os.path.exists(exe):
            raise RuntimeError(
                f"no daemon binary at {exe} — build it first:\n"
                f"  cargo build --release -p skill-daemon --no-default-features --features apple"
            )
        shutil.rmtree(self.dir, ignore_errors=True)
        os.makedirs(f"{self.dir}/config"); os.makedirs(f"{self.dir}/data")
        wait_port_free(self.port)
        env = dict(os.environ,
                   SKILL_DAEMON_CONFIG_ROOT=f"{self.dir}/config",
                   SKILL_DATA_DIR=f"{self.dir}/data",
                   SKILL_DAEMON_ADDR=f"127.0.0.1:{self.port}")
        env.update(self.extra_env)
        self.log = open(f"{self.dir}/daemon.log", "wb")
        self.proc = subprocess.Popen(
            [f"{ROOT}/src-tauri/target/{self.profile}/skill-daemon"],
            stdout=self.log, stderr=subprocess.STDOUT, env=env)
        self.t0 = time.time()
        threading.Thread(target=self._sample, daemon=True).start()
        for _ in range(120):
            if self.proc.poll() is not None:
                raise RuntimeError(f"daemon exited at startup (rc={self.proc.returncode})")
            if port_open(self.port): break
            time.sleep(1)
        else:
            raise RuntimeError("daemon never bound its port")
        self.token = open(f"{self.dir}/config/auth.token").read().strip()

    def _sample(self):
        import resource
        while not self._stop.is_set() and self.proc.poll() is None:
            try:
                out = subprocess.run(["ps", "-o", "rss=", "-p", str(self.proc.pid)],
                                     capture_output=True, text=True).stdout.strip()
                if out: self.rss.append((time.time() - self.t0, int(out) * 1024))
            except Exception: pass
            time.sleep(3)

    def mark(self, name): self.marks.append((time.time() - self.t0, name))

    def alive(self): return self.proc.poll() is None

    def stop(self):
        self._stop.set()
        if self.proc is None:
            # `start()` never got as far as spawning (missing binary, port in
            # use). Returning quietly keeps the real error visible instead of
            # burying it under an AttributeError from the `finally` block.
            return
        if self.proc.poll() is None:
            self.proc.send_signal(signal.SIGTERM)
            try: self.proc.wait(timeout=30)
            except subprocess.TimeoutExpired: self.proc.kill()
        self.log.close()

    # ── http ───────────────────────────────────────────────────────────────
    def req(self, path, body=None, timeout=1800, stream=False):
        url = f"http://127.0.0.1:{self.port}{path}"
        data = json.dumps(body).encode() if body is not None else None
        r = urllib.request.Request(url, data=data, method="POST" if data else "GET",
                                   headers={"Authorization": f"Bearer {self.token}",
                                            "Content-Type": "application/json"})
        resp = urllib.request.urlopen(r, timeout=timeout)
        return resp if stream else json.loads(resp.read())

    def log_text(self):
        return open(f"{self.dir}/daemon.log", errors="replace").read()

    # ── model ──────────────────────────────────────────────────────────────
    def configure(self, tools):
        cfg = self.req("/v1/settings/llm-config")
        cfg.update(model_path=self.gguf, enabled=True, rlx_device="metal", n_gpu_layers=999)
        # `CTX` pins ctx_size. The runners compile and RUN prefill at a fixed
        # `seq = max_seq`, so this is what actually sizes the prefill arena and
        # the time-to-first-token — not the auto-context ladder, which skill
        # clamps to 8192 anyway (so 16384 and 8192 produce identical runs).
        if os.environ.get("CTX"):
            cfg["ctx_size"] = int(os.environ["CTX"])
        # `PREFILL` pins the Qwen3.5 prefill-graph width independently of the
        # decode context — the point of the knob is that those two stop being
        # the same number.
        if os.environ.get("PREFILL"):
            cfg["rlx_prefill_seq"] = int(os.environ["PREFILL"])
        cfg.setdefault("tools", {})
        cfg["tools"]["enabled"] = tools
        self.req("/v1/settings/llm-config", cfg)

    def start_llm(self, timeout=900):
        self.req("/v1/llm/server/start", {})
        t = time.time()
        while time.time() - t < timeout:
            if not self.alive(): raise RuntimeError("daemon DIED while loading the model")
            s = self.req("/v1/llm/server/status").get("status")
            if s == "running": return
            time.sleep(2)
        raise RuntimeError(f"LLM never reached running (last={s})")

    def chat(self, messages, **params):
        p = dict(temperature=0, **params)
        t = time.time()
        d = self.req("/v1/chat/completions", {"messages": messages, "params": p})
        ch = (d.get("choices") or [{}])[0]
        return {"content": (ch.get("message") or {}).get("content"),
                "finish": ch.get("finish_reason"),
                "usage": d.get("usage") or {},
                "error": d.get("error"),
                "secs": time.time() - t}

    def chat_stream(self, messages, **params):
        """Stream via `curl -N` and time each arrival.

        NOT via urllib: `HTTPResponse` iteration buffers, so every delta looked
        like it arrived at the end and the harness blamed the daemon for
        buffering. `curl -N` with an unbuffered read is the only client here
        that measures the server's actual pacing — verified by hand against a
        live daemon (deltas ~85 ms apart).
        """
        p = dict(temperature=0, **params)
        body = json.dumps({"messages": messages, "params": p, "stream": True})
        t = time.time()
        proc = subprocess.Popen(
            ["curl", "-N", "-sS", "-H", f"Authorization: Bearer {self.token}",
             "-H", "Content-Type: application/json", "-X", "POST", "--data-binary", body,
             f"http://127.0.0.1:{self.port}/v1/chat/completions"],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, bufsize=1, text=True)
        chunks, first, text, done = 0, None, "", False
        first_any, phases, reasoning = None, 0, 0
        for line in proc.stdout:
            line = line.strip()
            if not line.startswith("data:"): continue
            payload = line[5:].strip()
            if first_any is None: first_any = time.time() - t
            if payload == "[DONE]": done = True; continue
            try: j = json.loads(payload)
            except Exception: continue
            dl = (j.get("choices") or [{}])[0].get("delta") or {}
            if dl.get("phase"): phases += 1
            if dl.get("reasoning_content"): reasoning += 1
            delta = dl.get("content")
            if delta:
                if first is None: first = time.time() - t
                chunks += 1; text += delta
        proc.wait(timeout=60)
        return {"chunks": chunks, "first_content": first, "first_any": first_any,
                "phases": phases, "reasoning": reasoning,
                "total": time.time() - t, "text": text, "done": done}

    def _chat_stream_urllib_unused(self, messages, **params):
        p = dict(temperature=0, **params)
        t = time.time()
        resp = self.req("/v1/chat/completions",
                        {"messages": messages, "params": p, "stream": True}, stream=True)
        chunks, first, text, done = 0, None, "", False
        first_any, phases, reasoning = None, 0, 0
        for raw in resp:
            line = raw.decode(errors="replace").strip()
            if not line.startswith("data:"): continue
            payload = line[5:].strip()
            if first_any is None: first_any = time.time() - t
            if payload == "[DONE]": done = True; continue
            try: j = json.loads(payload)
            except Exception: continue
            dl = (j.get("choices") or [{}])[0].get("delta") or {}
            if dl.get("phase"): phases += 1
            if dl.get("reasoning_content"): reasoning += 1
            delta = dl.get("content")
            if delta:
                if first is None: first = time.time() - t
                chunks += 1; text += delta
        return {"chunks": chunks, "first_content": first, "first_any": first_any,
                "phases": phases, "reasoning": reasoning,
                "total": time.time() - t, "text": text, "done": done}

# ── helpers ───────────────────────────────────────────────────────────────
def port_open(p):
    with socket.socket() as s:
        s.settimeout(0.5)
        return s.connect_ex(("127.0.0.1", p)) == 0

def wait_port_free(p, timeout=60):
    t = time.time()
    while port_open(p) and time.time() - t < timeout: time.sleep(1)
    if port_open(p): raise RuntimeError(f"port {p} still in use")

class Report:
    def __init__(self): self.rows = []
    def check(self, name, cond, detail=""):
        self.rows.append((bool(cond), name, detail))
        print(f"  {'PASS' if cond else 'FAIL'}  {name}" + (f"   {detail}" if detail else ""), flush=True)
    def note(self, s): print(f"        {s}", flush=True)
    @property
    def failed(self): return sum(1 for ok, _, _ in self.rows if not ok)
    def summary(self):
        p = sum(1 for ok, _, _ in self.rows if ok)
        print(f"\n  {p} passed, {self.failed} failed", flush=True)
