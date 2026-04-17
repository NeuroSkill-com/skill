<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Interactive Cross-Modal Search — 3D Graph Viewer

  Renders the 4-layer graph returned by `interactive_search`:
    Layer 0  query        (violet)  — center
    Layer 1  text_label   (blue)    — semantically similar labels
    Layer 2  eeg_point    (amber)   — raw EEG neighbors of the label windows
    Layer 3  found_label  (emerald) — labels found near EEG neighbor timestamps

  Click a node to highlight its direct connections; click again / click empty
  space to deselect.  Unconnected nodes and edges are dimmed to 10 % opacity.
-->
<script lang="ts">
import { onDestroy, onMount } from "svelte";
import type * as THREE_NS from "three";
import type { OrbitControls as OrbitControlsType } from "three/examples/jsm/controls/OrbitControls.js";
import { fmtDateTimeLocale } from "$lib/format";
import { t } from "$lib/i18n/index.svelte";
import { getResolved } from "$lib/stores/theme.svelte";

// ── Types ─────────────────────────────────────────────────────────────────
interface GraphNode {
  id: string;
  kind: "query" | "text_label" | "eeg_point" | "found_label" | "screenshot";
  text?: string;
  timestamp_unix?: number;
  distance: number;
  eeg_metrics?: Record<string, number | null> | null;
  parent_id?: string;
  /** 2-D PCA projection – both axes in [-1, 1].  Present on found_label nodes
   *  when the backend successfully embedded the label text.  Similar labels
   *  share nearby (proj_x, proj_y) values. */
  proj_x?: number;
  proj_y?: number;
  /** Screenshot image URL — only present on kind === "screenshot" nodes. */
  screenshot_url?: string;
  /** Session identifier for grouping. */
  session_id?: string;
  /** Composite relevance score (0 = best). */
  relevance_score?: number;
}
interface GraphEdge {
  from_id: string;
  to_id: string;
  distance: number;
  kind: "text_sim" | "eeg_bridge" | "eeg_sim" | "label_prox" | "screenshot_link";
}

type ThreeModule = typeof import("three");
type NodeMesh = THREE_NS.Mesh<THREE_NS.SphereGeometry, THREE_NS.MeshPhongMaterial>;
type NodeSprite = THREE_NS.Sprite;
type EdgeLine = THREE_NS.Line<THREE_NS.BufferGeometry, THREE_NS.LineBasicMaterial>;

// Internal scene-object records ──────────────────────────────────────────
interface NodeEntry {
  mesh: NodeMesh;
  sprite: NodeSprite | null;
  node: GraphNode;
  baseColor: number; // original hex color
  baseEmissive: number; // original emissiveIntensity
}
interface EdgeEntry {
  line: EdgeLine;
  fromId: string;
  toId: string;
  baseOpacity: number; // opacity at rest
}

let { nodes, edges, usePca = true, onselect, hiddenKinds = [], colorMode = "timestamp" }: {
  nodes: GraphNode[];
  edges: GraphEdge[];
  usePca?: boolean;
  onselect?: (node: GraphNode | null) => void;
  hiddenKinds?: GraphNode["kind"][];
  colorMode?: "timestamp" | "engagement" | "snr" | "session";
} = $props();

// ── Visual constants ─────────────────────────────────────────────────────
const KIND_COLOR: Record<GraphNode["kind"], number> = {
  query: 0x8b5cf6,
  text_label: 0x3b82f6,
  eeg_point: 0xf59e0b,
  found_label: 0x10b981,
  screenshot: 0x06b6d4,
};
const KIND_RADIUS: Record<GraphNode["kind"], number> = {
  query: 1.2,
  text_label: 0.8,
  eeg_point: 0.55,
  found_label: 0.65,
  screenshot: 0.45,
};
const EDGE_COLOR: Record<GraphEdge["kind"], number> = {
  text_sim: 0x8b5cf6,
  eeg_bridge: 0xf59e0b,
  eeg_sim: 0xf59e0b,
  label_prox: 0x10b981,
  screenshot_link: 0x06b6d4,
};
const BASE_EMISSIVE: Record<GraphNode["kind"], number> = {
  query: 0.3,
  text_label: 0.18,
  eeg_point: 0.35,
  found_label: 0.18,
  screenshot: 0.25,
};

const LAYER_RADIUS = { query: 0, text_label: 6, eeg_point: 5, found_label: 4.5, screenshot: 2.5 };
const GOLDEN = Math.PI * (3 - Math.sqrt(5));
const BG_DARK = 0x13131f;
const BG_LIGHT = 0xf1f5f9;

// Highlight visual constants
const DIM_OPACITY = 0.08;
const DIM_EMISSIVE = 0.01;
const SEL_EMISSIVE = 0.85; // selected node glow
const NEIGHBOR_EMISSIVE_MULT = 1.6;
const EDGE_BRIGHT_MULT = 2.8;
const DIM_EDGE_OPACITY = 0.03;

// ── Jet / turbo colormap ──────────────────────────────────────────────────
function turbo(t: number): [number, number, number] {
  const c = Math.max(0, Math.min(1, t));
  const r = Math.max(
    0,
    Math.min(
      1,
      0.13572138 + c * (4.6153926 + c * (-42.66032258 + c * (132.13108234 + c * (-152.54893924 + c * 59.28637943)))),
    ),
  );
  const g = Math.max(
    0,
    Math.min(
      1,
      0.09140261 + c * (2.19418839 + c * (4.84296658 + c * (-14.18503333 + c * (4.27729857 + c * 2.82956604)))),
    ),
  );
  const b = Math.max(
    0,
    Math.min(
      1,
      0.1066733 + c * (12.64194608 + c * (-60.58204836 + c * (110.36276771 + c * (-89.90310912 + c * 27.34824973)))),
    ),
  );
  return [r, g, b];
}
function turboHex(t: number): number {
  const [r, g, b] = turbo(t);
  return (Math.round(r * 255) << 16) | (Math.round(g * 255) << 8) | Math.round(b * 255);
}
function turboCss(t: number): string {
  const [r, g, b] = turbo(t);
  const h = (v: number) =>
    Math.round(v * 255)
      .toString(16)
      .padStart(2, "0");
  return `#${h(r)}${h(g)}${h(b)}`;
}

