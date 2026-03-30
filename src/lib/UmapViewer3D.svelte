<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  3D UMAP scatter viewer — raw Three.js (no Threlte).
  Creates its own canvas and renderer, bypassing any framework issues.

  - Shows random placeholder cloud immediately while UMAP computes
  - Animates smoothly to real positions when data arrives
  - Auto-orbit, hover tooltips, click-to-connect labelled points
-->

<script lang="ts">
import { onDestroy, onMount } from "svelte";
import type * as THREE_NS from "three";
import type { Color as ThreeColor, Vector3 as ThreeVector3 } from "three";
import type { OrbitControls as OrbitControlsType } from "three/examples/jsm/controls/OrbitControls.js";
import { Spinner } from "$lib/components/ui/spinner";
import {
  UMAP_ANIM_MS,
  UMAP_BG,
  UMAP_BG_LIGHT,
  UMAP_COLOR_A,
  UMAP_COLOR_B,
  UMAP_DATE_LIT,
  UMAP_DATE_SAT,
  UMAP_LINK_PALETTE,
  UMAP_POINT_SIZE,
  UMAP_SCALE,
  UMAP_SCALE_MAX,
  UMAP_SCALE_MIN,
  UMAP_TRACE_COLOR,
  UMAP_TRACE_GROW_MS,
  UMAP_TRACE_INTERVAL_MS,
  UMAP_TRACE_NODE_COLOR,
} from "$lib/constants";
import { dateToLocalKey, fmtTimeShort, fromUnix } from "$lib/format";
import { t } from "$lib/i18n/index.svelte";
import { getResolved } from "$lib/stores/theme.svelte";
import type { UmapPoint, UmapProgress, UmapResult } from "$lib/types";
import {
  easeOut,
  fmtGradientTs,
  fmtUtcTime,
  gauss,
  hslToRgb,
  labelHex,
  turboRaw,
  utcToLocalDate,
} from "$lib/umap-helpers";

type ThreeModule = typeof import("three");
type LabelCloudGroup = THREE_NS.Group & { __updatePositions?: (pos: Float32Array) => void };
type LabelMesh = THREE_NS.Mesh<THREE_NS.BufferGeometry, THREE_NS.MeshBasicMaterial> & { _baseScale?: number };
type TraceLine = THREE_NS.Line<THREE_NS.BufferGeometry, THREE_NS.LineBasicMaterial>;
type TraceSphere = THREE_NS.Mesh<THREE_NS.SphereGeometry, THREE_NS.MeshBasicMaterial>;

let {
  data,
  computing = false,
  colorByDate = false,
  progress = null,
  autoConnectLabels = false,
}: {
  data: UmapResult;
  computing?: boolean;
  /** When true, color points by local date instead of session A/B. */
  colorByDate?: boolean;
  /** Live epoch-level progress from the UMAP trainer. */
  progress?: UmapProgress | null;
  /**
   * When true, automatically draw connection lines between every group of
   * points that share the same label and has ≥ 2 members.  Connections are
   * activated after the intro animation finishes so they land at the correct
   * final positions, not the random start positions.
   */
  autoConnectLabels?: boolean;
} = $props();

// ── Constants (from constants.ts) ────────────────────────────────────────
const COLOR_A = UMAP_COLOR_A,
  COLOR_B = UMAP_COLOR_B;
const SCALE = UMAP_SCALE,
  ANIM_MS = UMAP_ANIM_MS;
const LINK_PALETTE = UMAP_LINK_PALETTE;

// ── Theme-aware colors ─────────────────────────────────────────────────
let isDark = $derived(getResolved() === "dark");

// ── Reactive UI state ────────────────────────────────────────────────────
let container: HTMLDivElement | undefined = $state();
let tooltip = $state<{ x: number; y: number; text: string } | null>(null);
let activeLabels = $state<string[]>([]);
let error = $state<string | null>(null);
let loaded = $state(false);
let pointScale = $state(0.5);

// ── Time-gradient coloring ──────────────────────────────────────────────
/** Which session (if any) to color with a jet time gradient. */
let timeGradient = $state<"A" | "B" | null>(null);
/** Min/max timestamps for the gradient legend. */
let gradientRange = $state<{ minUtc: number; maxUtc: number } | null>(null);

/** Jet colormap — turbo with light-theme darkening. */
function jet(t: number): [number, number, number] {
  const [r, g, b] = turboRaw(t);
  if (isDark) return [r, g, b];
  // Light theme: slightly darken for contrast against pale background.
  const dim = 0.75;
  return [r * dim, g * dim, b * dim];
}

function jetHex(t: number): string {
  const [r, g, b] = jet(t);
  const hex = (v: number) =>
    Math.round(v * 255)
      .toString(16)
      .padStart(2, "0");
  return `#${hex(r)}${hex(g)}${hex(b)}`;
}

/** CSS linear-gradient matching the current theme's turbo colormap. */
let jetGradientCSS = $derived(
  `linear-gradient(to right, ${[0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1].map((v) => jetHex(v)).join(", ")})`,
);

// ── Chronological trace animation ─────────────────────────────────────
let traceActive = $state(false);
let traceProgress = $state(0); // how many segments drawn so far
let traceTotal = $state(0); // total segments
let traceGroup: THREE_NS.Group | null = null; // THREE.Group holding trace objects
let traceSorted: number[] = []; // indices into curPoints sorted by utc
let traceTimer: ReturnType<typeof setInterval> | null = null;
/** Time range of the trace for the gradient legend. */
let traceTimeRange = $state<{ minUtc: number; maxUtc: number } | null>(null);
/** Tick positions for the trace gradient legend (0-1 normalised + label). */
let traceTimeTicks = $state<{ pct: number; label: string }[]>([]);

function buildTraceTimeTicks(sorted: number[]) {
  if (sorted.length < 2) {
    traceTimeRange = null;
    traceTimeTicks = [];
    return;
  }
  const minU = curPoints[sorted[0]].utc;
  const maxU = curPoints[sorted[sorted.length - 1]].utc;
  traceTimeRange = { minUtc: minU, maxUtc: maxU };
  const span = maxU - minU;
  if (span <= 0) {
    traceTimeTicks = [];
    return;
  }

  // Pick ~5 evenly-spaced ticks
  const N_TICKS = 5;
  const ticks: { pct: number; label: string }[] = [];
  for (let i = 0; i < N_TICKS; i++) {
    const frac = i / (N_TICKS - 1);
    const utc = minU + span * frac;
    const d = new Date(utc * 1000);
    let label: string;
    if (span > 86400 * 2) {
      label = d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
    } else if (span > 3600) {
      label = d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
    } else {
      label = d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit" });
    }
    ticks.push({ pct: frac * 100, label });
  }
  traceTimeTicks = ticks;
}
// Per-segment grow animation
let traceGrowing: {
  line: TraceLine;
  sphere: TraceSphere;
  edgeLine?: TraceLine;
  start: number;
  from: number[];
  to: number[];
}[] = [];

// ── Label sidebar ─────────────────────────────────────────────────────────
let sidebarOpen = $state(false);
let selectedLabel = $state<string | null>(null);
let proximityDist = $state(3.0);
let proximateLabels = $state<string[]>([]);
let animating = $state(false);
let cmdHeld = $state(false); // true while ⌘/Ctrl is held → pan mode

interface LabelEntry {
  label: string;
  inA: boolean;
  inB: boolean;
  hue: number;
  timestamps: { utc: number; session: number }[];
}

const uniqueLabels = $derived.by((): LabelEntry[] => {
  const map = new Map<string, { inA: boolean; inB: boolean; timestamps: { utc: number; session: number }[] }>();
  for (const p of curPoints) {
    if (!p.label) continue;
    if (!map.has(p.label)) map.set(p.label, { inA: false, inB: false, timestamps: [] });
    // biome-ignore lint/style/noNonNullAssertion: value is guaranteed by surrounding logic
    const e = map.get(p.label)!;
    e.timestamps.push({ utc: p.utc, session: p.session });
    if (p.session === 0) e.inA = true;
    else e.inB = true;
  }
  const all = [...map.entries()].map(([label, d]) => {
    d.timestamps.sort((a, b) => a.utc - b.utc);
    return { label, ...d, hue: 0 };
  });
  // Sort: common (A+B) first, then A-only, then B-only; within each group by earliest timestamp.
  all.sort((a, b) => {
    const ga = a.inA && a.inB ? 0 : a.inA ? 1 : 2;
    const gb = b.inA && b.inB ? 0 : b.inA ? 1 : 2;
    if (ga !== gb) return ga - gb;
    return (a.timestamps[0]?.utc ?? 0) - (b.timestamps[0]?.utc ?? 0);
  });
  return all.map((e, i) => ({ ...e, hue: i / Math.max(all.length, 1) }));
});

// ── Imperative refs ──────────────────────────────────────────────────────
let THREE!: ThreeModule;
let scene!: THREE_NS.Scene;
let camera!: THREE_NS.PerspectiveCamera;
let renderer!: THREE_NS.WebGLRenderer;
let controls!: OrbitControlsType;
let animId = 0;
let resizeObs: ResizeObserver | null = null;

let mainCloud: THREE_NS.Points<THREE_NS.BufferGeometry, THREE_NS.PointsMaterial> | null = null;
let labelCloud: LabelCloudGroup | null = null;
// Multi-label: map from label name → { group, colorIdx }
let linkGroups = new Map<string, { group: THREE_NS.Group; colorIdx: number }>();
let curPoints = $state<UmapPoint[]>([]);
let labeledIdx: number[] = [];
let curPositions: Float32Array<ArrayBufferLike> = new Float32Array(0);
let baseColors: Float32Array | null = null;
/** Saved dot/ring base colors for each labeled point (index matches labeledIdx). */
let labelCloudBase: { dc: number; rr: number; rg: number; rb: number }[] = [];
/** Group of cylinder meshes drawn between selected and proximate labeled nodes. */
let highlightEdgesGroup: THREE_NS.Group | null = null;
let _onCmdDown: ((e: KeyboardEvent) => void) | null = null;
let _onCmdUp: ((e: KeyboardEvent) => void) | null = null;

