<!-- SPDX-License-Identifier: GPL-3.0-only -->
<script lang="ts">
import { animatedCanvas } from "$lib/use-canvas";

const N = 600;
const hbo = new Float64Array(N);
const hbr = new Float64Array(N);
const hbt = new Float64Array(N);
const wl = new Float64Array(N);
const oxy = new Float64Array(N);
let head = 0;
let filled = 0;
let needsRedraw = false;

let summary = $state({ wl30: 0, oxy30: 0, fatigue: 0 });

export function pushMetrics(m: { hbo: number; hbr: number; hbt: number; workload: number; oxygenation: number }) {
  const i = head % N;
  hbo[i] = m.hbo;
  hbr[i] = m.hbr;
  hbt[i] = m.hbt;
  wl[i] = m.workload;
  oxy[i] = m.oxygenation;
  head++;
  if (filled < N) filled++;

  // ~4 Hz status updates: 120 samples ≈ 30 s; 480 ≈ 2 min.
  const avg = (buf: Float64Array, win: number) => {
    const n = Math.min(win, filled);
    if (n <= 0) return 0;
    let s = 0;
    for (let k = 0; k < n; k++) {
      const idx = (head - 1 - k + N * 4) % N;
      s += buf[idx];
    }
    return s / n;
  };
  const wl30 = avg(wl, 120);
  const wl120 = avg(wl, 480);
  const oxy30 = avg(oxy, 120);
  // Fatigue proxy: sustained workload elevation with lower oxygenation.
  const fatigue = Math.max(0, Math.min(100, (wl120 - wl30) * 1.2 + (60 - oxy30) * 0.6));
  summary = { wl30, oxy30, fatigue };

  needsRedraw = true;
}

function minmax(buf: Float64Array, n: number): [number, number] {
  let mn = Infinity;
  let mx = -Infinity;
  for (let i = 0; i < n; i++) {
    const idx = (head - n + i + N * 4) % N;
    const v = buf[idx];
    if (v < mn) mn = v;
    if (v > mx) mx = v;
  }
  if (!Number.isFinite(mn) || !Number.isFinite(mx) || mn === mx) return [0, 1];
  return [mn, mx];
}

function drawLine(
  ctx: CanvasRenderingContext2D,
  buf: Float64Array,
  n: number,
  w: number,
  y0: number,
  h: number,
  mn: number,
  mx: number,
  color: string,
) {
  const r = mx - mn || 1;
  ctx.beginPath();
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.2;
  for (let i = 0; i < n; i++) {
    const idx = (head - n + i + N * 4) % N;
    const x = (i / (N - 1)) * w;
    const y = y0 + (1 - (buf[idx] - mn) / r) * h;
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  }
  ctx.stroke();
}

function draw(ctx: CanvasRenderingContext2D, w: number, h: number) {
  if (!needsRedraw) return;
  needsRedraw = false;
  const dpr = ctx.canvas.width / w;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);

  if (filled < 2) return;

  const topH = h * 0.62;
  const botH = h * 0.3;
  const n = filled;

  const [mnHbo, mxHbo] = minmax(hbo, n);
  const [mnHbr, mxHbr] = minmax(hbr, n);
  const [mnHbt, mxHbt] = minmax(hbt, n);
  const mnTop = Math.min(mnHbo, mnHbr, mnHbt);
  const mxTop = Math.max(mxHbo, mxHbr, mxHbt);

  drawLine(ctx, hbo, n, w, 4, topH - 8, mnTop, mxTop, "#22c55e");
  drawLine(ctx, hbr, n, w, 4, topH - 8, mnTop, mxTop, "#ef4444");
  drawLine(ctx, hbt, n, w, 4, topH - 8, mnTop, mxTop, "#60a5fa");

  // Bottom: workload + oxygenation (0..100)
  drawLine(ctx, wl, n, w, topH + 6, botH - 8, 0, 100, "#f59e0b");
  drawLine(ctx, oxy, n, w, topH + 6, botH - 8, 0, 100, "#a78bfa");

  ctx.fillStyle = "rgba(148,163,184,0.8)";
  ctx.font = "10px system-ui";
  ctx.fillText("HbO/HbR/HbT", 4, 12);
  ctx.fillText("Workload/Oxy", 4, topH + 16);
}
</script>

<div class="rounded-xl border border-border dark:border-white/[0.04] bg-muted dark:bg-[#1a1a28] px-3 py-2.5 flex flex-col gap-1.5">
  <div class="flex items-center justify-between gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">fNIRS Trends</span>
    <span class="text-[0.48rem] text-muted-foreground/60">WL30 {summary.wl30.toFixed(1)} · Oxy30 {summary.oxy30.toFixed(1)} · Fatigue {summary.fatigue.toFixed(1)}</span>
  </div>
  <canvas use:animatedCanvas={{ draw, heightPx: 130 }} class="w-full h-[130px]" aria-label="fNIRS trends chart"></canvas>
</div>
