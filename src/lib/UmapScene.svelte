<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Inner scene component for the 3D UMAP viewer.
  Must be rendered inside a Threlte <Canvas>.
  Handles point clouds, animation, raycasting, label connections.

  Point clouds are added directly to the Three.js scene (not via <T>)
  because $state proxies don't work well with Three.js objects.
-->

<script lang="ts">
import { T, useTask, useThrelte } from "@threlte/core";
import { Grid, OrbitControls } from "@threlte/extras";
import { onDestroy, onMount } from "svelte";
import {
  AmbientLight,
  BufferGeometry,
  Color,
  Float32BufferAttribute,
  GridHelper,
  Group,
  Line,
  LineBasicMaterial,
  LineDashedMaterial,
  Mesh,
  MeshBasicMaterial,
  PerspectiveCamera,
  Points,
  PointsMaterial,
  Raycaster,
  SphereGeometry,
  Vector2,
  Vector3,
} from "three";

// ── Types ────────────────────────────────────────────────────────────────
import type { UmapPoint, UmapResult } from "$lib/types";
import { gauss } from "$lib/umap-helpers";

// ── Props ────────────────────────────────────────────────────────────────
let {
  data,
  tooltip = $bindable(null),
  activeLabel = $bindable(null),
}: {
  data: UmapResult;
  tooltip?: { x: number; y: number; text: string } | null;
  activeLabel?: string | null;
} = $props();

// ── Constants ────────────────────────────────────────────────────────────
const COLOR_A = 0x3b82f6,
  COLOR_B = 0xf59e0b,
  LINK_COLOR = 0xffffff;
const SCALE = 15;
const ANIM_MS = 1800;

// ── Threlte context ──────────────────────────────────────────────────────
const ctx = useThrelte();
const scene = ctx.scene;

// BRIGHT background for debugging — if you see red, rendering works
scene.background = new Color(0xff0000);

// Test sphere — if you see this, the scene/camera/renderer pipeline works
{
  const testGeo = new SphereGeometry(3, 16, 16);
  const testMat = new MeshBasicMaterial({ color: 0x00ff00 });
  const testMesh = new Mesh(testGeo, testMat);
  testMesh.position.set(0, 0, 0);
  scene.add(testMesh);
}

// ── Camera — add directly to scene and set as default ────────────────────
const cam = new PerspectiveCamera(55, 1, 0.1, 1000);
cam.position.set(0, 0, 25);
scene.add(cam);
ctx.camera.set(cam);

// Update aspect ratio from canvas size
const size = ctx.size;
$effect(() => {
  const s = size.current;
  if (s.width > 0 && s.height > 0) {
    cam.aspect = s.width / s.height;
    cam.updateProjectionMatrix();
  }
});

// ── Scene setup (imperative — added directly to Three.js scene) ──────────
const ambientLight = new AmbientLight(0xffffff, 0.5);
scene.add(ambientLight);

const grid = new GridHelper(SCALE * 1.2, 8, 0x222233, 0x111122);
grid.position.y = -SCALE * 0.5 - 0.5;
scene.add(grid);

// ── Point cloud state (plain JS, no $state proxies) ──────────────────────
let mainCloud: Points | null = null;
let labelCloud: Points | null = null;
let linkGroup: Group | null = null;
let curPoints: UmapPoint[] = [];
let labeledIdx: number[] = [];
let curPositions: Float32Array<ArrayBufferLike> = new Float32Array(0);

// Animation
let fromPos: Float32Array | null = null;
let toPos: Float32Array | null = null;
let animStart = 0;

// Raycasting
const raycaster = new Raycaster();
raycaster.params.Points = { threshold: 0.5 };
const mouse = new Vector2();
let downPos = { x: 0, y: 0 };

// ── Math helpers ─────────────────────────────────────────────────────────
function easeOut(t: number) {
  return 1 - (1 - t) ** 3;
}
// gauss() imported from umap-helpers.ts