let fromPos: Float32Array | null = null;
let toPos: Float32Array | null = null;
let animStart = 0;

let raycaster!: THREE_NS.Raycaster;
let mouse!: THREE_NS.Vector2;
let downPos = { x: 0, y: 0 };

// Track data changes
let prevDataRef: UmapResult | null = null;

// ── Math helpers ─────────────────────────────────────────────────────────
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

function disposeSceneObject(obj: THREE_NS.Object3D) {
  const geometry = (obj as { geometry?: { dispose?: () => void } }).geometry;
  geometry?.dispose?.();
  const material = (obj as { material?: THREE_NS.Material | THREE_NS.Material[] }).material;
  if (Array.isArray(material)) {
    for (const mat of material) mat.dispose();
  } else {
    material?.dispose?.();
  }
}

// ── Label-connection lines (multi-label) ──────────────────────────────
let nextColorIdx = 0;

function removeLinkGroup(label: string) {
  const entry = linkGroups.get(label);
  if (!entry || !scene) return;
  entry.group.traverse(disposeSceneObject);
  scene.remove(entry.group);
  linkGroups.delete(label);
  activeLabels = activeLabels.filter((l) => l !== label);
}

function clearAllLinks() {
  if (!scene) return;
  for (const [label] of linkGroups) removeLinkGroup(label);
  linkGroups.clear();
  activeLabels = [];
  nextColorIdx = 0;
}

function toggleLinks(label: string) {
  if (linkGroups.has(label)) {
    removeLinkGroup(label);
  } else {
    addLinks(label);
  }
}

// ── Auto-connect support ─────────────────────────────────────────────────
/** Set true by onDataChange when autoConnectLabels is on; cleared by the
 *  render loop once the intro animation finishes and positions are final. */
let pendingAutoConnect = false;

/** Activate connections for every label that appears on ≥ 2 points. */
function runAutoConnect() {
  if (!curPoints.length) return;
  clearAllLinks();
  const counts = new Map<string, number>();
  for (const p of curPoints) {
    if (p.label) counts.set(p.label, (counts.get(p.label) ?? 0) + 1);
  }
  for (const [label, n] of counts) {
    if (n >= 2) addLinks(label);
  }
}

// ── Proximity highlight ───────────────────────────────────────────────────

function clearHighlightEdges() {
  if (!highlightEdgesGroup || !scene) return;
  scene.remove(highlightEdgesGroup);
  highlightEdgesGroup.traverse(disposeSceneObject);
  highlightEdgesGroup = null;
}

function applyHighlight() {
  if (!mainCloud) return;
  const colorAttr = mainCloud.geometry.getAttribute("color");
  if (!colorAttr) return;
  const n = curPoints.length;

  // ── 1. Compute selection / proximity sets ─────────────────────────────
  const selIdx = new Set<number>();
  const proxIdx = new Set<number>();
  const proxSet = new Set<string>();

  if (selectedLabel && baseColors) {
    for (let i = 0; i < n; i++) if (curPoints[i].label === selectedLabel) selIdx.add(i);
    const d2 = proximityDist * proximityDist;
    for (let i = 0; i < n; i++) {
      const lbl = curPoints[i].label;
      if (!lbl || lbl === selectedLabel) continue;
      for (const j of selIdx) {
        const dx = curPositions[i * 3] - curPositions[j * 3];
        const dy = curPositions[i * 3 + 1] - curPositions[j * 3 + 1];
        const dz = curPositions[i * 3 + 2] - curPositions[j * 3 + 2];
        if (dx * dx + dy * dy + dz * dz <= d2) {
          proxIdx.add(i);
          proxSet.add(lbl);
          break;
        }
      }
    }
  }
  proximateLabels = [...proxSet];

  // ── 2. Recolor mainCloud point buffer ─────────────────────────────────
  if (!selectedLabel || !baseColors) {
    if (baseColors) {
      colorAttr.array.set(baseColors);
      colorAttr.needsUpdate = true;
    }
  } else {
    const newC = new Float32Array(n * 3);
    for (let i = 0; i < n; i++) {
      if (selIdx.has(i)) {
        newC[i * 3] = 1;
        newC[i * 3 + 1] = 0.92;
        newC[i * 3 + 2] = 0.35;
      } else if (proxIdx.has(i)) {
        const entry = uniqueLabels.find((u) => u.label === curPoints[i].label);
        const [r, g, b] = hslToRgb(entry?.hue ?? 0, 0.85, 0.62);
        newC[i * 3] = r;
        newC[i * 3 + 1] = g;
        newC[i * 3 + 2] = b;
      } else {
        newC[i * 3] = baseColors[i * 3] * 0.5;
        newC[i * 3 + 1] = baseColors[i * 3 + 1] * 0.5;
        newC[i * 3 + 2] = baseColors[i * 3 + 2] * 0.5;
      }
    }
    colorAttr.array.set(newC);
    colorAttr.needsUpdate = true;
  }

  // ── 3. Recolor labelCloud dot + ring meshes ───────────────────────────
  if (labelCloud && labeledIdx.length) {
    for (let li = 0; li < labeledIdx.length; li++) {
      const ptIdx = labeledIdx[li];
      const dot = labelCloud.children[li * 2] as LabelMesh | undefined;
      const ring = labelCloud.children[li * 2 + 1] as LabelMesh | undefined;
      if (!dot?.material || !ring?.material) continue;

      const baseScale = dot._baseScale ?? 1;

      if (!selectedLabel) {
        const base = labelCloudBase[li];
        if (base) {
          dot.material.color.setHex(base.dc);
          dot.material.transparent = false;
          dot.material.opacity = 1;
          ring.material.color.setRGB(base.rr, base.rg, base.rb);
          ring.material.opacity = 0.85;
        }
        dot.scale.setScalar(baseScale);
        ring.scale.setScalar(baseScale);
      } else if (selIdx.has(ptIdx)) {
        // Selected: bright gold, scaled up for emphasis.
        dot.material.color.setRGB(1, 0.88, 0.2);
        dot.material.transparent = false;
        dot.material.opacity = 1;
        ring.material.color.setRGB(1, 0.88, 0.2);
        ring.material.opacity = 1.0;
        dot.scale.setScalar(baseScale * 1.45);
        ring.scale.setScalar(baseScale * 1.45);
      } else if (proxIdx.has(ptIdx)) {
        const entry = uniqueLabels.find((u) => u.label === curPoints[ptIdx].label);
        const [r, g, b] = hslToRgb(entry?.hue ?? 0, 0.85, 0.62);
        dot.material.color.setRGB(r, g, b);
        dot.material.transparent = false;
        dot.material.opacity = 1;
        ring.material.color.setRGB(r, g, b);
        ring.material.opacity = 0.9;
        dot.scale.setScalar(baseScale);
        ring.scale.setScalar(baseScale);
      } else {
        // Unrelated: 50% transparent, full colour, normal size.
        const base = labelCloudBase[li];
        if (base) {
          dot.material.color.setHex(base.dc);
          ring.material.color.setRGB(base.rr, base.rg, base.rb);
        }
        dot.material.transparent = true;
        dot.material.opacity = 0.5;
        ring.material.opacity = 0.5 * 0.85;
        dot.scale.setScalar(baseScale);
        ring.scale.setScalar(baseScale);
      }
      dot.material.needsUpdate = true;
      ring.material.needsUpdate = true;
    }
  }

  // ── 4. Cylinder edges between selected and proximate labeled nodes ─────
  clearHighlightEdges();
  if (selectedLabel && scene && THREE && selIdx.size > 0 && proxIdx.size > 0) {
    // Pre-build selected-label position vectors.
    const selVecs = [...selIdx].map(
      (i) => new THREE.Vector3(curPositions[i * 3], curPositions[i * 3 + 1], curPositions[i * 3 + 2]),
    );

    // For each proximate label keep only the single closest (proxPt → selPt) pair.
    interface BestEdge {
      pVec: ThreeVector3;
      sVec: ThreeVector3;
      dist: number;
      hue: number;
    }
    const best = new Map<string, BestEdge>();
    for (const ptIdx of proxIdx) {
      // biome-ignore lint/style/noNonNullAssertion: value is guaranteed by surrounding logic
      const lbl = curPoints[ptIdx].label!;
      const pVec = new THREE.Vector3(curPositions[ptIdx * 3], curPositions[ptIdx * 3 + 1], curPositions[ptIdx * 3 + 2]);
      let minD = Infinity,
        nearSel = selVecs[0];
      for (const sv of selVecs) {
        const d = pVec.distanceTo(sv);
        if (d < minD) {
          minD = d;
          nearSel = sv;
        }
      }
      const prev = best.get(lbl);
      if (!prev || minD < prev.dist) {
        const entry = uniqueLabels.find((u) => u.label === lbl);
        best.set(lbl, { pVec, sVec: nearSel, dist: minD, hue: entry?.hue ?? 0 });
      }
    }

    // Dot-sphere radius in world units (matches buildCloud: UMAP_POINT_SIZE * pointScale * 1.2)
    const dotR = UMAP_POINT_SIZE * pointScale * 1.2;
    const yAxis = new THREE.Vector3(0, 1, 0);
    const edgesGrp = new THREE.Group();

    for (const { pVec, sVec, dist, hue } of best.values()) {
      // t ∈ [0,1]: 1 = touching, 0 = at the radius limit → thicker when closer.
      const t = Math.max(0, 1 - dist / proximityDist);
      const radius = dotR * (0.06 + 0.7 * t); // thin at edge, fat when close
      const opacity = 0.2 + 0.7 * t; // subtle at edge, strong when close

      const dir = new THREE.Vector3().subVectors(sVec, pVec);
      const len = dir.length();
      if (len < 0.001) continue;

      const [r, g, b] = hslToRgb(hue, 0.85, 0.62);
      const geo = new THREE.CylinderGeometry(radius, radius, len, 8, 1);
      const mat = new THREE.MeshBasicMaterial({
        color: new THREE.Color(r, g, b),
        transparent: true,
        opacity,
        depthWrite: false,
      });
      const mesh = new THREE.Mesh(geo, mat);
      mesh.position.copy(new THREE.Vector3().addVectors(pVec, sVec).multiplyScalar(0.5));

      const dirNorm = dir.clone().normalize();
      if (Math.abs(yAxis.dot(dirNorm)) < 0.9999) {
        mesh.quaternion.setFromUnitVectors(yAxis, dirNorm);
      } else if (dirNorm.y < 0) {
        mesh.rotation.z = Math.PI;
      }
      edgesGrp.add(mesh);
    }

    if (edgesGrp.children.length) {
      highlightEdgesGroup = edgesGrp;
      scene.add(highlightEdgesGroup);
    }
  }
}

