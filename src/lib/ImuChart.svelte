<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts" module>
  /** IMU packet from the Rust backend. */
  export interface ImuPacket {
    sensor:    "accel" | "gyro";
    samples:   [number, number, number][];  // 3 XYZ samples per packet
    timestamp: number;
  }
</script>

<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import { getDpr } from "$lib/format";

  const CANVAS_W  = 400;
  const CANVAS_H  = 160;       // two stacked sub-charts (accel + gyro)
  const VISIBLE   = 512;       // ~10 s at ~52 Hz (3 samples/packet × ~17 packets/s)
  const AXES      = 3;         // X, Y, Z
  const AXIS_LABELS = ["X", "Y", "Z"] as const;
  const AXIS_COLORS = ["#f87171", "#4ade80", "#60a5fa"]; // red, green, blue

  // Ring buffers: [sensor][axis][sample]
  // sensor 0 = accel, 1 = gyro
  const bufs: Float32Array[][] = Array.from({ length: 2 }, () =>
    Array.from({ length: AXES }, () => new Float32Array(VISIBLE))
  );
  const heads  = [0, 0];
  const filled = [0, 0];

  let canvas: HTMLCanvasElement | undefined;
  let ctx: CanvasRenderingContext2D | null = null;
  let raf = 0;
  let needsRedraw = false;

  // Latest values for the numeric readout
  let latestAccel = $state<[number, number, number]>([0, 0, 0]);
  let latestGyro  = $state<[number, number, number]>([0, 0, 0]);

  /** Push an IMU packet. Called externally by the parent page. */
  export function pushPacket(pkt: ImuPacket): void {
    const si = pkt.sensor === "accel" ? 0 : 1;
    for (const xyz of pkt.samples) {
      for (let a = 0; a < AXES; a++) {
        bufs[si][a][heads[si] % VISIBLE] = xyz[a];
      }
      heads[si]++;
      if (filled[si] < VISIBLE) filled[si]++;
    }
    if (pkt.samples.length > 0) {
      const last = pkt.samples[pkt.samples.length - 1];
      if (si === 0) latestAccel = [last[0], last[1], last[2]];
      else          latestGyro  = [last[0], last[1], last[2]];
    }
    needsRedraw = true;
  }

  function draw() {
    raf = requestAnimationFrame(draw);
    if (!needsRedraw || !ctx) return;
    needsRedraw = false;

    const dpr = getDpr();
    const w = CANVAS_W * dpr;
    const h = CANVAS_H * dpr;
    const halfH = h / 2;

    ctx.clearRect(0, 0, w, h);

    const sensorLabels = [t("dashboard.accel"), t("dashboard.gyro")];
    const sensorUnits  = ["g", "°/s"];

    for (let si = 0; si < 2; si++) {
      const yOff = si * halfH;
      const n    = filled[si];
      const head = heads[si];

      // Separator line between accel and gyro
      if (si === 1) {
        ctx.strokeStyle = "rgba(128,128,128,0.2)";
        ctx.lineWidth   = 1 * dpr;
        ctx.beginPath();
        ctx.moveTo(0, yOff);
        ctx.lineTo(w, yOff);
        ctx.stroke();
      }

      // Sensor label
      ctx.fillStyle    = "rgba(160,160,180,0.6)";
      ctx.font         = `${9 * dpr}px system-ui, sans-serif`;
      ctx.textAlign    = "left";
      ctx.textBaseline = "top";
      ctx.fillText(`${sensorLabels[si]} (${sensorUnits[si]})`, 4 * dpr, yOff + 4 * dpr);

      if (n < 2) continue;

      // Find global min/max across all 3 axes for this sensor
      let mn = Infinity, mx = -Infinity;
      for (let a = 0; a < AXES; a++) {
        const buf = bufs[si][a];
        for (let i = 0; i < n; i++) {
          const idx = (head - n + i + VISIBLE * 2) % VISIBLE;
          const v = buf[idx];
          if (v < mn) mn = v;
          if (v > mx) mx = v;
        }
      }
      // Add 10% padding and ensure minimum range
      const range = (mx - mn) || 1;
      const pad   = range * 0.1;
      mn -= pad;
      mx += pad;
      const finalRange = mx - mn;

      // Draw zero line
      const zeroY = yOff + (1 - (0 - mn) / finalRange) * halfH;
      if (zeroY > yOff && zeroY < yOff + halfH) {
        ctx.strokeStyle = "rgba(128,128,128,0.15)";
        ctx.lineWidth   = 1 * dpr;
        ctx.setLineDash([3 * dpr, 3 * dpr]);
        ctx.beginPath();
        ctx.moveTo(0, zeroY);
        ctx.lineTo(w, zeroY);
        ctx.stroke();
        ctx.setLineDash([]);
      }

      // Draw each axis
      for (let a = 0; a < AXES; a++) {
        const buf = bufs[si][a];
        ctx.beginPath();
        ctx.strokeStyle = AXIS_COLORS[a];
        ctx.lineWidth   = 1.3 * dpr;
        ctx.globalAlpha = 0.85;
        for (let i = 0; i < n; i++) {
          const idx = (head - n + i + VISIBLE * 2) % VISIBLE;
          const x = (i / (VISIBLE - 1)) * w;
          const y = yOff + (1 - (buf[idx] - mn) / finalRange) * halfH;
          if (i === 0) ctx.moveTo(x, y);
          else ctx.lineTo(x, y);
        }
        ctx.stroke();
        ctx.globalAlpha = 1;
      }
    }
  }

  $effect(() => {
    if (canvas) {
      const dpr = getDpr();
      canvas.width  = CANVAS_W * dpr;
      canvas.height = CANVAS_H * dpr;
      ctx = canvas.getContext("2d");
      raf = requestAnimationFrame(draw);
    }
    return () => { if (raf) cancelAnimationFrame(raf); };
  });
</script>

<div class="flex flex-col gap-1.5">
  <!-- Header with live XYZ values -->
  <div class="flex items-center gap-4 flex-wrap">
    <div class="flex items-center gap-1.5">
      <span class="text-[0.5rem] font-semibold text-muted-foreground/70">{t("dashboard.accel")}</span>
      {#each AXIS_LABELS as axis, i}
        <span class="font-mono text-[0.55rem] tabular-nums" style="color:{AXIS_COLORS[i]}">
          {axis}{latestAccel[i] >= 0 ? "+" : ""}{latestAccel[i].toFixed(3)}
        </span>
      {/each}
    </div>
    <div class="flex items-center gap-1.5">
      <span class="text-[0.5rem] font-semibold text-muted-foreground/70">{t("dashboard.gyro")}</span>
      {#each AXIS_LABELS as axis, i}
        <span class="font-mono text-[0.55rem] tabular-nums" style="color:{AXIS_COLORS[i]}">
          {axis}{latestGyro[i] >= 0 ? "+" : ""}{latestGyro[i].toFixed(1)}
        </span>
      {/each}
    </div>
  </div>

  <!-- Canvas -->
  <canvas
    bind:this={canvas}
    class="w-full rounded-lg bg-black/[0.03] dark:bg-white/[0.03]"
    style="height:{CANVAS_H}px; image-rendering:pixelated"
  ></canvas>
</div>
