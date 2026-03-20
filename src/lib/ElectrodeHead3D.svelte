<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { T, useTask, useThrelte } from "@threlte/core";
  import { OrbitControls, useGltf } from "@threlte/extras";
  import {
    BufferGeometry,
    Float32BufferAttribute,
    ShaderMaterial,
    Color,
    Vector3,
    Group,
    MeshPhysicalMaterial,
    LineBasicMaterial,
    RingGeometry,
    Raycaster,
    type Mesh,
  } from "three";
  import { getResolved } from "$lib/stores/theme.svelte";
  import {
    type Electrode,
    type ElectrodeSystem,
    type BrainRegion,
    regionColors,
    getElectrodes,
  } from "$lib/data/electrodes";

  interface Props {
    system: ElectrodeSystem;
    /** When provided, overrides the electrode list derived from `system`. */
    electrodesOverride?: Electrode[] | null;
    onSelect?: (electrode: Electrode | null) => void;
    selectedName?: string | null;
    /** Position offset for the electrode group [x, y, z] */
    electrodePosition?: [number, number, number];
    /** Scale for the electrode group [x, y, z] */
    electrodeScale?: [number, number, number];
  }

  let {
    system,
    electrodesOverride = null,
    onSelect,
    selectedName = null,
    electrodePosition = [0, 0, 0],
    electrodeScale = [1, 1, 1],
  }: Props = $props();

  let resolvedTheme = $derived(getResolved());
  let visible = $derived(electrodesOverride ?? getElectrodes(system));

  const { camera } = useThrelte();

  // ── Load head model ──
  const gltfStore = useGltf("/models/head.glb");
  let gltfScene: Group | null = $state(null);

  const headMat = new MeshPhysicalMaterial({
    color: 0x2a2a40,
    roughness: 0.35,
    metalness: 0.3,
    clearcoat: 0.6,
    clearcoatRoughness: 0.15,
    envMapIntensity: 1.2,
    emissive: new Color(0x818cf8),
    emissiveIntensity: 0.08,
  });

  let headMesh: Mesh | null = $state(null);
  let headCenter = $state(new Vector3(0, 0, 0));

  // ── Simple mapping ──
  // MNE positions are in mm. The head model is ~8.5 units wide, MNE cloud is ~176mm wide.
  // One uniform scale factor, no stretching.
  // const SCALE = 8.55 / 176.0; // ≈ 0.0486
  const SCALE = 0.0235
  // MNE cloud center (in Three.js coords): approximately (0.6, 16.6, -15.5) mm
  // Model center: approximately (0, 0, 0)
  // Offset = modelCenter - mneCenter * scale
  const OFFSET = new Vector3(
    0 - 0.6 * SCALE,      // x
    0 + 85 * SCALE,        // y
    0 + 29 * SCALE,        // z
  );

  // Raycast origin — the center of the cranium, NOT the full model.
  // The model includes neck/shoulders so its geometric center is too low.
  // The cranium center is roughly at Y≈2 (upper half of head), Z≈-0.3 (slightly back).
  let craniumCenter = $state(new Vector3(0, 2, -0.3));

  gltfStore.subscribe((g) => {
    if (!g) return;
    gltfScene = g.scene;
    g.scene.traverse((child: any) => {
      if (child.isMesh) {
        headMesh = child;
        child.material = headMat;
        child.geometry.computeBoundingBox();
        const box = child.geometry.boundingBox!;
        headCenter = new Vector3().addVectors(box.min, box.max).multiplyScalar(0.5);
        // Cranium center: midpoint of upper 60% of the head, slightly behind center
        craniumCenter = new Vector3(
          0,
          box.min.y + (box.max.y - box.min.y) * 0.6,
          box.min.z + (box.max.z - box.min.z) * 0.4,
        );
      }
    });
  });

  // ── Electrode positions ──
  const _raycaster = new Raycaster();
  const NORMAL_OFFSET = 0.1;

  function mneToWorld(pos: [number, number, number]): Vector3 {
    return new Vector3(
      pos[0] * SCALE + OFFSET.x,
      pos[1] * SCALE + OFFSET.y,
      pos[2] * SCALE + OFFSET.z,
    );
  }

  /** Find the closest point on the head mesh surface to a given point. */
  function closestPointOnMesh(target: Vector3): { point: Vector3; normal: Vector3 } | null {
    if (!headMesh) return null;
    const posAttr = headMesh.geometry.getAttribute("position");
    const indexAttr = headMesh.geometry.getIndex();
    if (!posAttr) return null;

    const a = new Vector3(), b = new Vector3(), c = new Vector3();
    const closestOnTri = new Vector3();
    let bestDist = Infinity;
    let bestPoint = new Vector3();
    let bestNormal = new Vector3();

    const triCount = indexAttr ? indexAttr.count / 3 : posAttr.count / 3;

    for (let i = 0; i < triCount; i++) {
      let ia: number, ib: number, ic: number;
      if (indexAttr) {
        ia = indexAttr.getX(i * 3);
        ib = indexAttr.getX(i * 3 + 1);
        ic = indexAttr.getX(i * 3 + 2);
      } else {
        ia = i * 3; ib = i * 3 + 1; ic = i * 3 + 2;
      }
      a.fromBufferAttribute(posAttr, ia);
      b.fromBufferAttribute(posAttr, ib);
      c.fromBufferAttribute(posAttr, ic);

      closestPointOnTriangle(target, a, b, c, closestOnTri);
      const d = target.distanceToSquared(closestOnTri);
      if (d < bestDist) {
        bestDist = d;
        bestPoint.copy(closestOnTri);
        // Compute triangle normal
        const edge1 = b.clone().sub(a);
        const edge2 = c.clone().sub(a);
        bestNormal.crossVectors(edge1, edge2).normalize();
        // Ensure normal faces outward (away from cranium center)
        if (bestNormal.dot(bestPoint.clone().sub(craniumCenter)) < 0) {
          bestNormal.negate();
        }
      }
    }
    return bestDist < Infinity ? { point: bestPoint.clone(), normal: bestNormal.clone() } : null;
  }

  /** Closest point on triangle ABC to point P. */
  function closestPointOnTriangle(p: Vector3, a: Vector3, b: Vector3, c: Vector3, out: Vector3): Vector3 {
    const ab = b.clone().sub(a), ac = c.clone().sub(a), ap = p.clone().sub(a);
    const d1 = ab.dot(ap), d2 = ac.dot(ap);
    if (d1 <= 0 && d2 <= 0) return out.copy(a);

    const bp = p.clone().sub(b);
    const d3 = ab.dot(bp), d4 = ac.dot(bp);
    if (d3 >= 0 && d4 <= d3) return out.copy(b);

    const cp = p.clone().sub(c);
    const d5 = ab.dot(cp), d6 = ac.dot(cp);
    if (d6 >= 0 && d5 <= d6) return out.copy(c);

    const vc = d1 * d4 - d3 * d2;
    if (vc <= 0 && d1 >= 0 && d3 <= 0) {
      const v = d1 / (d1 - d3);
      return out.copy(a).addScaledVector(ab, v);
    }

    const vb = d5 * d2 - d1 * d6;
    if (vb <= 0 && d2 >= 0 && d6 <= 0) {
      const w = d2 / (d2 - d6);
      return out.copy(a).addScaledVector(ac, w);
    }

    const va = d3 * d6 - d5 * d4;
    if (va <= 0 && (d4 - d3) >= 0 && (d5 - d6) >= 0) {
      const w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
      return out.copy(b).addScaledVector(c.clone().sub(b), w);
    }

    const denom = 1 / (va + vb + vc);
    const v = vb * denom;
    const w = vc * denom;
    return out.copy(a).addScaledVector(ab, v).addScaledVector(ac, w);
  }

  function electrodePosOnSurface(el: Electrode): Vector3 {
    const target = mneToWorld(el.pos);
    if (!headMesh) return target;

    // First try raycast from cranium center outward toward electrode
    const dir = target.clone().sub(craniumCenter).normalize();
    _raycaster.set(craniumCenter, dir);
    const hits = _raycaster.intersectObject(headMesh, false);
    if (hits.length > 0) {
      const hit = hits[hits.length - 1];
      return hit.point.clone().add(hit.face!.normal.clone().multiplyScalar(NORMAL_OFFSET));
    }

    // Fallback: find the closest point on the mesh surface
    const closest = closestPointOnMesh(target);
    if (closest) {
      return closest.point.add(closest.normal.multiplyScalar(NORMAL_OFFSET));
    }

    return target;
  }

  // ── Electrode point cloud ──
  function buildElectrodeGeo(els: Electrode[], selName: string | null) {
    const positions = new Float32Array(els.length * 3);
    const colors = new Float32Array(els.length * 3);
    const sizes = new Float32Array(els.length);

    for (let i = 0; i < els.length; i++) {
      const pos = electrodePosOnSurface(els[i]);
      positions[i * 3] = pos.x;
      positions[i * 3 + 1] = pos.y;
      positions[i * 3 + 2] = pos.z;

      const isSelected = els[i].name === selName;
      const col = new Color(regionColors[els[i].region]);
      if (isSelected) col.lerp(new Color(0xffffff), 0.4);
      colors[i * 3] = col.r;
      colors[i * 3 + 1] = col.g;
      colors[i * 3 + 2] = col.b;

      sizes[i] = isSelected ? 2.8 : els[i].muse ? 2.0 : 1.0;
    }

    const geo = new BufferGeometry();
    geo.setAttribute("position", new Float32BufferAttribute(positions, 3));
    geo.setAttribute("aColor", new Float32BufferAttribute(colors, 3));
    geo.setAttribute("aSize", new Float32BufferAttribute(sizes, 1));
    return geo;
  }

  let elGeo = $derived(buildElectrodeGeo(visible, selectedName));

  const elMat = new ShaderMaterial({
    uniforms: { uOpacity: { value: 0.95 } },
    vertexShader: `
      attribute vec3 aColor;
      attribute float aSize;
      varying vec3 vColor;
      varying float vSize;
      void main() {
        vec4 mvPos = modelViewMatrix * vec4(position, 1.0);
        gl_PointSize = aSize * (300.0 / -mvPos.z);
        gl_Position = projectionMatrix * mvPos;
        vColor = aColor;
        vSize = aSize;
      }
    `,
    fragmentShader: `
      uniform float uOpacity;
      varying vec3 vColor;
      varying float vSize;
      void main() {
        float d = length(gl_PointCoord - 0.5) * 2.0;
        float inner = 1.0 - smoothstep(0.0, 0.55, d);
        float ring = smoothstep(0.45, 0.6, d) * (1.0 - smoothstep(0.65, 0.95, d));
        float halo = (1.0 - smoothstep(0.0, 1.0, d)) * 0.25;
        float brightness = vSize > 1.5 ? 1.3 : 1.0;
        vec3 col = vColor * brightness;
        float a = (inner * 0.9 + ring * 0.5 + halo) * uOpacity;
        if (a < 0.01) discard;
        gl_FragColor = vec4(col, a);
      }
    `,
    transparent: true,
    depthWrite: false,
    depthTest: true,
  });

  // ── Muse headband arc ──
  function buildMuseBand(): BufferGeometry {
    const geo = new BufferGeometry();
    const museNames = ["TP9", "AF7", "AF8", "TP10"];
    const museEls = museNames.map(n => visible.find(e => e.name === n)).filter(Boolean) as Electrode[];
    if (museEls.length !== 4) return geo;

    const musePositions = museEls.map(e => electrodePosOnSurface(e));
    const pts: Vector3[] = [];
    const steps = 48;
    for (let i = 0; i <= steps; i++) {
      const t = i / steps;
      let p: Vector3;
      if (t < 0.333) {
        p = musePositions[0].clone().lerp(musePositions[1], t / 0.333);
      } else if (t < 0.667) {
        p = musePositions[1].clone().lerp(musePositions[2], (t - 0.333) / 0.334);
      } else {
        p = musePositions[2].clone().lerp(musePositions[3], (t - 0.667) / 0.333);
      }
      if (headMesh) {
        const dir = p.clone().sub(craniumCenter).normalize();
        _raycaster.set(craniumCenter, dir);
        const hits = _raycaster.intersectObject(headMesh, false);
        if (hits.length > 0) {
          const hit = hits[hits.length - 1];
          p = hit.point.clone().add(hit.face!.normal.clone().multiplyScalar(0.08));
        }
      }
      pts.push(p);
    }

    const verts = new Float32Array((pts.length - 1) * 6);
    for (let i = 0; i < pts.length - 1; i++) {
      verts[i * 6] = pts[i].x;     verts[i * 6 + 1] = pts[i].y;     verts[i * 6 + 2] = pts[i].z;
      verts[i * 6 + 3] = pts[i+1].x; verts[i * 6 + 4] = pts[i+1].y; verts[i * 6 + 5] = pts[i+1].z;
    }
    geo.setAttribute("position", new Float32BufferAttribute(verts, 3));
    return geo;
  }

  let bandGeo = $derived(buildMuseBand());
  const bandMat = new LineBasicMaterial({ color: 0x818cf8, transparent: true, opacity: 0.7 });

  // ── Selection ring ──
  function buildSelectionRing(name: string | null) {
    if (!name) return null;
    const el = visible.find(e => e.name === name);
    if (!el) return null;
    const pos = electrodePosOnSurface(el);
    const normal = pos.clone().sub(craniumCenter).normalize();
    return { position: pos, normal };
  }

  let selRing = $derived(buildSelectionRing(selectedName));

  const ringGeo = new RingGeometry(0.18, 0.28, 24);
  const ringMat = new ShaderMaterial({
    uniforms: { uTime: { value: 0 }, uColor: { value: new Color(0xffffff) } },
    vertexShader: `
      varying vec2 vUv;
      void main() { vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }
    `,
    fragmentShader: `
      uniform float uTime; uniform vec3 uColor; varying vec2 vUv;
      void main() { float pulse = 0.6 + 0.4 * sin(uTime * 4.0); gl_FragColor = vec4(uColor, pulse); }
    `,
    transparent: true, depthWrite: false, side: 2,
  });

  // ── Group / hit test ──
  let group: Group | undefined = $state();

  export function hitTest(ndcX: number, ndcY: number): Electrode | null {
    const cam = camera.current;
    if (!cam || !group) return null;
    let bestDist = 0.06;
    let bestEl: Electrode | null = null;
    for (const el of visible) {
      const pos = electrodePosOnSurface(el);
      pos.applyMatrix4(group.matrixWorld);
      const projected = pos.clone().project(cam);
      const dx = projected.x - ndcX;
      const dy = projected.y - ndcY;
      const screenDist = Math.sqrt(dx * dx + dy * dy);
      if (screenDist < bestDist) { bestDist = screenDist; bestEl = el; }
    }
    return bestEl;
  }

  // ── Theme ──
  $effect(() => {
    const dark = resolvedTheme === "dark";
    headMat.color = new Color(dark ? 0x2a2a40 : 0xc8c8d8);
    headMat.metalness = dark ? 0.3 : 0.1;
    headMat.roughness = dark ? 0.35 : 0.5;
    headMat.emissive = new Color(dark ? 0x818cf8 : 0x6366f1);
    headMat.emissiveIntensity = dark ? 0.08 : 0.03;
    elMat.uniforms.uOpacity.value = dark ? 0.95 : 0.85;
    bandMat.color = new Color(dark ? 0x818cf8 : 0x6366f1);
  });

  useTask((delta) => { ringMat.uniforms.uTime.value += delta; });

  let modelLoaded = $derived(!!gltfScene);