function addLinks(label: string) {
  if (!THREE || !scene || linkGroups.has(label)) return;
  const matching: number[] = [];
  for (let i = 0; i < curPoints.length; i++) if (curPoints[i].label === label) matching.push(i);
  if (matching.length < 2) return;
  matching.sort((a, b) => curPoints[a].utc - curPoints[b].utc);

  const colorIdx = nextColorIdx;
  const linkColor = LINK_PALETTE[colorIdx % LINK_PALETTE.length];
  nextColorIdx++;

  const group = new THREE.Group();

  // Main connecting line (2× thick)
  const lp: number[] = [];
  for (const idx of matching) lp.push(curPositions[idx * 3], curPositions[idx * 3 + 1], curPositions[idx * 3 + 2]);
  const lg = new THREE.BufferGeometry();
  lg.setAttribute("position", new THREE.Float32BufferAttribute(lp, 3));
  group.add(
    new THREE.Line(
      lg,
      new THREE.LineBasicMaterial({
        color: linkColor,
        transparent: true,
        opacity: 0.6,
        linewidth: 2,
      }),
    ),
  );

  // Node spheres
  const sg = new THREE.SphereGeometry(UMAP_POINT_SIZE * pointScale * 0.44, 8, 8);
  for (const idx of matching) {
    const m = new THREE.Mesh(sg, new THREE.MeshBasicMaterial({ color: linkColor, transparent: true, opacity: 0.9 }));
    m.position.set(curPositions[idx * 3], curPositions[idx * 3 + 1], curPositions[idx * 3 + 2]);
    group.add(m);
  }

  // Dashed lines to centroid
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
  const dm = new THREE.LineDashedMaterial({
    color: linkColor,
    transparent: true,
    opacity: 0.25,
    dashSize: 0.3,
    gapSize: 0.2,
    linewidth: 2,
  });
  for (const idx of matching) {
    const dg = new THREE.BufferGeometry().setFromPoints([
      new THREE.Vector3(curPositions[idx * 3], curPositions[idx * 3 + 1], curPositions[idx * 3 + 2]),
      new THREE.Vector3(cx2, cy2, cz2),
    ]);
    const dl = new THREE.Line(dg, dm.clone());
    dl.computeLineDistances();
    group.add(dl);
  }

  scene.add(group);
  linkGroups.set(label, { group, colorIdx });
  activeLabels = [...activeLabels, label];
}

// ── Chronological trace logic ───────────────────────────────────────────

function stopTrace() {
  if (traceTimer) {
    clearInterval(traceTimer);
    traceTimer = null;
  }
  traceGrowing = [];
  if (traceGroup && scene) {
    traceGroup.traverse(disposeSceneObject);
    scene.remove(traceGroup);
  }
  traceGroup = null;
  traceSorted = [];
  traceProgress = 0;
  traceTotal = 0;
  traceActive = false;
  traceTimeRange = null;
  traceTimeTicks = [];
}

function startTrace() {
  if (!THREE || !scene || curPoints.length < 2) return;
  stopTrace(); // clean any previous

  // Sort all point indices chronologically
  const indices = curPoints.map((_, i) => i).filter((i) => curPoints[i].utc > 0);
  indices.sort((a, b) => curPoints[a].utc - curPoints[b].utc);
  if (indices.length < 2) return;

  traceSorted = indices;
  traceTotal = indices.length - 1;
  traceProgress = 0;
  traceActive = true;
  buildTraceTimeTicks(indices);

  traceGroup = new THREE.Group();
  scene.add(traceGroup);
  const activeTraceGroup = traceGroup;

  // Trace line edge color: white on dark, black on light
  const edgeColor = new THREE.Color(isDark ? 0xffffff : 0x000000);

  // Place a small sphere on the first point (jet start)
  const firstIdx = traceSorted[0];
  const [jr0, jg0, jb0] = jet(0);
  const sphereGeo = new THREE.SphereGeometry(UMAP_POINT_SIZE * pointScale * 0.8, 8, 8);
  const sphereMat = new THREE.MeshBasicMaterial({
    color: new THREE.Color(jr0, jg0, jb0),
    transparent: true,
    opacity: 1.0,
  });
  const firstSphere = new THREE.Mesh(sphereGeo, sphereMat);
  firstSphere.position.set(curPositions[firstIdx * 3], curPositions[firstIdx * 3 + 1], curPositions[firstIdx * 3 + 2]);
  activeTraceGroup.add(firstSphere);

  let segIdx = 0;

  traceTimer = setInterval(() => {
    if (segIdx >= traceSorted.length - 1) {
      if (traceTimer) {
        clearInterval(traceTimer);
        traceTimer = null;
      }
      return;
    }

    const iA = traceSorted[segIdx];
    const iB = traceSorted[segIdx + 1];
    const ax = curPositions[iA * 3],
      ay = curPositions[iA * 3 + 1],
      az = curPositions[iA * 3 + 2];
    const bx = curPositions[iB * 3],
      by = curPositions[iB * 3 + 1],
      bz = curPositions[iB * 3 + 2];

    // Create line starting at point A with zero length (will animate to B)
    // Edge line (contrasting outline behind the jet-colored line)
    const edgeLineGeo = new THREE.BufferGeometry();
    edgeLineGeo.setAttribute("position", new THREE.Float32BufferAttribute([ax, ay, az, ax, ay, az], 3));
    const edgeLineMat = new THREE.LineBasicMaterial({
      color: edgeColor,
      transparent: true,
      opacity: isDark ? 0.5 : 0.3,
      linewidth: 3,
    });
    const edgeLine = new THREE.Line(edgeLineGeo, edgeLineMat);
    activeTraceGroup.add(edgeLine);

    // Jet-colored line on top
    const lineGeo = new THREE.BufferGeometry();
    lineGeo.setAttribute("position", new THREE.Float32BufferAttribute([ax, ay, az, ax, ay, az], 3));
    const age = segIdx / Math.max(traceSorted.length - 1, 1);
    const [jr, jg, jb] = jet(age);
    const lineColor = new THREE.Color(jr, jg, jb);
    const lineMat = new THREE.LineBasicMaterial({
      color: lineColor,
      transparent: true,
      opacity: 0.9,
      linewidth: 2,
    });
    const line = new THREE.Line(lineGeo, lineMat);
    activeTraceGroup.add(line);

    // Destination sphere — also jet-colored for continuity
    const sGeo = new THREE.SphereGeometry(UMAP_POINT_SIZE * pointScale * 0.6, 8, 8);
    const sMat = new THREE.MeshBasicMaterial({ color: lineColor, transparent: true, opacity: 0 });
    const sphere = new THREE.Mesh(sGeo, sMat);
    sphere.position.set(bx, by, bz);
    activeTraceGroup.add(sphere);

    traceGrowing.push({
      line,
      sphere,
      edgeLine,
      start: performance.now(),
      from: [ax, ay, az],
      to: [bx, by, bz],
    });

    segIdx++;
    traceProgress = segIdx;
  }, UMAP_TRACE_INTERVAL_MS);
}

function toggleTrace() {
  if (traceActive) {
    stopTrace();
  } else {
    startTrace();
  }
}

/** Called from the render loop to animate growing segments. */
function updateTraceGrow() {
  if (!traceGrowing.length) return;
  const now = performance.now();
  const done: number[] = [];
  for (let i = 0; i < traceGrowing.length; i++) {
    const seg = traceGrowing[i];
    const t = Math.min(1, (now - seg.start) / UMAP_TRACE_GROW_MS);
    const e = easeOut(t);
    // Grow line endpoint from A toward B
    const bx = seg.from[0] + (seg.to[0] - seg.from[0]) * e;
    const by = seg.from[1] + (seg.to[1] - seg.from[1]) * e;
    const bz = seg.from[2] + (seg.to[2] - seg.from[2]) * e;
    const pos = seg.line.geometry.getAttribute("position");
    pos.array[3] = bx;
    pos.array[4] = by;
    pos.array[5] = bz;
    pos.needsUpdate = true;
    // Grow edge line in sync
    if (seg.edgeLine) {
      const ep = seg.edgeLine.geometry.getAttribute("position");
      ep.array[3] = bx;
      ep.array[4] = by;
      ep.array[5] = bz;
      ep.needsUpdate = true;
    }
    // Fade in destination sphere
    seg.sphere.material.opacity = e * 1.0;
    if (t >= 1) done.push(i);
  }
  // Remove completed from active list (iterate backwards)
  for (let i = done.length - 1; i >= 0; i--) {
    traceGrowing.splice(done[i], 1);
  }
}

// ── Build point clouds ───────────────────────────────────────────────────

/** Build a date→color map using evenly spaced hues. */
function buildDatePalette(pts: UmapPoint[]): Map<string, ThreeColor> {
  const dates = [...new Set(pts.filter((p) => p.utc > 0).map((p) => utcToLocalDate(p.utc)))].sort();
  const map = new Map<string, ThreeColor>();
  for (let i = 0; i < dates.length; i++) {
    const hue = dates.length <= 1 ? 0.55 : i / dates.length;
    map.set(dates[i], new THREE.Color().setHSL(hue, UMAP_DATE_SAT, UMAP_DATE_LIT));
  }
  return map;
}

/** Exposed for the legend: unique dates with their hex colors. */
let dateLegend = $state<{ date: string; hex: string }[]>([]);