// ── State ─────────────────────────────────────────────────────────────────
let container = $state<HTMLDivElement | undefined>();
let tooltip = $state<{ x: number; y: number; lines: string[] } | null>(null);
let loaded = $state(false);
let isDark = $derived(getResolved() === "dark");
let selectedNodeId = $state<string | null>(null);
let hoveredNodeId = $state<string | null>(null); // for cursor style only

let eegTimeMin = $state(0);
let eegTimeMax = $state(0);
let eegGradientCss = $derived.by(() => {
  const stops = Array.from({ length: 10 }, (_, i) => turboCss(i / 9));
  return `linear-gradient(to right, ${stops.join(", ")})`;
});

// ── Three.js refs ─────────────────────────────────────────────────────────
let THREE!: ThreeModule;
let scene!: THREE_NS.Scene;
let camera!: THREE_NS.PerspectiveCamera;
let renderer!: THREE_NS.WebGLRenderer;
let controls!: OrbitControlsType;
let animId = 0;
let resizeObs: ResizeObserver | null = null;
let raycaster!: THREE_NS.Raycaster;
let mouse!: THREE_NS.Vector2;
let canvasClickHandler: ((e: MouseEvent) => void) | null = null;

// Richer scene-object records
let nodeEntries: NodeEntry[] = [];
let edgeEntries: EdgeEntry[] = [];