</script>

<T.PerspectiveCamera
  makeDefault={true as any}
  position={[0, 4, 14.12]}
  fov={36}
  near={0.1}
  far={100}
  oncreate={(ref) => ref?.lookAt(0, 0, 0)}
>
  <OrbitControls
    enableDamping dampingFactor={0.12}
    enableZoom minDistance={6} maxDistance={20}
    enablePan={false}
    autoRotate autoRotateSpeed={0.6}
    target={[0, 0, 0]}
  />
</T.PerspectiveCamera>

<T.AmbientLight intensity={resolvedTheme === "dark" ? 0.5 : 0.7} />
<T.DirectionalLight position={[5, 8, 8]} intensity={resolvedTheme === "dark" ? 0.8 : 0.6} color={resolvedTheme === "dark" ? 0xc8c8ff : 0xffffff} />
<T.DirectionalLight position={[-6, 4, -3]} intensity={0.3} color={0x818cf8} />
<T.DirectionalLight position={[0, -3, 5]} intensity={0.15} />
<T.DirectionalLight position={[0, 10, 0]} intensity={resolvedTheme === "dark" ? 0.4 : 0.2} color={0xa5b4fc} />

<T.Group oncreate={(ref) => { group = ref; }}>
  {#if gltfScene}
    <T is={gltfScene} />
  {/if}

  <!-- Electrode group — position/scale moves all electrodes together -->
  <T.Group
    position={electrodePosition}
    scale={electrodeScale}
  >
    {#if modelLoaded}
      <T.LineSegments geometry={bandGeo} material={bandMat} />
    {/if}

    <T.Points geometry={elGeo} material={elMat} />

    {#if selRing}
      <T.Mesh
        geometry={ringGeo} material={ringMat}
        position={[selRing.position.x, selRing.position.y, selRing.position.z]}
        oncreate={(ref) => {
          if (selRing) {
            const target = selRing.position.clone().add(selRing.normal);
            ref.lookAt(target.x, target.y, target.z);
          }
        }}
      />
    {/if}
  </T.Group>
</T.Group>