function buildCloud(pts: UmapPoint[], positions: Float32Array) {
  const n = pts.length;
  const colA = new THREE.Color(COLOR_A),
    colB = new THREE.Color(COLOR_B);
  const datePalette = colorByDate ? buildDatePalette(pts) : null;
  if (datePalette) {
    dateLegend = [...datePalette.entries()].map(([date, col]) => ({ date, hex: `#${col.getHexString()}` }));
  } else {
    dateLegend = [];
  }
  // ── Time-gradient palette for one session ─────────────────────────────
  const gradSess = timeGradient; // "A" | "B" | null
  let gradMinUtc = Infinity,
    gradMaxUtc = -Infinity;
  if (gradSess) {
    const sessIdx = gradSess === "A" ? 0 : 1;
    for (const p of pts) {
      if (p.session === sessIdx && p.utc > 0) {
        if (p.utc < gradMinUtc) gradMinUtc = p.utc;
        if (p.utc > gradMaxUtc) gradMaxUtc = p.utc;
      }
    }
    if (gradMaxUtc > gradMinUtc) {
      gradientRange = { minUtc: gradMinUtc, maxUtc: gradMaxUtc };
    } else {
      gradientRange = null;
    }
  } else {
    gradientRange = null;
  }
  const gradSpan = gradMaxUtc - gradMinUtc || 1;
  // Muted grey for the "other" session — lighter on dark, darker on light
  const fadedColor = isDark ? new THREE.Color(0.35, 0.35, 0.4) : new THREE.Color(0.72, 0.72, 0.75);

  const colors = new Float32Array(n * 3);
  const lPos: number[] = [],
    lCol: number[] = [],
    lIdx: number[] = [];
  for (let i = 0; i < n; i++) {
    let c: ThreeColor;
    if (gradSess) {
      const sessIdx = gradSess === "A" ? 0 : 1;
      if (pts[i].session === sessIdx && pts[i].utc > 0) {
        const t = (pts[i].utc - gradMinUtc) / gradSpan;
        const [jr, jg, jb] = jet(t);
        c = new THREE.Color(jr, jg, jb);
      } else {
        c = fadedColor;
      }
    } else if (datePalette && pts[i].utc > 0) {
      c = datePalette.get(utcToLocalDate(pts[i].utc)) ?? colA;
    } else {
      c = pts[i].session === 0 ? colA : colB;
    }
    colors[i * 3] = c.r;
    colors[i * 3 + 1] = c.g;
    colors[i * 3 + 2] = c.b;
    if (pts[i].label) {
      lPos.push(positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]);
      lCol.push(c.r, c.g, c.b);
      lIdx.push(i);
    }
  }
  if (mainCloud) {
    scene.remove(mainCloud);
    mainCloud.geometry.dispose();
    mainCloud.material.dispose();
  }
  if (labelCloud) {
    scene.remove(labelCloud);
    labelCloud.traverse(disposeSceneObject);
    labelCloud = null;
  }
  clearAllLinks();
  const ps = UMAP_POINT_SIZE * pointScale;
  if (raycaster) raycaster.params.Points.threshold = ps * 0.6;
  const g = new THREE.BufferGeometry();
  g.setAttribute("position", new THREE.Float32BufferAttribute(positions.slice(), 3));
  g.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  mainCloud = new THREE.Points(
    g,
    new THREE.PointsMaterial({
      size: ps,
      vertexColors: true,
      transparent: true,
      opacity: 0.5,
      sizeAttenuation: true,
      depthWrite: false,
    }),
  );
  scene.add(mainCloud);

  // Labeled points — grouped by label, each label gets a unique hue-shifted
  // ring color so you can visually distinguish clusters with the same label.
  labelCloudBase = [];
  if (lIdx.length) {
    // Collect unique labels and assign each a bright hue
    // biome-ignore lint/style/noNonNullAssertion: value is guaranteed by surrounding logic
    const uniqueLabels = [...new Set(lIdx.map((i) => pts[i].label!))];
    const labelColorMap = new Map<string, ThreeColor>();
    for (let li = 0; li < uniqueLabels.length; li++) {
      const hue = li / Math.max(uniqueLabels.length, 1);
      labelColorMap.set(uniqueLabels[li], new THREE.Color().setHSL(hue, 0.9, 0.65));
    }

    // For each labeled point, create a shape + billboard ring encoding the session:
    //   Session A (0) → sphere       + smooth 24-segment circular ring
    //   Session B (1) → octahedron   + 4-segment ring rotated 45° (diamond border)
    const labelGrp = new THREE.Group();
    const DOT_R = UMAP_POINT_SIZE * pointScale * 1.2;
    const RING_I = UMAP_POINT_SIZE * pointScale * 1.8;
    const RING_O = UMAP_POINT_SIZE * pointScale * 2.4;

    // Pre-compute dist range so we can scale nodes: closer → larger, farther → smaller.
    const distVals = lIdx.map((i) => pts[i].dist).filter((d): d is number => d != null);
    const distMin = distVals.length ? Math.min(...distVals) : 0;
    const distMax = distVals.length ? Math.max(...distVals) : 1;
    const distSpan = distMax - distMin || 1;
    const SCALE_NEAR = 1.9,
      SCALE_FAR = 0.5;

    for (let li = 0; li < lIdx.length; li++) {
      const i = lIdx[li];
      const sess = pts[i].session; // 0 = A, 1 = B
      const sessionCol =
        datePalette && pts[i].utc > 0
          ? (datePalette.get(utcToLocalDate(pts[i].utc))?.getHex() ?? COLOR_A)
          : sess === 0
            ? COLOR_A
            : COLOR_B;
      // biome-ignore lint/style/noNonNullAssertion: value is guaranteed by surrounding logic
      const ringCol = labelColorMap.get(pts[i].label!) ?? new THREE.Color(0xffffff);
      const px = positions[i * 3],
        py = positions[i * 3 + 1],
        pz = positions[i * 3 + 2];

      // Scale inversely by distance: closest node is SCALE_NEAR×, farthest is SCALE_FAR×.
      const d = pts[i].dist;
      const sizeMult = d != null ? SCALE_NEAR - ((d - distMin) / distSpan) * (SCALE_NEAR - SCALE_FAR) : 1.0;

      // Dot: sphere for session A, octahedron for session B.
      const dotGeo = sess === 0 ? new THREE.SphereGeometry(DOT_R, 8, 8) : new THREE.OctahedronGeometry(DOT_R);
      const dot = new THREE.Mesh(dotGeo, new THREE.MeshBasicMaterial({ color: sessionCol }));
      dot.position.set(px, py, pz);
      dot.scale.setScalar(sizeMult);
      (dot as LabelMesh)._baseScale = sizeMult;
      labelGrp.add(dot);

      // Ring: smooth circle for A; 4-segment square rotated 45° → diamond for B.
      const ringGeo =
        sess === 0 ? new THREE.RingGeometry(RING_I, RING_O, 24) : new THREE.RingGeometry(RING_I, RING_O, 4);
      const ring = new THREE.Mesh(
        ringGeo,
        new THREE.MeshBasicMaterial({
          color: ringCol,
          transparent: true,
          opacity: 0.85,
          side: THREE.DoubleSide,
        }),
      );
      ring.position.set(px, py, pz);
      ring.scale.setScalar(sizeMult);
      if (sess === 1) ring.rotation.z = Math.PI / 4; // square → diamond orientation
      ring.userData.billboard = true;
      (ring as LabelMesh)._baseScale = sizeMult;
      labelGrp.add(ring);

      // Save base colors so applyHighlight can restore them later.
      const dc = typeof sessionCol === "number" ? sessionCol : new THREE.Color(sessionCol).getHex();
      labelCloudBase.push({ dc, rr: ringCol.r, rg: ringCol.g, rb: ringCol.b });
    }

    labelCloud = labelGrp as LabelCloudGroup;
    scene.add(labelCloud);

    // Store labeled geometry update function for animation
    labelCloud.__updatePositions = (pos: Float32Array) => {
      let ci = 0;
      for (let li = 0; li < lIdx.length; li++) {
        const i = lIdx[li];
        const px = pos[i * 3],
          py = pos[i * 3 + 1],
          pz = pos[i * 3 + 2];
        // dot
        labelGrp.children[ci].position.set(px, py, pz);
        ci++;
        // ring
        labelGrp.children[ci].position.set(px, py, pz);
        ci++;
      }
    };
  }
  curPoints = pts;
  labeledIdx = lIdx;
  curPositions = positions.slice();
  baseColors = colors.slice();
  if (selectedLabel) applyHighlight();
}

function applyPositions(pos: Float32Array) {
  if (!mainCloud) return;
  const a = mainCloud.geometry.getAttribute("position");
  a.array.set(pos);
  a.needsUpdate = true;
  labelCloud?.__updatePositions?.(pos);
  curPositions = pos;
}

// ── Handle data changes ──────────────────────────────────────────────────
function onDataChange(d: UmapResult) {
  if (!scene || !d?.points?.length) return;
  stopTrace();
  selectedLabel = null;
  proximateLabels = [];
  baseColors = null;
  labelCloudBase = [];
  clearHighlightEdges();
  animating = true;
  const target = normalise(d.points);
  const start = randomPositions(d.points);
  buildCloud(d.points, start);
  fromPos = start;
  toPos = target;
  animStart = performance.now();
  // Schedule auto-connect to fire once the intro animation lands at final positions.
  if (autoConnectLabels) pendingAutoConnect = true;
}

$effect(() => {
  const d = data;
  if (!loaded || !d?.points?.length) return;
  if (d === prevDataRef) return;
  prevDataRef = d;
  onDataChange(d);
});

// Re-scale points when pointScale changes
let prevPointScale: number | null = null;
$effect(() => {
  const s = pointScale;
  if (prevPointScale === null) {
    prevPointScale = s;
    return;
  }
  if (s === prevPointScale) return;
  prevPointScale = s;
  if (mainCloud) {
    const ps = UMAP_POINT_SIZE * s;
    mainCloud.material.size = ps;
    mainCloud.material.needsUpdate = true;
    if (raycaster) raycaster.params.Points.threshold = ps * 0.6;
  }
  // Rebuild labeled cloud geometries at new scale
  if (curPoints.length > 0 && scene) {
    buildCloud(curPoints, curPositions);
  }
});