function normalise(pts: UmapPoint[]): Float32Array {
  const n = pts.length,
    out = new Float32Array(n * 3);
  let mnX = Infinity,
    mxX = -Infinity,
    mnY = Infinity,
    mxY = -Infinity,
    mnZ = Infinity,
    mxZ = -Infinity;
  for (const p of pts) {
    const z = p.z ?? 0;
    if (p.x < mnX) mnX = p.x;
    if (p.x > mxX) mxX = p.x;
    if (p.y < mnY) mnY = p.y;
    if (p.y > mxY) mxY = p.y;
    if (z < mnZ) mnZ = z;
    if (z > mxZ) mxZ = z;
  }
  const rX = mxX - mnX || 1,
    rY = mxY - mnY || 1,
    rZ = mxZ - mnZ || 1;
  for (let i = 0; i < n; i++) {
    out[i * 3] = ((pts[i].x - mnX) / rX - 0.5) * SCALE;
    out[i * 3 + 1] = ((pts[i].y - mnY) / rY - 0.5) * SCALE;
    out[i * 3 + 2] = (((pts[i].z ?? 0) - mnZ) / rZ - 0.5) * SCALE;
  }
  return out;
}

function randomPositions(pts: UmapPoint[]): Float32Array {
  const n = pts.length,
    out = new Float32Array(n * 3);
  for (let i = 0; i < n; i++) {
    const cx = pts[i].session === 0 ? -3 : 3;
    out[i * 3] = cx + gauss() * 3;
    out[i * 3 + 1] = gauss() * 3;
    out[i * 3 + 2] = gauss() * 3;
  }
  return out;
}

// ── Label-connection lines ───────────────────────────────────────────────
function clearLinks() {
  if (!linkGroup) return;
  // biome-ignore lint/suspicious/noExplicitAny: three.js Object3D child type
  linkGroup.traverse((o: any) => {
    o.geometry?.dispose();
    o.material?.dispose();
  });
  scene.remove(linkGroup);
  linkGroup = null;
  activeLabel = null;
}

function showLinks(label: string) {
  clearLinks();
  activeLabel = label;
  const matching: number[] = [];
  for (let i = 0; i < curPoints.length; i++) if (curPoints[i].label === label) matching.push(i);
  if (matching.length < 2) return;
  matching.sort((a, b) => curPoints[a].utc - curPoints[b].utc);

  linkGroup = new Group();

  const lp: number[] = [];
  for (const idx of matching) lp.push(curPositions[idx * 3], curPositions[idx * 3 + 1], curPositions[idx * 3 + 2]);
  const lg = new BufferGeometry();
  lg.setAttribute("position", new Float32BufferAttribute(lp, 3));
  linkGroup.add(new Line(lg, new LineBasicMaterial({ color: LINK_COLOR, transparent: true, opacity: 0.45 })));

  const sg = new SphereGeometry(0.18, 8, 8);
  for (const idx of matching) {
    const c = curPoints[idx].session === 0 ? COLOR_A : COLOR_B;
    const m = new Mesh(sg, new MeshBasicMaterial({ color: c, transparent: true, opacity: 0.9 }));
    m.position.set(curPositions[idx * 3], curPositions[idx * 3 + 1], curPositions[idx * 3 + 2]);
    linkGroup.add(m);
  }

  let cx2 = 0,
    cy2 = 0,
    cz2 = 0;
  for (const idx of matching) {
    cx2 += curPositions[idx * 3];
    cy2 += curPositions[idx * 3 + 1];
    cz2 += curPositions[idx * 3 + 2];
  }
  cx2 /= matching.length;
  cy2 /= matching.length;
  cz2 /= matching.length;
  const dm = new LineDashedMaterial({
    color: LINK_COLOR,
    transparent: true,
    opacity: 0.2,
    dashSize: 0.3,
    gapSize: 0.2,
  });
  for (const idx of matching) {
    const dg = new BufferGeometry().setFromPoints([
      new Vector3(curPositions[idx * 3], curPositions[idx * 3 + 1], curPositions[idx * 3 + 2]),
      new Vector3(cx2, cy2, cz2),
    ]);
    const dl = new Line(dg, dm.clone());
    dl.computeLineDistances();
    linkGroup.add(dl);
  }
  scene.add(linkGroup);
}

