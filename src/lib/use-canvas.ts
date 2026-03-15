// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Shared canvas lifecycle helper — DRY replacement for the ResizeObserver +
// requestAnimationFrame + DPR boilerplate duplicated across EegChart,
// BandChart, PpgChart, GpuChart, ImuChart, etc.
//
// Usage (Svelte action):
//
//   <canvas use:animatedCanvas={{ draw, heightPx: 160 }} />
//
// The action:
//   1. Scales the canvas for the current device-pixel-ratio.
//   2. Observes `container ?? canvas.parentElement` for size changes.
//   3. Runs a `requestAnimationFrame` loop calling `draw(ctx, cssW, cssH)`.
//   4. Cleans up on destroy (cancels RAF, disconnects observer).

import { getDpr } from "$lib/format";

export interface AnimatedCanvasOpts {
  /** Called every animation frame with the 2-D context and logical size. */
  draw: (ctx: CanvasRenderingContext2D, width: number, height: number) => void;

  /** Fixed CSS height of the canvas (logical px). */
  heightPx: number;

  /**
   * Fixed CSS width (logical px).  When set, the canvas is sized to this
   * constant instead of tracking the container's width.  Used by ImuChart
   * and PpgChart which render at a fixed resolution.
   */
  widthPx?: number;

  /**
   * Element to observe for width changes.  Defaults to `canvas.parentElement`.
   * Ignored when `widthPx` is set (no need to track container width).
   */
  container?: HTMLElement;

  /**
   * If true the RAF loop is NOT started automatically — call the returned
   * `start()` function manually (useful when the chart is initially hidden).
   */
  manual?: boolean;
}

/**
 * Svelte action that manages the RAF + ResizeObserver lifecycle for a `<canvas>`.
 */
export function animatedCanvas(
  canvas: HTMLCanvasElement,
  opts: AnimatedCanvasOpts,
): { destroy: () => void; update: (o: AnimatedCanvasOpts) => void } {
  let { draw, heightPx, widthPx, container, manual } = opts;
  let raf: number | undefined;
  let ro: ResizeObserver | undefined;
  let running = false;

  function resize() {
    const dpr = getDpr();
    if (widthPx != null) {
      canvas.width  = Math.round(widthPx * dpr);
      canvas.height = Math.round(heightPx * dpr);
    } else {
      const observed = container ?? canvas.parentElement ?? canvas;
      const w = observed.clientWidth || canvas.clientWidth;
      canvas.width  = Math.round(w * dpr);
      canvas.height = Math.round(heightPx * dpr);
      canvas.style.width  = `${w}px`;
      canvas.style.height = `${heightPx}px`;
    }
  }

  function tick() {
    raf = requestAnimationFrame(tick);
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = getDpr();
    const w = canvas.width / dpr;
    const h = canvas.height / dpr;
    try {
      draw(ctx, w, h);
    } catch (err) {
      console.error("[animatedCanvas] draw error:", err);
    }
  }

  function start() {
    if (running) return;
    running = true;
    raf = requestAnimationFrame(tick);
  }

  function stop() {
    running = false;
    if (raf !== undefined) {
      cancelAnimationFrame(raf);
      raf = undefined;
    }
  }

  // Initial setup
  if (widthPx == null) {
    const observed = container ?? canvas.parentElement ?? canvas;
    ro = new ResizeObserver(resize);
    ro.observe(observed);
  }
  resize();

  if (!manual) start();

  return {
    update(o: AnimatedCanvasOpts) {
      draw = o.draw;
      heightPx = o.heightPx;
      widthPx  = o.widthPx;
      if (o.container !== container) {
        container = o.container;
        ro?.disconnect();
        if (widthPx == null) {
          const newObs = container ?? canvas.parentElement ?? canvas;
          ro = new ResizeObserver(resize);
          ro.observe(newObs);
        }
      }
      resize();
    },
    destroy() {
      stop();
      ro?.disconnect();
    },
  };
}