// Re-color when colorByDate toggles (no animation, just rebuild colors)
let prevColorByDate: boolean | null = null;
$effect(() => {
  const cbd = colorByDate;
  if (prevColorByDate === null) {
    prevColorByDate = cbd;
    return;
  }
  if (cbd === prevColorByDate) return;
  prevColorByDate = cbd;
  if (cbd) timeGradient = null; // colorByDate overrides time gradient
  if (curPoints.length > 0 && scene) {
    buildCloud(curPoints, curPositions);
  }
});

// Re-color when timeGradient toggles
let prevTimeGradient: "A" | "B" | null | undefined;
$effect(() => {
  const tg = timeGradient;
  if (prevTimeGradient === undefined) {
    prevTimeGradient = tg;
    return;
  }
  if (tg === prevTimeGradient) return;
  prevTimeGradient = tg;
  if (curPoints.length > 0 && scene) {
    buildCloud(curPoints, curPositions);
  }
});

// Re-apply highlight when proximityDist slider changes (the slider uses bind:value so
// only this effect is needed; label clicks call applyHighlight() directly).
$effect(() => {
  const _dist = proximityDist;
  if (loaded && !animating && selectedLabel) applyHighlight();
});

// ── Export ───────────────────────────────────────────────────────────────

/** Brief "✓ saved" flash after a successful export. */
let exportFlash = $state<"png" | "json" | null>(null);
let exportFlashTimer: ReturnType<typeof setTimeout> | null = null;

function flashExport(kind: "png" | "json") {
  exportFlash = kind;
  if (exportFlashTimer) clearTimeout(exportFlashTimer);
  exportFlashTimer = setTimeout(() => {
    exportFlash = null;
  }, 1800);
}

/**
 * Capture the WebGL canvas as a PNG and trigger a browser download.
 *
 * Requires `preserveDrawingBuffer: true` on the renderer (set in onMount).
 * We force a synchronous render immediately before calling `toDataURL` so
 * the buffer is guaranteed to contain the current frame.
 */
function exportPng() {
  if (!renderer || !scene || !camera) return;
  renderer.render(scene, camera); // ensure buffer is current
  const url = renderer.domElement.toDataURL("image/png");
  const a = document.createElement("a");
  a.href = url;
  a.download = `umap-${new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19)}.png`;
  a.click();
  flashExport("png");
}

/**
 * Serialise the current point cloud to JSON and trigger a browser download.
 *
 * Each entry contains:
 * - `x_umap / y_umap / z_umap` — raw UMAP coordinates (dimensionality-
 *   reduced space, as returned by the Rust UMAP worker)
 * - `x3d / y3d / z3d` — normalised Three.js scene coordinates used for
 *   rendering (centred on origin, scaled to ±UMAP_SCALE/2)
 * - `session` — 0 = A, 1 = B
 * - `utc` — Unix timestamp of the EEG window (seconds)
 * - `label` — optional annotation string
 * - `dist` — optional semantic distance from the search anchor
 */
function exportJson() {
  if (!curPoints.length) return;
  const pts = curPoints.map((p, i) => {
    const entry: Record<string, unknown> = {
      x_umap: p.x,
      y_umap: p.y,
      z_umap: p.z ?? 0,
      x3d: curPositions[i * 3],
      y3d: curPositions[i * 3 + 1],
      z3d: curPositions[i * 3 + 2],
      session: p.session,
      utc: p.utc,
    };
    if (p.label != null) entry.label = p.label;
    if (p.dist != null) entry.dist = p.dist;
    return entry;
  });

  const payload = {
    exported_at: new Date().toISOString(),
    n_points: curPoints.length,
    n_a: data.n_a,
    n_b: data.n_b,
    dim: data.dim,
    points: pts,
  };

  const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `umap-${new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19)}.json`;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
  flashExport("json");
}

// ── Mount ────────────────────────────────────────────────────────────────
onMount(async () => {
  if (!container) return;
  try {
    THREE = await import("three");
    const { OrbitControls } = await import("three/examples/jsm/controls/OrbitControls.js");

    // Create canvas manually and append to container
    const canvas = document.createElement("canvas");
    canvas.style.width = "100%";
    canvas.style.height = "100%";
    canvas.style.display = "block";
    container.appendChild(canvas);

    // Wait for layout — container may have 0 width on first frame
    let w = container.clientWidth;
    let h = container.clientHeight;
    if (w < 1 || h < 1) {
      // Force a reflow and try again after a frame
      await new Promise((r) => requestAnimationFrame(r));
      w = container.clientWidth;
      h = container.clientHeight;
    }
    if (w < 1 || h < 1) {
      // Still 0 — try parentElement dimensions
      w = container.parentElement?.clientWidth || 600;
      h = container.parentElement?.clientHeight || 400;
    }

    // Scene — theme-aware background
    scene = new THREE.Scene();
    scene.background = new THREE.Color(isDark ? UMAP_BG : UMAP_BG_LIGHT);

    // Camera
    camera = new THREE.PerspectiveCamera(55, w / h, 0.1, 1000);
    camera.position.set(0, 0, 25);

    // Renderer — use the canvas we created
    // preserveDrawingBuffer keeps the framebuffer alive after each render
    // so that exportPng() can call canvas.toDataURL() at any time.
    renderer = new THREE.WebGLRenderer({ canvas, antialias: true, preserveDrawingBuffer: true });
    renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
    renderer.setSize(w, h);

    // Controls
    controls = new OrbitControls(camera, canvas);
    controls.enableDamping = true;
    controls.dampingFactor = 0.08;
    controls.autoRotate = true;
    controls.autoRotateSpeed = 0.8;
    controls.minDistance = 5;
    controls.maxDistance = 80;

    // ── Cmd / Ctrl + drag → pan camera ───────────────────────────────
    // OrbitControls reads mouseButtons at the start of each drag, so we
    // switch the LEFT button mapping while the modifier key is held.
    _onCmdDown = (e: KeyboardEvent) => {
      if (e.key !== "Meta" && e.key !== "Control") return;
      controls.mouseButtons = {
        LEFT: THREE.MOUSE.PAN,
        MIDDLE: THREE.MOUSE.DOLLY,
        RIGHT: THREE.MOUSE.PAN,
      };
      cmdHeld = true;
    };
    _onCmdUp = (e: KeyboardEvent) => {
      if (e.key !== "Meta" && e.key !== "Control") return;
      controls.mouseButtons = {
        LEFT: THREE.MOUSE.ROTATE,
        MIDDLE: THREE.MOUSE.DOLLY,
        RIGHT: THREE.MOUSE.PAN,
      };
      cmdHeld = false;
    };
    window.addEventListener("keydown", _onCmdDown);
    window.addEventListener("keyup", _onCmdUp);

    // Grid + light — theme-aware
    const gridMain = isDark ? 0x444466 : 0x999999;
    const gridSub = isDark ? 0x333355 : 0xbbbbbb;
    const grid = new THREE.GridHelper(SCALE * 1.2, 8, gridMain, gridSub);
    grid.position.y = -SCALE * 0.5 - 0.5;
    scene.add(grid);
    scene.add(new THREE.AmbientLight(0xffffff, 0.5));

    // Raycaster
    raycaster = new THREE.Raycaster();
    raycaster.params.Points = { threshold: 0.5 };
    mouse = new THREE.Vector2();

    // ── Hover ────────────────────────────────────────────────────────
    canvas.addEventListener("pointermove", (e: PointerEvent) => {
      if (!curPoints.length) {
        tooltip = null;
        return;
      }
      const r = canvas.getBoundingClientRect();
      mouse.x = ((e.clientX - r.left) / r.width) * 2 - 1;
      mouse.y = -((e.clientY - r.top) / r.height) * 2 + 1;
      raycaster.setFromCamera(mouse, camera);
      let hitIdx = -1;
      // Check labeled group (meshes, every 2nd child is a dot)
      if (labelCloud) {
        const h = raycaster.intersectObjects(labelCloud.children, false);
        if (h.length) {
          // Each labeled point = 2 children (dot + ring), so child index / 2 = label index
          const childIdx = labelCloud.children.indexOf(h[0].object);
          const li = Math.floor(childIdx / 2);
          if (li >= 0 && li < labeledIdx.length) hitIdx = labeledIdx[li];
        }
      }
      if (hitIdx < 0 && mainCloud) {
        const h = raycaster.intersectObject(mainCloud);
        // biome-ignore lint/style/noNonNullAssertion: value is guaranteed by surrounding logic
        if (h.length) hitIdx = h[0].index!;
      }
      if (hitIdx >= 0 && hitIdx < curPoints.length) {
        const p = curPoints[hitIdx];
        const dt = new Date(p.utc * 1000);
        const s = p.session === 0 ? "A" : "B";
        const dateStr = colorByDate || timeGradient ? `${utcToLocalDate(p.utc)} ` : "";
        const distStr = p.dist != null ? `\n📏 dist ${p.dist.toFixed(4)}` : "";
        const lb = p.label ? `\n🏷 ${p.label}` : "";
        const ch = p.label ? `\n${t("umap.clickConnect")}` : "";
        tooltip = {
          x: e.clientX - r.left,
          y: e.clientY - r.top,
          text: `${dateStr}${s} · ${dt.toLocaleTimeString()}${distStr}${lb}${ch}`,
        };
      } else {
        tooltip = null;
      }
    });
    canvas.addEventListener("pointerleave", () => {
      tooltip = null;
    });

    // ── Click ────────────────────────────────────────────────────────
    canvas.addEventListener("pointerdown", (e: PointerEvent) => {
      downPos = { x: e.clientX, y: e.clientY };
    });
    canvas.addEventListener("pointerup", (e: PointerEvent) => {
      if ((e.clientX - downPos.x) ** 2 + (e.clientY - downPos.y) ** 2 > 25) return;
      const r = canvas.getBoundingClientRect();
      mouse.x = ((e.clientX - r.left) / r.width) * 2 - 1;
      mouse.y = -((e.clientY - r.top) / r.height) * 2 + 1;
      raycaster.setFromCamera(mouse, camera);
      let label: string | undefined;
      if (labelCloud) {
        const h = raycaster.intersectObjects(labelCloud.children, false);
        if (h.length) {
          const childIdx = labelCloud.children.indexOf(h[0].object);
          const li = Math.floor(childIdx / 2);
          if (li >= 0 && li < labeledIdx.length) label = curPoints[labeledIdx[li]]?.label;
        }
      }
      if (!label && mainCloud) {
        const h = raycaster.intersectObject(mainCloud);
        // biome-ignore lint/style/noNonNullAssertion: value is guaranteed by surrounding logic
        if (h.length) label = curPoints[h[0].index!]?.label;
      }
      if (label) {
        toggleLinks(label);
      }
    });

    // ── Resize ───────────────────────────────────────────────────────
    resizeObs = new ResizeObserver(() => {
      if (!container || !renderer) return;
      const nw = container.clientWidth || 600;
      const nh = container.clientHeight || 400;
      if (nw < 2 || nh < 2) return;
      camera.aspect = nw / nh;
      camera.updateProjectionMatrix();
      renderer.setSize(nw, nh);
    });
    resizeObs.observe(container);

    // ── Render loop ──────────────────────────────────────────────────
    function loop() {
      animId = requestAnimationFrame(loop);
      // Animation
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
          animating = false;
          if (pendingAutoConnect) {
            pendingAutoConnect = false;
            runAutoConnect();
          }
        }
      }
      // Chronological trace grow animation
      updateTraceGrow();
      // Billboard rings — make them face the camera
      if (labelCloud) {
        labelCloud.traverse((child) => {
          if (child.userData?.billboard && camera) {
            child.quaternion.copy(camera.quaternion);
          }
        });
      }
      controls.update();
      renderer.render(scene, camera);
    }
    loop();

    loaded = true;

    // Watch for theme changes and update scene, cloud colors, and trace
    const themeObs = new MutationObserver(() => {
      const dark = document.documentElement.classList.contains("dark");
      if (scene) scene.background = new THREE.Color(dark ? UMAP_BG : UMAP_BG_LIGHT);
      if (grid) {
        const cols = grid.material;
        if (Array.isArray(cols)) {
          cols[0]?.color?.setHex(dark ? 0x444466 : 0x999999);
          cols[1]?.color?.setHex(dark ? 0x333355 : 0xbbbbbb);
        }
      }
      // Rebuild cloud colors (jet / faded colors are theme-dependent)
      if (curPoints.length > 0 && scene) {
        buildCloud(curPoints, curPositions);
      }
      // Restart trace with new theme colors if active
      if (traceActive) {
        stopTrace();
        startTrace();
      }
    });
    themeObs.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });

    // Process current data if already available
    if (data?.points?.length) {
      prevDataRef = data;
      onDataChange(data);
    }
  } catch (e: unknown) {
    error = String(e);
  }
});

