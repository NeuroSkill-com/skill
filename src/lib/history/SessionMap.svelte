<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  SessionMap — renders a GPS track for a single recording session using
  MapLibre GL JS with a PMTiles basemap (OpenFreeMap planet tiles, no API key).

  Props:
    points  – array of {ts, lat, lon, alt, accuracy, speed}
    color   – CSS colour string for the track line (defaults to violet)
    height  – CSS height string (defaults to "180px")
-->
<script lang="ts">
import { onDestroy, onMount } from "svelte";
import { getResolved } from "$lib/stores/theme.svelte";

export interface GpsPoint {
  ts: number;
  lat: number;
  lon: number;
  alt: number;
  accuracy: number;
  speed: number;
}

interface Props {
  points: GpsPoint[];
  color?: string;
  height?: string;
}

let { points, color = "#7c3aed", height = "180px" }: Props = $props();

// biome-ignore lint/style/noNonNullAssertion: bound by svelte bind:this
let container: HTMLDivElement = $state()!;
let map: import("maplibre-gl").Map | null = null;
let destroyed = false;

// ── helpers ──────────────────────────────────────────────────────────────────

function bbox(): [number, number, number, number] {
  let minLon = Infinity,
    maxLon = -Infinity;
  let minLat = Infinity,
    maxLat = -Infinity;
  for (const p of points) {
    if (p.lon < minLon) minLon = p.lon;
    if (p.lon > maxLon) maxLon = p.lon;
    if (p.lat < minLat) minLat = p.lat;
    if (p.lat > maxLat) maxLat = p.lat;
  }
  return [minLon, minLat, maxLon, maxLat];
}

function center(): [number, number] {
  const [minLon, minLat, maxLon, maxLat] = bbox();
  return [(minLon + maxLon) / 2, (minLat + maxLat) / 2];
}

// ── map initialisation ────────────────────────────────────────────────────────

async function initMap() {
  if (points.length === 0 || !container || destroyed) return;

  // Dynamic imports — avoids including MapLibre in the initial bundle
  // for pages that never open the history view with GPS data.
  const [maplibreGl, pmtilesLib] = await Promise.all([import("maplibre-gl"), import("pmtiles")]);
  const { Map: MapLibreMap, NavigationControl } = maplibreGl;
  const { Protocol } = pmtilesLib;

  if (destroyed) return;

  // Register the pmtiles:// protocol handler once per page lifetime.
  const proto = new Protocol();
  maplibreGl.addProtocol("pmtiles", proto.tile.bind(proto));

  const dark = getResolved() === "dark";

  // OpenFreeMap serves free PMTiles planet tiles — no API key required.
  // Style reference: https://openfreemap.org/
  const styleUrl = dark ? "https://tiles.openfreemap.org/styles/dark" : "https://tiles.openfreemap.org/styles/liberty";

  const [cx, cy] = center();

  map = new MapLibreMap({
    container,
    style: styleUrl,
    center: [cx, cy],
    zoom: 12,
    attributionControl: false,
    pitchWithRotate: false,
  });

  map.addControl(new NavigationControl({ showCompass: false }), "top-right");

  map.on("load", () => {
    if (!map || destroyed) return;

    // ── Track line ──────────────────────────────────────────────────────
    const coords = points.map((p) => [p.lon, p.lat]);

    map.addSource("track", {
      type: "geojson",
      data: {
        type: "Feature",
        properties: {},
        geometry: { type: "LineString", coordinates: coords },
      },
    });

    map.addLayer({
      id: "track-line",
      type: "line",
      source: "track",
      layout: { "line-join": "round", "line-cap": "round" },
      paint: {
        "line-color": color,
        "line-width": 3,
        "line-opacity": 0.85,
      },
    });

    // ── Start marker (green circle) ─────────────────────────────────────
    const first = points[0];
    map.addSource("start", {
      type: "geojson",
      data: {
        type: "Feature",
        properties: {},
        geometry: { type: "Point", coordinates: [first.lon, first.lat] },
      },
    });
    map.addLayer({
      id: "start-dot",
      type: "circle",
      source: "start",
      paint: {
        "circle-radius": 6,
        "circle-color": "#22c55e",
        "circle-stroke-width": 2,
        "circle-stroke-color": "#fff",
      },
    });

    // ── End marker (red circle) ─────────────────────────────────────────
    const last = points[points.length - 1];
    map.addSource("end", {
      type: "geojson",
      data: {
        type: "Feature",
        properties: {},
        geometry: { type: "Point", coordinates: [last.lon, last.lat] },
      },
    });
    map.addLayer({
      id: "end-dot",
      type: "circle",
      source: "end",
      paint: {
        "circle-radius": 6,
        "circle-color": "#ef4444",
        "circle-stroke-width": 2,
        "circle-stroke-color": "#fff",
      },
    });

    // Fit the viewport to the full track with padding.
    if (points.length > 1) {
      const [minLon, minLat, maxLon, maxLat] = bbox();
      map.fitBounds(
        [
          [minLon, minLat],
          [maxLon, maxLat],
        ],
        {
          padding: 32,
          maxZoom: 16,
          duration: 0,
        },
      );
    }
  });
}

onMount(() => {
  void initMap();
});

onDestroy(() => {
  destroyed = true;
  map?.remove();
  map = null;
});

// Re-initialise when points change (e.g. lazy-loaded after expand).
$effect(() => {
  const _dep = points;
  if (map) {
    map.remove();
    map = null;
  }
  if (!destroyed && container && points.length > 0) {
    void initMap();
  }
});
</script>

<svelte:head>
  <!-- MapLibre GL CSS — loaded once per page, cached by browser thereafter. -->
  <link
    rel="stylesheet"
    href="https://unpkg.com/maplibre-gl@5/dist/maplibre-gl.css"
  />
</svelte:head>

{#if points.length === 0}
  <!-- No GPS data for this session — render nothing -->
{:else}
  <div
    bind:this={container}
    class="w-full rounded-lg overflow-hidden border border-border dark:border-white/[0.06]"
    style="height:{height};"
    aria-label="GPS track map"
  ></div>
{/if}