// ── Build point clouds (added directly to scene) ─────────────────────────
function buildCloud(pts: UmapPoint[], positions: Float32Array) {
  const n = pts.length;
  const colA = new Color(COLOR_A),
    colB = new Color(COLOR_B);
  const colors = new Float32Array(n * 3);
  const lPos: number[] = [],
    lCol: number[] = [],
    lIdx: number[] = [];

  for (let i = 0; i < n; i++) {
    const c = pts[i].session === 0 ? colA : colB;
    colors[i * 3] = c.r;
    colors[i * 3 + 1] = c.g;
    colors[i * 3 + 2] = c.b;
    if (pts[i].label) {
      lPos.push(positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]);
      lCol.push(c.r, c.g, c.b);
      lIdx.push(i);
    }
  }

  // Remove old clouds from scene
  if (mainCloud) {
    scene.remove(mainCloud);
    mainCloud.geometry.dispose();
    (mainCloud.material as PointsMaterial).dispose();
  }
  if (labelCloud) {
    scene.remove(labelCloud);
    labelCloud.geometry.dispose();
    (labelCloud.material as PointsMaterial).dispose();
    labelCloud = null;
  }
  clearLinks();

  const ps = Math.max(1.5, Math.min(4, 200 / Math.sqrt(n)));
  // biome-ignore lint/style/noNonNullAssertion: raycaster.params.Points exists when using PointsMaterial
  raycaster.params.Points!.threshold = ps * 0.3;

  // Main cloud
  const g = new BufferGeometry();
  g.setAttribute("position", new Float32BufferAttribute(positions.slice(), 3));
  g.setAttribute("color", new Float32BufferAttribute(colors, 3));
  mainCloud = new Points(
    g,
    new PointsMaterial({
      size: ps,
      vertexColors: true,
      transparent: true,
      opacity: 0.65,
      sizeAttenuation: true,
      depthWrite: false,
    }),
  );
  scene.add(mainCloud);

  // Labeled cloud (larger dots)
  if (lPos.length) {
    const lg2 = new BufferGeometry();
    lg2.setAttribute("position", new Float32BufferAttribute(lPos, 3));
    lg2.setAttribute("color", new Float32BufferAttribute(lCol, 3));
    labelCloud = new Points(
      lg2,
      new PointsMaterial({
        size: ps * 2.5,
        vertexColors: true,
        transparent: true,
        opacity: 0.95,
        sizeAttenuation: true,
      }),
    );
    scene.add(labelCloud);
  }

  curPoints = pts;
  labeledIdx = lIdx;
  curPositions = positions.slice();
}

function applyPositions(pos: Float32Array) {
  if (!mainCloud) return;
  // biome-ignore lint/suspicious/noExplicitAny: three.js BufferAttribute typed as any for direct array access
  const a = mainCloud.geometry.getAttribute("position") as any;
  a.array.set(pos);
  a.needsUpdate = true;

  if (labelCloud && labeledIdx.length) {
    // biome-ignore lint/suspicious/noExplicitAny: three.js BufferAttribute typed as any for direct array access
    const la = labelCloud.geometry.getAttribute("position") as any;
    for (let li = 0; li < labeledIdx.length; li++) {
      const i = labeledIdx[li];
      la.array[li * 3] = pos[i * 3];
      la.array[li * 3 + 1] = pos[i * 3 + 1];
      la.array[li * 3 + 2] = pos[i * 3 + 2];
    }
    la.needsUpdate = true;
  }
  curPositions = pos;
}

// ── React to data changes ────────────────────────────────────────────────
let prevData: UmapResult | null = null;

$effect(() => {
  const d = data; // track reactivity
  if (!d?.points?.length) return;
  if (d === prevData) return;
  prevData = d;

  const target = normalise(d.points);
  const start = randomPositions(d.points);
  buildCloud(d.points, start);

  // Start animation → target
  fromPos = start;
  toPos = target;
  animStart = performance.now();
});

// ── Animation tick (runs on mainStage, before Threlte's auto-render) ────
useTask(() => {
  // OrbitControls
  controls?.update();

  // Position animation
  if (fromPos && toPos && mainCloud) {
    const t = Math.min(1, (performance.now() - animStart) / ANIM_MS);
    const e = easeOut(t);
    const n = toPos.length;
    const buf = new Float32Array(n);
    for (let i = 0; i < n; i++) buf[i] = fromPos[i] + (toPos[i] - fromPos[i]) * e;
    applyPositions(buf);
    if (t >= 1) {
      fromPos = null;
      toPos = null;
    }
  }

  // Tell Threlte a frame is needed (for on-demand mode; always mode ignores this)
  ctx.invalidate();
});