onDestroy(() => {
  stopTrace();
  clearHighlightEdges();
  if (exportFlashTimer) clearTimeout(exportFlashTimer);
  if (_onCmdDown) window.removeEventListener("keydown", _onCmdDown);
  if (_onCmdUp) window.removeEventListener("keyup", _onCmdUp);
  if (animId) cancelAnimationFrame(animId);
  resizeObs?.disconnect();
  controls?.dispose();
  renderer?.dispose();
});
</script>

<div class="flex flex-col" style="width:100%; height:100%;">

  <!-- Trace time gradient bar — rendered above the 3D canvas in normal flow -->
  {#if traceActive && traceTimeRange && traceTimeTicks.length > 0}
    {@const prog = traceTotal > 0 ? traceProgress / traceTotal : 0}
    <div class="shrink-0 select-none px-3 py-1.5
                bg-white dark:bg-[#111118] border-b border-black/8 dark:border-white/8">
      <div class="flex flex-col gap-0.5">
        <!-- Gradient bar with playhead -->
        <div class="relative w-full h-2 rounded-full overflow-hidden
                    border border-black/10 dark:border-white/10"
             style="background:{jetGradientCSS}">
          <!-- Playhead marker -->
          <div class="absolute top-0 h-full w-0.5 bg-black dark:bg-white shadow-sm shadow-black/40 dark:shadow-white/40 transition-all duration-100"
               style="left:{prog * 100}%"></div>
        </div>
        <!-- Timestamp ticks -->
        <div class="relative w-full h-3">
          {#each traceTimeTicks as tick, i}
            {@const align = i === 0 ? 'left-0' : i === traceTimeTicks.length - 1 ? 'right-0' : '-translate-x-1/2'}
            <span class="absolute text-[0.38rem] tabular-nums text-slate-500 dark:text-white/50 top-0 whitespace-nowrap {align}"
                  style={i > 0 && i < traceTimeTicks.length - 1 ? `left:${tick.pct}%` : ''}>
              {tick.label}
            </span>
          {/each}
        </div>
      </div>
    </div>
  {/if}

  <div class="flex flex-1 min-h-0">

  <!-- ── Label sidebar ──────────────────────────────────────────────────── -->
  {#if sidebarOpen && uniqueLabels.length > 0}
    {@const groups = [
      { key: "common",  label: t("umap.common"),   items: uniqueLabels.filter(l =>  l.inA &&  l.inB) },
      { key: "a-only",  label: t("umap.sessionA"),  items: uniqueLabels.filter(l =>  l.inA && !l.inB) },
      { key: "b-only",  label: t("umap.sessionB"),  items: uniqueLabels.filter(l => !l.inA &&  l.inB) },
    ].filter(g => g.items.length > 0)}
    <div class="w-52 shrink-0 flex flex-col overflow-hidden select-none
                bg-white/95 dark:bg-[#0f0f1a]
                border-r border-black/10 dark:border-white/[0.07]">
      <!-- Header -->
      <div class="flex items-center justify-between px-3 py-2 shrink-0
                  border-b border-black/8 dark:border-white/[0.06]">
        <span class="text-[0.56rem] font-semibold tracking-widest uppercase
                     text-slate-500 dark:text-white/50">
          {t("umap.labels")}
        </span>
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span class="text-[0.62rem] text-slate-400 dark:text-white/30
                     hover:text-slate-600 dark:hover:text-white/60 cursor-pointer leading-none"
              onclick={() => { sidebarOpen = false; selectedLabel = null; applyHighlight(); }}>✕</span>
      </div>
      <!-- Shape legend -->
      <div class="flex items-center gap-3 px-3 py-1.5 shrink-0
                  border-b border-black/6 dark:border-white/[0.05]
                  bg-black/[0.02] dark:bg-white/[0.02]">
        <span class="flex items-center gap-1 text-[0.44rem] text-slate-400 dark:text-white/30">
          <span class="w-1.5 h-1.5 rounded-full bg-slate-400 dark:bg-white/40 shrink-0"></span>
          {t("umap.sessionA")}
        </span>
        <span class="flex items-center gap-1 text-[0.44rem] text-slate-400 dark:text-white/30">
          <span class="w-1.5 h-1.5 rotate-45 bg-slate-400 dark:bg-white/40 shrink-0"></span>
          {t("umap.sessionB")}
        </span>
      </div>

      <!-- Label list -->
      <div class="flex-1 overflow-y-auto py-1">
        {#each groups as group}
          <!-- Group header -->
          <div class="flex items-center gap-1.5 px-3 pt-2 pb-0.5">
            <span class="text-[0.44rem] font-semibold tracking-widest uppercase
                         text-slate-400 dark:text-white/25 shrink-0">{group.label}</span>
            <div class="flex-1 h-px bg-black/6 dark:bg-white/[0.05]"></div>
          </div>

          {#each group.items as entry}
            {@const isSelected  = selectedLabel === entry.label}
            {@const isProximate = proximateLabels.includes(entry.label)}
            {@const hex = labelHex(entry.hue)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="mx-1.5 mb-0.5 px-2 py-1.5 rounded-lg cursor-pointer
                        flex flex-col gap-0.5 transition-colors duration-100
                        {isSelected
                          ? 'bg-amber-400/15 dark:bg-amber-400/12 border border-amber-400/30 dark:border-amber-400/25'
                          : isProximate
                            ? 'bg-sky-500/10 dark:bg-sky-500/12 border border-sky-400/25 dark:border-sky-400/20'
                            : 'border border-transparent hover:bg-black/4 dark:hover:bg-white/[0.04]'}"
                 onclick={() => { selectedLabel = selectedLabel === entry.label ? null : entry.label; applyHighlight(); }}>
              <!-- Row: shape-icon · label · session badges -->
              <div class="flex items-center gap-1.5">
                {#if entry.inA && entry.inB}
                  <!-- Common: show both shapes side-by-side -->
                  <span class="w-1.5 h-1.5 rounded-full shrink-0" style="background:{hex}"></span>
                  <span class="w-1.5 h-1.5 rotate-45 shrink-0 -ml-1" style="background:{hex}"></span>
                {:else if entry.inA}
                  <span class="w-1.5 h-1.5 rounded-full shrink-0" style="background:{hex}"></span>
                {:else}
                  <span class="w-1.5 h-1.5 rotate-45 shrink-0" style="background:{hex}"></span>
                {/if}
                <span class="flex-1 text-[0.6rem] font-medium truncate leading-tight
                             text-slate-700 dark:text-white/75">{entry.label}</span>
                <div class="flex gap-0.5 shrink-0">
                  {#if entry.inA}
                    <span class="text-[0.42rem] px-0.5 py-px rounded
                                 bg-blue-500/12 text-blue-600 dark:text-blue-400 font-semibold">A</span>
                  {/if}
                  {#if entry.inB}
                    <span class="text-[0.42rem] px-0.5 py-px rounded
                                 bg-amber-500/12 text-amber-600 dark:text-amber-400 font-semibold">B</span>
                  {/if}
                </div>
              </div>
              <!-- Timestamps -->
              <div class="flex flex-wrap gap-0.5 pl-3">
                {#each entry.timestamps.slice(0, 6) as ts}
                  <span class="text-[0.4rem] tabular-nums px-0.5 rounded
                               {ts.session === 0
                                 ? 'bg-primary/10 text-primary/80'
                                 : 'bg-amber-500/10 text-amber-600 dark:text-amber-400/80'}">
                    {fmtUtcTime(ts.utc)}
                  </span>
                {/each}
                {#if entry.timestamps.length > 6}
                  <span class="text-[0.4rem] text-slate-400 dark:text-white/25">
                    +{entry.timestamps.length - 6}
                  </span>
                {/if}
              </div>
            </div>
          {/each}
        {/each}
      </div>

      <!-- Proximity slider -->
      <div class="shrink-0 border-t border-black/8 dark:border-white/[0.06]
                  px-3 py-2.5 flex flex-col gap-1.5">
        <div class="flex items-center justify-between">
          <span class="text-[0.5rem] font-medium text-slate-500 dark:text-white/40">
            {t("umap.proximity")}
          </span>
          <span class="text-[0.5rem] tabular-nums text-slate-400 dark:text-white/30">
            {proximityDist.toFixed(1)}
          </span>
        </div>
        <input type="range" min="0.5" max="10" step="0.1"
               bind:value={proximityDist}
           class="w-full h-1 accent-violet-500 dark:accent-violet-400 cursor-pointer" />
        <div class="text-[0.44rem] leading-snug min-h-[1.6em]">
          {#if selectedLabel && !animating}
            {#if proximateLabels.length > 0}
              <span class="text-sky-600 dark:text-sky-400">
                {proximateLabels.length} {t("umap.nearbyLabels")}
              </span>
            {:else}
              <span class="text-slate-400 dark:text-white/30">{t("umap.noNearbyLabels")}</span>
            {/if}
          {:else if animating}
            <span class="text-slate-400 dark:text-white/25 italic">{t("umap.computing")}</span>
          {:else}
            <span class="text-slate-400/60 dark:text-white/20 italic">{t("umap.selectLabelHint")}</span>
          {/if}
        </div>
      </div>
    </div>
  {/if}

  <!-- ── 3D canvas ─────────────────────────────────────────────────────── -->
  <div class="relative flex-1 min-w-0 rounded-lg bg-slate-100 dark:bg-[#1a1a2e]"
       class:cursor-move={cmdHeld}
       bind:this={container}>

  <!-- Pan-mode badge -->
  {#if cmdHeld}
    <div class="absolute top-2.5 left-1/2 -translate-x-1/2 z-20 pointer-events-none
                px-2.5 py-1 rounded-full text-[0.52rem] font-medium select-none
                bg-white/80 dark:bg-black/70 backdrop-blur-md shadow-lg
                border border-black/10 dark:border-white/10
                text-slate-600 dark:text-white/70">
      ⌘ Pan
    </div>
  {/if}

  {#if error}
    <div class="absolute inset-0 z-20 flex flex-col items-center justify-center gap-2 text-red-400 text-xs p-4">
      <span>{t("umap.error")}</span>
      <pre class="text-[0.5rem] text-red-400/60 max-w-full overflow-auto whitespace-pre-wrap">{error}</pre>
    </div>
  {:else if !loaded}
    <div class="absolute inset-0 z-20 flex items-center justify-center">
      <div class="flex items-center gap-2 text-slate-400 dark:text-white/30 text-xs">
        <Spinner size="w-4 h-4" />
        {t("umap.loading")}
      </div>
    </div>
  {/if}

  <!-- "Computing" overlay -->
  {#if computing}
    <div class="absolute inset-0 z-10 flex items-center justify-center pointer-events-none">
      <div class="flex flex-col items-center gap-2.5 px-5 py-4 rounded-2xl
                  bg-white/70 dark:bg-black/50 backdrop-blur-md
                  border border-black/10 dark:border-white/10 shadow-2xl
                  min-w-[200px]">
        <div class="relative w-10 h-10 flex items-center justify-center">
          <div class="absolute inset-0 rounded-full border-2 border-black/10 dark:border-white/10 animate-ping"
               style="animation-duration:2.5s"></div>
          <Spinner size="w-5 h-5" class="text-slate-500 dark:text-white/40" />
        </div>
        <span class="text-[0.65rem] font-medium text-slate-700 dark:text-white/70">{t("umap.computing")}</span>
        {#if progress && progress.total_epochs > 0}
          {@const pct = Math.round(progress.epoch / progress.total_epochs * 100)}
          <!-- Progress bar -->
          <div class="w-full flex items-center gap-2">
            <div class="flex-1 h-1.5 rounded-full bg-black/10 dark:bg-white/[0.08] overflow-hidden">
              <div class="h-full rounded-full bg-blue-500 dark:bg-blue-400 transition-all duration-300"
                   style="width:{pct}%"></div>
            </div>
            <span class="text-[0.5rem] text-slate-500 dark:text-white/40 tabular-nums shrink-0">{pct}%</span>
          </div>
          <!-- Epoch info -->
          <span class="text-[0.48rem] text-slate-400 dark:text-white/30 tabular-nums">
            epoch {progress.epoch}/{progress.total_epochs} · {progress.epoch_ms.toFixed(0)}ms/ep
          </span>
        {:else}
          <span class="text-[0.5rem] text-slate-400 dark:text-white/35">{t("umap.placeholder")}</span>
        {/if}
      </div>
    </div>
  {/if}

  <!-- Active label pills (multi-label) -->
  {#if activeLabels.length > 0}
    <div class="absolute top-2.5 left-2.5 z-10 flex flex-wrap items-center gap-1.5 max-w-[80%] select-none">
      {#each activeLabels as label}
        {@const entry = linkGroups.get(label)}
        {@const col = entry ? LINK_PALETTE[entry.colorIdx % LINK_PALETTE.length] : 0xffffff}
        <div class="flex items-center gap-1 px-2 py-0.5 rounded-full
                    bg-black/5 dark:bg-white/10 backdrop-blur-md
                    border border-black/10 dark:border-white/15
                    text-[0.55rem] text-slate-700 dark:text-white/80 shadow-lg">
          <span class="inline-block w-2 h-2 rounded-full shrink-0"
                style="background:#{col.toString(16).padStart(6, '0')}"></span>
          <span class="font-semibold truncate max-w-[100px]">{label}</span>
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <span class="ml-0.5 cursor-pointer text-slate-400 dark:text-white/40
                       hover:text-slate-700 dark:hover:text-white/80 transition-colors
                       text-[0.65rem] leading-none"
                onclick={() => removeLinkGroup(label)}>✕</span>
        </div>
      {/each}
      {#if activeLabels.length > 1}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="flex items-center gap-1 px-2 py-0.5 rounded-full cursor-pointer
                    bg-red-500/10 dark:bg-red-500/20 hover:bg-red-500/20 dark:hover:bg-red-500/30
                    backdrop-blur-md border border-red-500/20 dark:border-red-500/30
                    text-[0.55rem] text-red-600 dark:text-red-300/80 shadow-lg transition-colors"
             onclick={() => clearAllLinks()}>
          <span>{t("umap.clearAll")}</span>
        </div>
      {/if}
    </div>
  {/if}

  <!-- Bottom-left legend stack -->
  <div class="absolute bottom-2.5 left-2.5 z-10 flex flex-col items-start gap-2 select-none">

    <!-- Jet gradient legend (when timeGradient is active) -->
    {#if timeGradient && gradientRange}
      {@const span = gradientRange.maxUtc - gradientRange.minUtc}
      <div class="flex flex-col gap-1">
        <span class="text-[0.45rem] font-medium text-slate-600 dark:text-white/60">
          Session {timeGradient} · time →
        </span>
        <div class="flex items-center gap-1.5">
          <span class="text-[0.42rem] text-slate-500 dark:text-white/40 tabular-nums whitespace-nowrap">
            {fmtGradientTs(gradientRange.minUtc, span)}
          </span>
          <div class="w-32 h-2.5 rounded-full overflow-hidden border border-black/10 dark:border-white/10 shadow-sm"
               style="background:{jetGradientCSS}">
          </div>
          <span class="text-[0.42rem] text-slate-500 dark:text-white/40 tabular-nums whitespace-nowrap">
            {fmtGradientTs(gradientRange.maxUtc, span)}
          </span>
        </div>
      </div>
    {/if}

    <!-- Date color legend (when colorByDate is active) -->
    {#if !timeGradient && colorByDate && dateLegend.length > 0}
      <div class="flex flex-wrap items-center gap-1.5 max-w-[80%]">
        {#each dateLegend as dl}
          <div class="flex items-center gap-1 px-1.5 py-0.5 rounded-full
                      bg-black/5 dark:bg-white/10 backdrop-blur-md
                      border border-black/10 dark:border-white/15
                      text-[0.5rem] text-slate-700 dark:text-white/80 shadow-lg">
            <span class="inline-block w-2 h-2 rounded-full shrink-0" style="background:{dl.hex}"></span>
            <span class="font-mono tabular-nums">{dl.date}</span>
          </div>
        {/each}
      </div>
    {/if}

    <!-- Shape + colour legend (always shown when labeled points exist) -->
    {#if loaded && !error && uniqueLabels.length > 0}
      {@const colAcss = `#${UMAP_COLOR_A.toString(16).padStart(6,"0")}`}
      {@const colBcss = `#${UMAP_COLOR_B.toString(16).padStart(6,"0")}`}
      <div class="flex flex-col gap-1 px-2.5 py-2
                  rounded-lg backdrop-blur-md shadow-lg
                  bg-white/70 dark:bg-black/55
                  border border-black/10 dark:border-white/10
                  text-[0.48rem] text-slate-600 dark:text-white/60">

        <!-- Shape rows — always visible -->
        <div class="flex items-center gap-1.5">
          <span class="w-1.5 h-1.5 rounded-full shrink-0" style="background:{colAcss}"></span>
          <span>{t("umap.sessionA")} — {t("umap.shapeSphere")}</span>
        </div>
        <div class="flex items-center gap-1.5">
          <span class="w-1.5 h-1.5 rotate-45 shrink-0" style="background:{colBcss}"></span>
          <span>{t("umap.sessionB")} — {t("umap.shapeDiamond")}</span>
        </div>

        <!-- Highlight legend — only when a label is selected -->
        {#if selectedLabel}
          <div class="my-0.5 h-px bg-black/10 dark:bg-white/10"></div>
          <div class="flex items-center gap-1.5">
            <span class="w-1.5 h-1.5 rounded-sm shrink-0"
                  style="background:#ffd633"></span>
            <span class="font-semibold truncate max-w-[110px]">{selectedLabel}</span>
            <span class="text-slate-400 dark:text-white/30">{t("umap.legendSelected")}</span>
          </div>
          {#if proximateLabels.length > 0}
            <div class="flex items-center gap-1.5">
              <span class="w-1.5 h-1.5 rounded-full border border-sky-400 dark:border-sky-300 shrink-0"></span>
              <span>{proximateLabels.length} {t("umap.legendNearby")}</span>
            </div>
          {:else}
            <div class="flex items-center gap-1.5 text-slate-400 dark:text-white/25 italic">
              <span class="w-1.5 h-1.5 rounded-full border border-current shrink-0"></span>
              <span>{t("umap.noNearbyLabels")}</span>
            </div>
          {/if}
          <div class="flex items-center gap-1.5 text-slate-400 dark:text-white/30">
            <span class="w-1.5 h-1.5 rounded-full bg-current opacity-20 shrink-0"></span>
            <span>{t("umap.legendDimmed")}</span>
          </div>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Top-right toolbar: time gradient + trace -->
  {#if loaded && !error && curPoints.length >= 2}
    <div class="absolute top-2.5 right-2.5 z-10 flex items-center gap-1.5 select-none">
      <!-- Time gradient buttons -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="flex items-center gap-0 rounded-full overflow-hidden shadow-lg backdrop-blur-md border
                  border-black/10 dark:border-white/10">
        <div class="px-2 py-1 cursor-pointer transition-colors text-[0.55rem] font-medium whitespace-nowrap
                    {timeGradient === 'A'
                      ? 'bg-blue-500/20 dark:bg-blue-500/30 text-blue-700 dark:text-blue-300'
                      : 'bg-white/70 dark:bg-black/50 text-slate-500 dark:text-white/60 hover:text-slate-800 dark:hover:text-white/80'}"
             onclick={() => { timeGradient = timeGradient === 'A' ? null : 'A'; }}>
          🌈 A
        </div>
        <div class="w-px h-4 bg-black/10 dark:bg-white/10"></div>
        <div class="px-2 py-1 cursor-pointer transition-colors text-[0.55rem] font-medium whitespace-nowrap
                    {timeGradient === 'B'
                      ? 'bg-amber-500/20 dark:bg-amber-500/30 text-amber-700 dark:text-amber-300'
                      : 'bg-white/70 dark:bg-black/50 text-slate-500 dark:text-white/60 hover:text-slate-800 dark:hover:text-white/80'}"
             onclick={() => { timeGradient = timeGradient === 'B' ? null : 'B'; }}>
          🌈 B
        </div>
      </div>

      <!-- Trace button -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="flex items-center gap-1.5 px-2.5 py-1 rounded-full cursor-pointer
                  transition-colors shadow-lg backdrop-blur-md border
                  {traceActive
                    ? 'bg-cyan-500/20 dark:bg-cyan-500/30 border-cyan-400/30 dark:border-cyan-400/40 text-cyan-700 dark:text-cyan-200'
                    : 'bg-white/70 dark:bg-black/50 border-black/10 dark:border-white/10 text-slate-500 dark:text-white/60 hover:text-slate-800 dark:hover:text-white/80'}"
           onclick={toggleTrace}>
        <span class="text-[0.55rem] font-medium whitespace-nowrap">
          {traceActive ? t("umap.traceStop") : t("umap.trace")}
        </span>
        {#if traceActive && traceTotal > 0}
          <span class="text-[0.45rem] tabular-nums text-slate-400 dark:text-white/40">{traceProgress}/{traceTotal}</span>
        {/if}
      </div>

      <!-- Labels sidebar toggle (only when there are labeled points) -->
      {#if uniqueLabels.length > 0}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="flex items-center gap-1 px-2.5 py-1 rounded-full cursor-pointer
                    transition-colors shadow-lg backdrop-blur-md border
                    {sidebarOpen
                      ? 'bg-violet-500/20 dark:bg-violet-500/30 border-violet-400/30 dark:border-violet-400/40 text-violet-700 dark:text-violet-200'
                      : 'bg-white/70 dark:bg-black/50 border-black/10 dark:border-white/10 text-slate-500 dark:text-white/60 hover:text-slate-800 dark:hover:text-white/80'}"
             onclick={() => { sidebarOpen = !sidebarOpen; if (!sidebarOpen) { selectedLabel = null; applyHighlight(); } }}>
          <span class="text-[0.55rem] font-medium whitespace-nowrap">
            🏷 {uniqueLabels.length}
          </span>
        </div>
      {/if}

      <!-- Export buttons (PNG + JSON) -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="flex items-center gap-0 rounded-full overflow-hidden shadow-lg backdrop-blur-md
                  border border-black/10 dark:border-white/10">
        <!-- PNG -->
        <div class="flex items-center gap-1 px-2.5 py-1 cursor-pointer transition-colors
                    {exportFlash === 'png'
                      ? 'bg-emerald-500/20 dark:bg-emerald-500/30 text-emerald-700 dark:text-emerald-300'
                      : 'bg-white/70 dark:bg-black/50 text-slate-500 dark:text-white/60 hover:text-slate-800 dark:hover:text-white/80'}"
             title={t("umap.exportPngTitle")}
             onclick={exportPng}>
          {#if exportFlash === "png"}
            <!-- Checkmark flash -->
            <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2"
                 stroke-linecap="round" stroke-linejoin="round" class="w-2.5 h-2.5 shrink-0">
              <polyline points="1.5 6 4.5 9 10.5 3"/>
            </svg>
            <span class="text-[0.55rem] font-medium whitespace-nowrap">{t("umap.exportedPng")}</span>
          {:else}
            <!-- Camera icon -->
            <svg viewBox="0 0 14 12" fill="none" stroke="currentColor" stroke-width="1.4"
                 stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3 shrink-0">
              <path d="M5 1H9L10.5 3H13C13.6 3 14 3.4 14 4V11C14 11.6 13.6 12 13 12H1C0.4 12 0 11.6 0 11V4C0 3.4 0.4 3 1 3H3.5L5 1Z"/>
              <circle cx="7" cy="7.5" r="2.5"/>
            </svg>
            <span class="text-[0.55rem] font-medium whitespace-nowrap">{t("umap.exportPng")}</span>
          {/if}
        </div>
        <!-- Divider -->
        <div class="w-px h-4 bg-black/10 dark:bg-white/10 shrink-0"></div>
        <!-- JSON -->
        <div class="flex items-center gap-1 px-2.5 py-1 cursor-pointer transition-colors
                    {exportFlash === 'json'
                      ? 'bg-emerald-500/20 dark:bg-emerald-500/30 text-emerald-700 dark:text-emerald-300'
                      : 'bg-white/70 dark:bg-black/50 text-slate-500 dark:text-white/60 hover:text-slate-800 dark:hover:text-white/80'}"
             title={t("umap.exportJsonTitle")}
             onclick={exportJson}>
          {#if exportFlash === "json"}
            <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2"
                 stroke-linecap="round" stroke-linejoin="round" class="w-2.5 h-2.5 shrink-0">
              <polyline points="1.5 6 4.5 9 10.5 3"/>
            </svg>
            <span class="text-[0.55rem] font-medium whitespace-nowrap">{t("umap.exportedJson")}</span>
          {:else}
            <!-- Download icon -->
            <svg viewBox="0 0 12 14" fill="none" stroke="currentColor" stroke-width="1.4"
                 stroke-linecap="round" stroke-linejoin="round" class="w-2.5 h-3 shrink-0">
              <path d="M6 1v8M3 6l3 3 3-3"/>
              <path d="M1 11h10v2H1z"/>
            </svg>
            <span class="text-[0.55rem] font-medium whitespace-nowrap">{t("umap.exportJson")}</span>
          {/if}
        </div>
      </div>
    </div>
  {/if}

  <!-- Node size slider -->
  {#if loaded && !error}
    <div class="absolute bottom-2.5 right-2.5 z-10 flex items-center gap-1.5
                bg-white/70 dark:bg-black/50 backdrop-blur-md
                border border-black/10 dark:border-white/10 rounded-full
                px-2.5 py-1 select-none shadow-lg">
      <span class="text-[0.5rem] text-slate-500 dark:text-white/50 whitespace-nowrap">{t("umap.nodeSize")}</span>
      <input type="range"
             min={UMAP_SCALE_MIN} max={UMAP_SCALE_MAX} step="0.05"
             bind:value={pointScale}
              class="w-16 h-1 accent-violet-500 dark:accent-violet-400 cursor-pointer" />
      <span class="text-[0.5rem] text-slate-400 dark:text-white/40 tabular-nums w-6 text-right">{pointScale.toFixed(1)}×</span>
    </div>
  {/if}

  <!-- Hover tooltip -->
  {#if tooltip}
    <div class="absolute pointer-events-none z-10 px-2.5 py-1.5 rounded-lg
                bg-white/90 dark:bg-black/85 text-slate-800 dark:text-white
                text-[0.6rem] leading-snug
                whitespace-pre-wrap max-w-[220px] shadow-xl backdrop-blur-sm
                border border-black/10 dark:border-white/10"
         style="left:{tooltip.x + 12}px; top:{tooltip.y - 8}px; transform:translateY(-100%);">
      {tooltip.text}
    </div>
  {/if}
</div><!-- end 3D canvas -->
</div><!-- end flex row (sidebar + canvas) -->
</div><!-- end flex-col outer -->

<!-- no component-scoped styles — all styling via Tailwind utilities -->