// ── Layout helpers ────────────────────────────────────────────────────────
function fibSphere(i: number, n: number): [number, number, number] {
  const y = 1 - (i / Math.max(n - 1, 1)) * 2;
  const r = Math.sqrt(Math.max(0, 1 - y * y));
  const θ = GOLDEN * i;
  return [Math.cos(θ) * r, y, Math.sin(θ) * r];
}
function add3(a: [number, number, number], b: [number, number, number]): [number, number, number] {
  return [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
}
function scale3(v: [number, number, number], s: number): [number, number, number] {
  return [v[0] * s, v[1] * s, v[2] * s];
}
function normalize3(v: [number, number, number]): [number, number, number] {
  const len = Math.sqrt(v[0] ** 2 + v[1] ** 2 + v[2] ** 2) || 1;
  return [v[0] / len, v[1] / len, v[2] / len];
}

function computePositions(ns: GraphNode[], usePcaLayout: boolean): Map<string, [number, number, number]> {
  const pos = new Map<string, [number, number, number]>();
  // Place query node at center — match by kind since ID may vary (e.g. "q0")
  const queryNode = ns.find(n => n.kind === "query");
  if (queryNode) pos.set(queryNode.id, [0, 0, 0]);
  pos.set("query", [0, 0, 0]); // fallback

  const textLabels = ns.filter((n) => n.kind === "text_label");
  for (let i = 0; i < textLabels.length; i++) {
    pos.set(textLabels[i].id, scale3(fibSphere(i, textLabels.length), LAYER_RADIUS.text_label));
  }

  const eegMap = new Map<string, GraphNode[]>();
  for (const n of ns) {
    if (n.kind !== "eeg_point") continue;
    const pid = n.parent_id ?? "query";
    if (!eegMap.has(pid)) eegMap.set(pid, []);
    eegMap.get(pid)?.push(n);
  }
  for (const [pid, children] of eegMap) {
    const parentPos = pos.get(pid) ?? [0, 0, 0];
    const outDir = normalize3(parentPos[0] === 0 && parentPos[1] === 0 && parentPos[2] === 0 ? [1, 0, 0] : parentPos);
    for (let j = 0; j < children.length; j++) {
      const local = fibSphere(j, Math.max(children.length, 3));
      pos.set(
        children[j].id,
        add3(parentPos, scale3(normalize3(add3(scale3(outDir, 1.5), local)), LAYER_RADIUS.eeg_point)),
      );
    }
  }

  // ── Found labels ──────────────────────────────────────────────────────
  // When PCA layout is enabled (usePcaLayout) AND the backend has computed
  // proj_x / proj_y for the found_labels, place them on an outer sphere shell
  // keyed by embedding azimuth / elevation.  Semantically similar labels
  // cluster together.  Toggle off to restore parent-relative layout.
  const allFoundLabels = ns.filter((n) => n.kind === "found_label");
  const hasProjData = usePcaLayout && allFoundLabels.some((n) => n.proj_x !== undefined);

  if (hasProjData) {
    // Radius slightly outside the EEG + text layers so found_labels occupy
    // the outermost shell and don't collide with the inner layers.
    const FOUND_PCA_R = 9.5;
    for (const n of allFoundLabels) {
      const px = n.proj_x ?? 0;
      const py = n.proj_y ?? 0;
      // proj_x drives azimuth (rotation around Y), proj_y drives elevation.
      // Clamp elevation to ±80° so nodes never pile up exactly at the poles.
      const phi = px * Math.PI; // azimuth: -π … π
      const theta = py * (Math.PI / 2.4); // elevation: ≈ ±75°
      const cosT = Math.cos(theta);
      pos.set(n.id, [
        FOUND_PCA_R * cosT * Math.cos(phi),
        FOUND_PCA_R * Math.sin(theta),
        FOUND_PCA_R * cosT * Math.sin(phi),
      ]);
    }
  } else {
    // Fallback: cluster each found_label near its EEG-point parent.
    const flMap = new Map<string, GraphNode[]>();
    for (const n of allFoundLabels) {
      const pid = n.parent_id ?? "query";
      if (!flMap.has(pid)) flMap.set(pid, []);
      flMap.get(pid)?.push(n);
    }
    for (const [pid, children] of flMap) {
      const parentPos = pos.get(pid) ?? [0, 0, 0];
      const outDir = normalize3(parentPos[0] === 0 && parentPos[1] === 0 && parentPos[2] === 0 ? [0, 1, 0] : parentPos);
      for (let j = 0; j < children.length; j++) {
        const local = fibSphere(j, Math.max(children.length, 3));
        pos.set(
          children[j].id,
          add3(parentPos, scale3(normalize3(add3(scale3(outDir, 1.2), local)), LAYER_RADIUS.found_label)),
        );
      }
    }
  }

  // ── Screenshot nodes — clustered near their parent EEG-point ────────────
  const ssMap = new Map<string, GraphNode[]>();
  for (const n of ns) {
    if (n.kind !== "screenshot") continue;
    const pid = n.parent_id ?? "query";
    if (!ssMap.has(pid)) ssMap.set(pid, []);
    ssMap.get(pid)?.push(n);
  }
  for (const [pid, children] of ssMap) {
    const parentPos = pos.get(pid) ?? [0, 0, 0];
    const outDir = normalize3(parentPos[0] === 0 && parentPos[1] === 0 && parentPos[2] === 0 ? [0, -1, 0] : parentPos);
    for (let j = 0; j < children.length; j++) {
      const local = fibSphere(j, Math.max(children.length, 3));
      pos.set(
        children[j].id,
        add3(parentPos, scale3(normalize3(add3(scale3(outDir, 0.8), local)), LAYER_RADIUS.screenshot)),
      );
    }
  }

  return pos;
}

// ── Scene init ────────────────────────────────────────────────────────────
async function initScene() {
  THREE = await import("three");
  const controlsMod = (await import("three/addons/controls/OrbitControls.js")) as {
    OrbitControls: new (object: THREE_NS.Camera, domElement?: HTMLElement) => OrbitControlsType;
  };
  const OrbitControls = controlsMod.OrbitControls;
  if (!container) return;

  const w = container.clientWidth,
    h = container.clientHeight;
  renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setPixelRatio(window.devicePixelRatio);
  renderer.setSize(w, h);
  container.appendChild(renderer.domElement);

  scene = new THREE.Scene();
  camera = new THREE.PerspectiveCamera(55, w / h, 0.1, 500);
  camera.position.set(0, 8, 28);

  scene.add(new THREE.AmbientLight(0xffffff, 0.6));
  const dir = new THREE.DirectionalLight(0xffffff, 0.9);
  dir.position.set(10, 20, 10);
  scene.add(dir);

  controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.07;
  controls.autoRotate = true;
  controls.autoRotateSpeed = 0.5;

  raycaster = new THREE.Raycaster();
  raycaster.params.Points = { threshold: 0.5 };
  mouse = new THREE.Vector2();

  scene.background = new THREE.Color(isDark ? BG_DARK : BG_LIGHT);

  // Subtle grid pattern
  const gridColor = isDark ? 0x1a1a28 : 0xe8ecf0;
  const grid = new THREE.GridHelper(60, 20, gridColor, gridColor);
  grid.position.y = -15;
  grid.material.opacity = isDark ? 0.08 : 0.1;
  grid.material.transparent = true;
  scene.add(grid);

  buildGraph();
  loaded = true;
  animate();

  // Attach click directly to the canvas so OrbitControls pointer-event
  // handling doesn't interfere with event bubbling to the container div.
  // Track pointer-down position; only treat as a click if the pointer
  // moved less than 5 px (i.e. not a drag/orbit gesture).
  let downX = 0,
    downY = 0;
  renderer.domElement.addEventListener(
    "pointerdown",
    (e: PointerEvent) => {
      downX = e.clientX;
      downY = e.clientY;
    },
    { passive: true },
  );
  canvasClickHandler = (e: MouseEvent) => {
    const dx = e.clientX - downX,
      dy = e.clientY - downY;
    if (dx * dx + dy * dy > 25) return; // was a drag, ignore
    onClick(e);
  };
  renderer.domElement.addEventListener("click", canvasClickHandler);
  renderer.domElement.addEventListener("dblclick", onDblClick);

  resizeObs = new ResizeObserver(() => {
    if (!container) return;
    const w2 = container.clientWidth,
      h2 = container.clientHeight;
    camera.aspect = w2 / h2;
    camera.updateProjectionMatrix();
    renderer.setSize(w2, h2);
  });
  resizeObs.observe(container);
}

function buildGraph() {
  if (!THREE || !scene) return;

  // Clear previous
  for (const ne of nodeEntries) {
    ne.mesh.geometry.dispose();
    ne.mesh.material.dispose();
    scene.remove(ne.mesh);
    if (ne.sprite) {
      ne.sprite.material.map?.dispose();
      ne.sprite.material.dispose();
      scene.remove(ne.sprite);
    }
  }
  for (const ee of edgeEntries) {
    ee.line.geometry.dispose();
    ee.line.material.dispose();
    scene.remove(ee.line);
  }
  nodeEntries = [];
  edgeEntries = [];
  selectedNodeId = null;

  // Apply node kind filter
  const hiddenSet = new Set(hiddenKinds);
  const visibleNodes = hiddenSet.size > 0 ? nodes.filter(n => !hiddenSet.has(n.kind)) : nodes;
  const visibleIds = new Set(visibleNodes.map(n => n.id));
  const visibleEdges = hiddenSet.size > 0 ? edges.filter(e => visibleIds.has(e.from_id) && visibleIds.has(e.to_id)) : edges;

  const positions = computePositions(visibleNodes, usePca);

  // EEG time range
  const eegTs = visibleNodes
    .filter((n) => n.kind === "eeg_point" && n.timestamp_unix != null)
    .map((n) => n.timestamp_unix as number);
  const tMin = eegTs.length ? Math.min(...eegTs) : 0;
  const tMax = eegTs.length ? Math.max(...eegTs) : 1;
  eegTimeMin = tMin;
  eegTimeMax = tMax;
  const tRange = tMax - tMin || 1;

  function eegColor(ts: number | undefined): number {
    if (ts == null || eegTs.length === 0) return KIND_COLOR.eeg_point;
    return turboHex((ts - tMin) / tRange);
  }

  // Max distances per edge kind for normalisation
  const maxDist = new Map<string, number>();
  for (const e of visibleEdges) {
    const cur = maxDist.get(e.kind) ?? 0;
    if (e.distance > cur) maxDist.set(e.kind, e.distance);
  }

  // ── Edges ────────────────────────────────────────────────────────────
  for (const edge of visibleEdges) {
    const fromPos = positions.get(edge.from_id);
    const toPos = positions.get(edge.to_id);
    if (!fromPos || !toPos) continue;

    const mx = maxDist.get(edge.kind) || 1;
    const norm = edge.distance / mx;
    const opa = Math.max(0.08, 1 - norm * 0.8);

    let edgeCol = EDGE_COLOR[edge.kind as keyof typeof EDGE_COLOR] ?? 0x888888;
    if (edge.kind === "eeg_bridge") {
      const toNode = visibleNodes.find((n) => n.id === edge.to_id);
      if (toNode?.kind === "eeg_point") edgeCol = eegColor(toNode.timestamp_unix);
    }

    const geo = new THREE.BufferGeometry();
    geo.setFromPoints([
      new THREE.Vector3(fromPos[0], fromPos[1], fromPos[2]),
      new THREE.Vector3(toPos[0], toPos[1], toPos[2]),
    ]);
    const mat = new THREE.LineBasicMaterial({ color: edgeCol, transparent: true, opacity: opa, linewidth: 1 });
    const line = new THREE.Line(geo, mat);
    scene.add(line);
    edgeEntries.push({ line, fromId: edge.from_id, toId: edge.to_id, baseOpacity: opa });
  }

  // ── Nodes ────────────────────────────────────────────────────────────
  for (const node of visibleNodes) {
    const pos = positions.get(node.id);
    if (!pos) continue;

    // Scale EEG nodes by relevance (lower = bigger = better), subtle range
    let radius = KIND_RADIUS[node.kind];
    if (node.kind === "eeg_point" && node.relevance_score != null) {
      const scale = 1.0 + (1.0 - Math.min(1, node.relevance_score)) * 0.3; // 1.0–1.3x
      radius *= scale;
    }
    // Color based on selected mode
    let color: number;
    if (node.kind === "eeg_point") {
      if (colorMode === "engagement" && node.eeg_metrics?.engagement != null) {
        color = turboHex(Math.min(1, node.eeg_metrics.engagement as number));
      } else if (colorMode === "snr" && node.eeg_metrics?.snr != null) {
        color = turboHex(Math.min(1, (node.eeg_metrics.snr as number) / 20));
      } else if (colorMode === "session" && node.session_id) {
        // Hash session_id to a hue
        let h = 0;
        for (let ci = 0; ci < node.session_id.length; ci++) h = (h * 31 + node.session_id.charCodeAt(ci)) & 0xffffff;
        color = h;
      } else {
        color = eegColor(node.timestamp_unix);
      }
    } else {
      color = KIND_COLOR[node.kind];
    }
    const emissive = BASE_EMISSIVE[node.kind];

    const geo = new THREE.SphereGeometry(radius, 24, 16);
    const mat = new THREE.MeshPhongMaterial({
      color,
      shininess: 90,
      emissive: color,
      emissiveIntensity: emissive,
      transparent: true, // required so we can dim with opacity
      opacity: 1.0,
    });
    const mesh = new THREE.Mesh(geo, mat);
    mesh.position.set(pos[0], pos[1], pos[2]);
    scene.add(mesh);

    // Label sprite — text labels for most kinds, thumbnail for screenshots
    let sprite: NodeSprite | null = null;
    if (node.kind === "screenshot" && node.screenshot_url) {
      sprite = makeScreenshotSprite(node.screenshot_url, node.text ?? "");
      if (sprite) {
        sprite.position.set(pos[0], pos[1] + radius + 2.0, pos[2]);
        scene.add(sprite);
      }
    } else if (node.kind === "query" || node.kind === "text_label" || node.kind === "found_label") {
      sprite = makeTextSprite(node.text ?? "", color, node.kind);
      if (sprite) {
        sprite.position.set(pos[0], pos[1] + radius + 1.2, pos[2]);
        scene.add(sprite);
      }
    }

    nodeEntries.push({ mesh, sprite, node, baseColor: color, baseEmissive: emissive });
  }
}

// ── Text sprite ───────────────────────────────────────────────────────────
function makeTextSprite(text: string, hexColor: number, kind: GraphNode["kind"]): NodeSprite | null {
  if (!text || !THREE) return null;
  const W = 1024,
    H = 128;
  const canvas = document.createElement("canvas");
  canvas.width = W;
  canvas.height = H;
  const ctx = canvas.getContext("2d") as CanvasRenderingContext2D;

  const r = (hexColor >> 16) & 0xff;
  const g = (hexColor >> 8) & 0xff;
  const b = hexColor & 0xff;

  ctx.fillStyle = `rgba(${r},${g},${b},0.12)`;
  ctx.beginPath();
  ctx.roundRect(8, 8, W - 16, H - 16, 24);
  ctx.fill();
  ctx.strokeStyle = `rgba(${r},${g},${b},0.55)`;
  ctx.lineWidth = 3;
  ctx.beginPath();
  ctx.roundRect(8, 8, W - 16, H - 16, 24);
  ctx.stroke();

  const fontSize = kind === "query" ? 52 : kind === "text_label" ? 44 : 38;
  ctx.font = `bold ${fontSize}px system-ui, sans-serif`;
  ctx.fillStyle = `rgba(${r},${g},${b},1.0)`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  const maxW = W - 40;
  let label = text;
  while (ctx.measureText(label).width > maxW && label.length > 4) label = `${label.slice(0, -4)}…`;
  ctx.fillText(label, W / 2, H / 2);

  const tex = new THREE.CanvasTexture(canvas);
  tex.needsUpdate = true;
  const mat = new THREE.SpriteMaterial({ map: tex, transparent: true, opacity: 1.0, depthTest: false });
  const spr = new THREE.Sprite(mat);
  const sW = kind === "query" ? 9 : kind === "text_label" ? 7.5 : 6;
  spr.scale.set(sW, sW * (H / W), 1);
  return spr;
}

// ── Screenshot thumbnail sprite ─────────────────────────────────────────
function makeScreenshotSprite(url: string, label: string): NodeSprite | null {
  if (!THREE) return null;
  const W = 512,
    H = 384;
  // Start with a placeholder frame; the image loads asynchronously.
  const canvas = document.createElement("canvas");
  canvas.width = W;
  canvas.height = H;
  const ctx = canvas.getContext("2d") as CanvasRenderingContext2D;

  // Rounded frame background
  ctx.fillStyle = "rgba(6,182,212,0.08)";
  ctx.beginPath();
  ctx.roundRect(0, 0, W, H, 16);
  ctx.fill();
  ctx.strokeStyle = "rgba(6,182,212,0.55)";
  ctx.lineWidth = 4;
  ctx.beginPath();
  ctx.roundRect(2, 2, W - 4, H - 4, 16);
  ctx.stroke();
  // Label at bottom
  if (label) {
    ctx.font = "bold 22px system-ui, sans-serif";
    ctx.fillStyle = "rgba(6,182,212,0.85)";
    ctx.textAlign = "center";
    ctx.textBaseline = "bottom";
    let lbl = label;
    while (ctx.measureText(lbl).width > W - 30 && lbl.length > 4) lbl = `${lbl.slice(0, -4)}…`;
    ctx.fillText(lbl, W / 2, H - 12);
  }

  const tex = new THREE.CanvasTexture(canvas);
  tex.needsUpdate = true;
  const mat = new THREE.SpriteMaterial({ map: tex, transparent: true, opacity: 1.0, depthTest: false });
  const spr = new THREE.Sprite(mat);
  const sW = 5.5;
  spr.scale.set(sW, sW * (H / W), 1);

  // Load actual screenshot image asynchronously and repaint the canvas
  const img = new Image();
  img.crossOrigin = "anonymous";
  img.onload = () => {
    const PAD = 12;
    const LABEL_H = label ? 32 : 0;
    const imgW = W - PAD * 2;
    const imgH = H - PAD * 2 - LABEL_H;

    // Clear and redraw frame
    ctx.clearRect(0, 0, W, H);
    ctx.fillStyle = "rgba(6,182,212,0.08)";
    ctx.beginPath();
    ctx.roundRect(0, 0, W, H, 16);
    ctx.fill();
    ctx.strokeStyle = "rgba(6,182,212,0.55)";
    ctx.lineWidth = 4;
    ctx.beginPath();
    ctx.roundRect(2, 2, W - 4, H - 4, 16);
    ctx.stroke();

    // Clip inner area and draw the image
    ctx.save();
    ctx.beginPath();
    ctx.roundRect(PAD, PAD, imgW, imgH, 8);
    ctx.clip();
    ctx.drawImage(img, PAD, PAD, imgW, imgH);
    ctx.restore();

    // Re-draw label
    if (label) {
      ctx.font = "bold 22px system-ui, sans-serif";
      ctx.fillStyle = "rgba(6,182,212,0.85)";
      ctx.textAlign = "center";
      ctx.textBaseline = "bottom";
      let lbl = label;
      while (ctx.measureText(lbl).width > W - 30 && lbl.length > 4) lbl = `${lbl.slice(0, -4)}…`;
      ctx.fillText(lbl, W / 2, H - 12);
    }

    tex.needsUpdate = true;
  };
  img.onerror = () => {
    // On failure, draw a fallback "no image" indicator
    ctx.font = "bold 28px system-ui, sans-serif";
    ctx.fillStyle = "rgba(6,182,212,0.35)";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText("(no image)", W / 2, H / 2);
    tex.needsUpdate = true;
  };
  img.src = url;

  return spr;
}

// ── Selection / highlight ─────────────────────────────────────────────────
function applySelection(nodeId: string | null) {
  selectedNodeId = nodeId;

  if (nodeId === null) {
    // Reset everything
    for (const ne of nodeEntries) {
      ne.mesh.material.opacity = 1.0;
      ne.mesh.material.emissiveIntensity = ne.baseEmissive;
      if (ne.sprite) ne.sprite.material.opacity = 1.0;
    }
    for (const ee of edgeEntries) {
      ee.line.material.opacity = ee.baseOpacity;
    }
    if (controls) controls.autoRotate = true;
    return;
  }

  // Build sets of connected nodes and edge indices
  const connectedNodeIds = new Set<string>([nodeId]);
  const connectedEdgeIdx = new Set<number>();
  for (let i = 0; i < edgeEntries.length; i++) {
    const ee = edgeEntries[i];
    if (ee.fromId === nodeId || ee.toId === nodeId) {
      connectedEdgeIdx.add(i);
      connectedNodeIds.add(ee.fromId);
      connectedNodeIds.add(ee.toId);
    }
  }

  // Apply to nodes
  for (const ne of nodeEntries) {
    const isSelected = ne.node.id === nodeId;
    const isNeighbor = !isSelected && connectedNodeIds.has(ne.node.id);
    const isDimmed = !connectedNodeIds.has(ne.node.id);

    if (isSelected) {
      ne.mesh.material.opacity = 1.0;
      ne.mesh.material.emissiveIntensity = SEL_EMISSIVE;
    } else if (isNeighbor) {
      ne.mesh.material.opacity = 1.0;
      ne.mesh.material.emissiveIntensity = Math.min(0.9, ne.baseEmissive * NEIGHBOR_EMISSIVE_MULT);
    } else {
      ne.mesh.material.opacity = DIM_OPACITY;
      ne.mesh.material.emissiveIntensity = DIM_EMISSIVE;
    }

    if (ne.sprite) ne.sprite.material.opacity = isDimmed ? DIM_OPACITY * 0.8 : 1.0;
  }

  // Apply to edges
  for (let i = 0; i < edgeEntries.length; i++) {
    const ee = edgeEntries[i];
    if (connectedEdgeIdx.has(i)) {
      ee.line.material.opacity = Math.min(1.0, ee.baseOpacity * EDGE_BRIGHT_MULT);
    } else {
      ee.line.material.opacity = DIM_EDGE_OPACITY;
    }
  }

  if (controls) controls.autoRotate = false;
}

// ── Raycasting helper ─────────────────────────────────────────────────────
function getHitNode(e: MouseEvent): NodeEntry | null {
  if (!renderer || !container || !raycaster || !mouse) return null;
  const rect = renderer.domElement.getBoundingClientRect();
  mouse.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
  mouse.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;
  raycaster.setFromCamera(mouse, camera);
  const hits = raycaster.intersectObjects(nodeEntries.map((ne) => ne.mesh));
  if (!hits.length) return null;
  return nodeEntries.find((ne) => ne.mesh === hits[0].object) ?? null;
}

// ── Mouse move (hover tooltip + cursor) ───────────────────────────────────
function onMouseMove(e: MouseEvent) {
  const hit = getHitNode(e);
  hoveredNodeId = hit?.node.id ?? null;

  if (hit) {
    const n = hit.node;
    const kindLabel: Record<string, string> = {
      query: "Query",
      text_label: "Text label",
      eeg_point: "EEG point",
      found_label: usePca && n.proj_x !== undefined ? "Found label  (PCA-clustered)" : "Found label",
      screenshot: "Screenshot",
    };
    const lines: string[] = [`${kindLabel[n.kind] ?? n.kind}`];
    if (n.text) lines.push(n.text.slice(0, 80));
    if (n.timestamp_unix) lines.push(fmtDateTimeLocale(n.timestamp_unix));
    if (n.distance > 0) lines.push(`dist: ${n.distance.toFixed(4)}`);
    if (n.relevance_score != null) lines.push(`relevance: ${n.relevance_score.toFixed(3)}`);
    if (n.session_id) lines.push(`session: ${n.session_id}`);
    // Show EEG metrics summary in tooltip
    if (n.eeg_metrics) {
      const m = n.eeg_metrics;
      const parts: string[] = [];
      if (m.engagement != null) parts.push(`eng:${(m.engagement as number).toFixed(2)}`);
      if (m.relaxation != null) parts.push(`rel:${(m.relaxation as number).toFixed(2)}`);
      if (m.snr != null) parts.push(`snr:${(m.snr as number).toFixed(1)}`);
      if (m.hr != null && (m.hr as number) > 0) parts.push(`hr:${(m.hr as number).toFixed(0)}`);
      if (parts.length) lines.push(parts.join("  "));
    }
    // Show connected edge kinds when a node is selected
    if (selectedNodeId === n.id) {
      const edgeKinds = new Set(edgeEntries
        .filter(ee => ee.fromId === n.id || ee.toId === n.id)
        .map(ee => ee.line.material.color ? edgeEntries.find(e => e === ee)?.fromId === n.id ? `→ ${edges.find(ed => ed.from_id === ee.fromId && ed.to_id === ee.toId)?.kind ?? ""}` : `← ${edges.find(ed => ed.from_id === ee.fromId && ed.to_id === ee.toId)?.kind ?? ""}` : ""));
      if (edgeKinds.size > 0) lines.push([...edgeKinds].filter(Boolean).join(", "));
    }
    if (selectedNodeId === null) lines.push("click to highlight connections");
    else if (selectedNodeId === n.id) lines.push("click to deselect");
    tooltip = { x: e.clientX, y: e.clientY - 10, lines };
  } else {
    tooltip = null;
  }
}

// ── Click (select / deselect) ─────────────────────────────────────────────
function onClick(e: MouseEvent) {
  const hit = getHitNode(e);
  if (!hit) {
    // Click on empty space → deselect + reset camera
    if (selectedNodeId !== null) { applySelection(null); onselect?.(null); }
    resetCamera();
    return;
  }
  // Click same node → deselect; click different node → select it
  const newSel = hit.node.id === selectedNodeId ? null : hit.node.id;
  applySelection(newSel);
  onselect?.(newSel ? hit.node : null);
}

// ── Reset camera to default position ─────────────────────────────────────
function resetCamera() {
  flyTarget = {
    x: 0, y: 8, z: 28,
    tx: 0, ty: 0, tz: 0,
    t: 0,
  };
  if (controls) controls.autoRotate = true;
}

// ── Fly camera to a node (smooth tween) ──────────────────────────────────
let flyTarget: { x: number; y: number; z: number; tx: number; ty: number; tz: number; t: number } | null = null;

function flyToNode(node: GraphNode) {
  const ne = nodeEntries.find(n => n.node.id === node.id);
  if (!ne || !camera || !controls) return;
  const p = ne.mesh.position;
  const radius = KIND_RADIUS[node.kind] ?? 0.5;
  // Target: position the camera at a comfortable distance from the node
  const dir = camera.position.clone().sub(p).normalize();
  const dist = radius * 8 + 4;
  flyTarget = {
    x: p.x + dir.x * dist,
    y: p.y + dir.y * dist + 2,
    z: p.z + dir.z * dist,
    tx: p.x, ty: p.y, tz: p.z,
    t: 0,
  };
  if (controls) controls.autoRotate = false;
}

function onDblClick(e: MouseEvent) {
  const hit = getHitNode(e);
  if (hit) {
    flyToNode(hit.node);
    applySelection(hit.node.id);
    onselect?.(hit.node);
  }
}

// ── Animation loop ────────────────────────────────────────────────────────
function animate() {
  animId = requestAnimationFrame(animate);

  // Smooth camera fly-to
  if (flyTarget) {
    flyTarget.t += 0.03;
    const t = Math.min(1, flyTarget.t);
    const ease = t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2; // easeInOutQuad
    camera.position.x += (flyTarget.x - camera.position.x) * ease * 0.08;
    camera.position.y += (flyTarget.y - camera.position.y) * ease * 0.08;
    camera.position.z += (flyTarget.z - camera.position.z) * ease * 0.08;
    if (controls) {
      controls.target.x += (flyTarget.tx - controls.target.x) * ease * 0.08;
      controls.target.y += (flyTarget.ty - controls.target.y) * ease * 0.08;
      controls.target.z += (flyTarget.tz - controls.target.z) * ease * 0.08;
    }
    if (t >= 1) flyTarget = null;
  }

  controls?.update();
  renderer?.render(scene, camera);
}

// ── Reactivity on data change or PCA toggle ──────────────────────────────
$effect(() => {
  const _n = nodes.length;
  const _e = edges.length;
  const _p = usePca; // also rebuild when the PCA toggle flips
  const _h = hiddenKinds.length; // rebuild when filter changes
  const _cm = colorMode; // rebuild when color mode changes
  if (!loaded || !THREE || (_n === 0 && _e === 0)) return;
  buildGraph();
});

// ── Theme ─────────────────────────────────────────────────────────────────
$effect(() => {
  if (scene) scene.background = new THREE.Color(isDark ? BG_DARK : BG_LIGHT);
});

// ── Lifecycle ─────────────────────────────────────────────────────────────
onMount(() => {
  initScene();
});
onDestroy(() => {
  cancelAnimationFrame(animId);
  resizeObs?.disconnect();
  if (canvasClickHandler) renderer?.domElement?.removeEventListener("click", canvasClickHandler);
  controls?.dispose();
  renderer?.dispose();
  if (renderer?.domElement?.parentNode === container) container?.removeChild(renderer.domElement);
});

// ── Legend helpers ────────────────────────────────────────────────────────
// Recompute legend label for found_labels based on toggle + available data.
const foundLabelLegend = $derived.by(() => {
  const hasPCA = usePca && nodes.some((n) => n.kind === "found_label" && n.proj_x !== undefined);
  return hasPCA ? "Found label (PCA)" : "Found label";
});

const LEGEND_BASE = [
  { label: "Query", color: "#8b5cf6" },
  { label: "Text match", color: "#3b82f6" },
];
const EDGE_LEGEND = [
  { label: "Text sim", color: "#8b5cf6" },
  { label: "EEG bridge", color: "#f59e0b" },
  { label: "Time prox", color: "#10b981" },
];

const eegDots = $derived.by(() => {
  const pts = nodes
    .filter((n) => n.kind === "eeg_point" && n.timestamp_unix != null)
    .map((n) => n.timestamp_unix as number);
  if (!pts.length) return [] as { unix: number; t: number; css: string }[];
  const mn = Math.min(...pts),
    mx = Math.max(...pts),
    range = mx - mn || 1;
  return pts
    .map((unix) => ({ unix, t: (unix - mn) / range, css: turboCss((unix - mn) / range) }))
    .sort((a, b) => a.unix - b.unix);
});

const eegTicks = $derived.by(() => {
  if (eegTimeMax <= eegTimeMin) return [];
  return Array.from({ length: 5 }, (_, i) => {
    const t = i / 4;
    const unix = eegTimeMin + t * (eegTimeMax - eegTimeMin);
    const d = new Date(unix * 1000);
    return {
      t,
      label: d.toLocaleString(undefined, {
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
      }),
      css: turboCss(t),
    };
  });
});

function fmtTs(unix: number) {
  if (!unix) return "—";
  return fmtDateTimeLocale(unix);
}
</script>

<!-- Canvas container — click handled directly on renderer.domElement in initScene() -->
<div class="relative w-full h-full" bind:this={container}
     style="cursor:{hoveredNodeId ? 'pointer' : 'default'}"
     onmousemove={onMouseMove}
     onmouseleave={() => { tooltip = null; hoveredNodeId = null; }}
     role="img" aria-label="Interactive search 3D graph">

  <!-- Tooltip -->
  {#if tooltip}
    <div class="pointer-events-none fixed z-50 px-2.5 py-1.5 rounded-lg
                bg-background/95 border border-border shadow-xl
                text-[0.65rem] max-w-[260px] backdrop-blur-sm"
         style="left:{tooltip.x + 12}px; top:{tooltip.y}px;">
      {#each tooltip.lines as line, li}
        <div class="{li === 0 ? 'font-bold text-foreground' : li === tooltip.lines.length-1 && selectedNodeId === null && hoveredNodeId ? 'text-muted-foreground/40 italic text-[0.55rem]' : 'text-muted-foreground/80'} leading-snug">
          {line}
        </div>
      {/each}
    </div>
  {/if}

  <!-- Minimap -->
  {#if loaded && nodeEntries.length > 1}
    {@const mmSize = 80}
    {@const mmNodes = nodeEntries.map(ne => ({
      x: ne.mesh.position.x,
      z: ne.mesh.position.z,
      kind: ne.node.kind,
      selected: ne.node.id === selectedNodeId,
    }))}
    {@const xMin = Math.min(...mmNodes.map(n => n.x)) - 2}
    {@const xMax = Math.max(...mmNodes.map(n => n.x)) + 2}
    {@const zMin = Math.min(...mmNodes.map(n => n.z)) - 2}
    {@const zMax = Math.max(...mmNodes.map(n => n.z)) + 2}
    {@const xRange = xMax - xMin || 1}
    {@const zRange = zMax - zMin || 1}
    <div class="absolute bottom-2 right-2 rounded-lg border border-border/30 bg-background/60 backdrop-blur-sm overflow-hidden"
         style="width:{mmSize}px; height:{mmSize}px;">
      <svg viewBox="0 0 {mmSize} {mmSize}" width={mmSize} height={mmSize}>
        {#each mmNodes as mn}
          {@const mx = ((mn.x - xMin) / xRange) * (mmSize - 8) + 4}
          {@const mz = ((mn.z - zMin) / zRange) * (mmSize - 8) + 4}
          <circle cx={mx} cy={mz} r={mn.selected ? 3 : 1.5}
                  fill={mn.kind === "query" ? "#8b5cf6" : mn.kind === "text_label" ? "#3b82f6" : mn.kind === "eeg_point" ? "#f59e0b" : mn.kind === "found_label" ? "#10b981" : "#06b6d4"}
                  opacity={mn.selected ? 1 : 0.6} />
        {/each}
      </svg>
    </div>
  {/if}

  <!-- Reset view button -->
  <button onclick={resetCamera}
          class="absolute top-2 left-2 px-2 py-1 rounded-md text-[0.5rem]
                 bg-background/70 border border-border/40 backdrop-blur-sm
                 text-muted-foreground/50 hover:text-foreground hover:bg-background/90
                 transition-colors select-none z-10"
          title="Reset camera to default view (or click empty space)">
    ⌂ Reset
  </button>

  <!-- Deselect hint (shown while a node is selected) -->
  {#if selectedNodeId !== null}
    <div class="pointer-events-none absolute bottom-2 left-1/2 -translate-x-1/2
                px-2.5 py-1 rounded-full bg-background/80 border border-border/60
                text-[0.5rem] text-muted-foreground/55 backdrop-blur-sm select-none">
      click node or empty space to deselect · double-click to zoom · ⌂ to reset
    </div>
  {/if}

  <!-- Loading spinner -->
  {#if !loaded}
    <div class="absolute inset-0 flex items-center justify-center">
      <div class="w-5 h-5 rounded-full border-2 border-violet-500/30 border-t-violet-500 animate-spin"></div>
    </div>
  {/if}
</div>

<!-- ── Legend + Jet scale chart ─────────────────────────────────────────── -->
<div class="flex flex-col gap-2 px-4 pt-2 pb-3 border-t border-border dark:border-white/[0.06]">

  <!-- Kind / edge chips row -->
  <div class="flex items-center gap-3 flex-wrap text-[0.42rem] text-muted-foreground/60">
    {#each LEGEND_BASE as l}
      <div class="flex items-center gap-1">
        <span class="inline-block w-2 h-2 rounded-full shrink-0" style="background:{l.color}"></span>
        <span>{l.label}</span>
      </div>
    {/each}
    <div class="flex items-center gap-1">
      <span class="inline-block w-2 h-2 rounded-full shrink-0" style="background:#10b981"></span>
      <span>{foundLabelLegend}</span>
    </div>
    {#if nodes.some(n => n.kind === "screenshot")}
      <div class="flex items-center gap-1">
        <span class="inline-block w-2 h-2 rounded-full shrink-0" style="background:#06b6d4"></span>
        <span>Screenshot</span>
      </div>
    {/if}
    <span class="text-muted-foreground/15 select-none">·</span>
    {#each EDGE_LEGEND as l}
      <div class="flex items-center gap-1">
        <span class="inline-block w-4 h-0.5 shrink-0 rounded" style="background:{l.color}"></span>
        <span>{l.label}</span>
      </div>
    {/each}
    {#if nodes.some(n => n.kind === "screenshot")}
      <div class="flex items-center gap-1">
        <span class="inline-block w-4 h-0.5 shrink-0 rounded" style="background:#06b6d4"></span>
        <span>Screenshot link</span>
      </div>
    {/if}
    <span class="ml-auto text-[0.38rem] italic opacity-40 select-none">
      hover · click to highlight · click again to clear
    </span>
  </div>

  <!-- Jet time-scale chart -->
  {#if eegDots.length > 0}
    <div class="flex flex-col gap-0.5">
      <span class="text-[0.42rem] font-semibold uppercase tracking-widest text-muted-foreground/40 select-none">
        EEG node time scale
      </span>
      <svg width="100%" viewBox="0 0 500 56" preserveAspectRatio="xMidYMid meet"
           style="overflow:visible; display:block">
        <defs>
          <linearGradient id="ig3d-turbo" x1="0" y1="0" x2="1" y2="0">
            {#each Array.from({ length:12 }, (_,i) => i/11) as t}
              <stop offset="{(t*100).toFixed(1)}%" stop-color="{turboCss(t)}" />
            {/each}
          </linearGradient>
        </defs>
        <rect x="0" y="20" width="500" height="14" rx="3" fill="url(#ig3d-turbo)" />
        {#each eegDots as dot}
          {@const x = dot.t * 500}
          <line x1="{x}" y1="14" x2="{x}" y2="20" stroke="{dot.css}" stroke-width="1" stroke-opacity="0.55" />
          <circle cx="{x}" cy="8" r="4" fill="{dot.css}" fill-opacity="0.92" />
        {/each}
        {#each eegTicks as tick}
          {@const x = tick.t * 500}
          <line x1="{x}" y1="34" x2="{x}" y2="40" stroke="{tick.css}" stroke-width="1.2" stroke-opacity="0.65" />
          <text x="{x}" y="52"
                text-anchor="{tick.t < 0.12 ? 'start' : tick.t > 0.88 ? 'end' : 'middle'}"
                font-size="9" fill="currentColor" opacity="0.45" font-family="monospace">
            {tick.label}
          </text>
        {/each}
      </svg>
      <span class="text-[0.38rem] text-muted-foreground/35 tabular-nums select-none">
        {eegDots.length} EEG point{eegDots.length !== 1 ? "s" : ""} ·
        {fmtTs(eegTimeMin)} → {fmtTs(eegTimeMax)}
      </span>
    </div>
  {:else}
    <div class="text-[0.4rem] text-muted-foreground/25 italic select-none">
      EEG nodes colored by session time (turbo gradient)
    </div>
  {/if}
</div>