// ── Raycasting helpers ───────────────────────────────────────────────────
function raycast(e: PointerEvent): number {
  const rect = ctx.dom.getBoundingClientRect();
  mouse.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
  mouse.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;
  raycaster.setFromCamera(mouse, cam);

  let hitIdx = -1;
  if (labelCloud) {
    const lh = raycaster.intersectObject(labelCloud);
    // biome-ignore lint/style/noNonNullAssertion: raycaster intersection always has an index
    if (lh.length) hitIdx = labeledIdx[lh[0].index!];
  }
  if (hitIdx < 0 && mainCloud) {
    const h = raycaster.intersectObject(mainCloud);
    // biome-ignore lint/style/noNonNullAssertion: raycaster intersection always has an index
    if (h.length) hitIdx = h[0].index!;
  }
  return hitIdx;
}

function onPointerMove(e: PointerEvent) {
  if (!curPoints.length) {
    tooltip = null;
    return;
  }
  const hitIdx = raycast(e);
  if (hitIdx >= 0 && hitIdx < curPoints.length) {
    const p = curPoints[hitIdx];
    const dt = new Date(p.utc * 1000);
    const s = p.session === 0 ? "A" : "B";
    const lb = p.label ? `\n🏷 ${p.label}` : "";
    const ch = p.label ? "\nclick to show connections" : "";
    const rect = ctx.dom.getBoundingClientRect();
    tooltip = {
      x: e.clientX - rect.left,
      y: e.clientY - rect.top,
      text: `Session ${s} · ${dt.toLocaleString()}${lb}${ch}`,
    };
  } else {
    tooltip = null;
  }
}

function onPointerDown(e: PointerEvent) {
  downPos = { x: e.clientX, y: e.clientY };
}

function onPointerUp(e: PointerEvent) {
  if ((e.clientX - downPos.x) ** 2 + (e.clientY - downPos.y) ** 2 > 25) return;
  const hitIdx = raycast(e);
  let label: string | undefined;
  if (hitIdx >= 0 && hitIdx < curPoints.length) label = curPoints[hitIdx]?.label;
  if (label) {
    activeLabel === label ? clearLinks() : showLinks(label);
  } else {
    clearLinks();
  }
}

function onPointerLeave() {
  tooltip = null;
}

// OrbitControls — loaded and attached in onMount
// biome-ignore lint/suspicious/noExplicitAny: OrbitControls dynamically imported
let controls: any = null;

// ── DOM event listeners ──────────────────────────────────────────────────
onMount(async () => {
  ctx.dom.addEventListener("pointermove", onPointerMove);
  ctx.dom.addEventListener("pointerdown", onPointerDown);
  ctx.dom.addEventListener("pointerup", onPointerUp);
  ctx.dom.addEventListener("pointerleave", onPointerLeave);

  // Load and attach OrbitControls
  const { OrbitControls } = await import("three/examples/jsm/controls/OrbitControls.js");
  controls = new OrbitControls(cam, ctx.dom);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.autoRotate = true;
  controls.autoRotateSpeed = 0.8;
  controls.minDistance = 5;
  controls.maxDistance = 80;
  ctx.renderer.render(scene, cam);
});

onDestroy(() => {
  ctx.dom.removeEventListener("pointermove", onPointerMove);
  ctx.dom.removeEventListener("pointerdown", onPointerDown);
  ctx.dom.removeEventListener("pointerup", onPointerUp);
  ctx.dom.removeEventListener("pointerleave", onPointerLeave);
  controls?.dispose();
  if (mainCloud) {
    scene.remove(mainCloud);
    mainCloud.geometry.dispose();
    (mainCloud.material as PointsMaterial).dispose();
  }
  if (labelCloud) {
    scene.remove(labelCloud);
    labelCloud.geometry.dispose();
    (labelCloud.material as PointsMaterial).dispose();
  }
  scene.remove(ambientLight);
  scene.remove(grid);
  scene.remove(cam);
  clearLinks();
});
</script>

<!-- Scene is fully imperative — no declarative Threlte template needed -->
