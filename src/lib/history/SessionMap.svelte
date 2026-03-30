<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  SessionMap — PMTiles + MapLibre GL map for a session.

  Behaviour:
  • GPS points available  → fitBounds to show the full track (zoom out as needed).
  • No GPS points         → fetch https://ipwho.is for an IP-based city-level
                            location and centre the map there at zoom 10.
                            A dashed circle + label mark the area as approximate.

  Props:
    points  – GpsPoint[] from get_session_location (may be empty)
    color   – CSS colour for the track line  (default: violet)
    height  – CSS height string              (default: "180px")
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

interface IpGeo {
  latitude: number;
  longitude: number;
  city: string;
  region: string;
  country_code: string;
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
let usingIpFallback = $state(false);
let ipCity = $state("");

// ── Module-level IP-geo cache (shared across all SessionMap instances) ────────
// Avoids re-fetching on every expand / re-render.
let _ipGeoCache: IpGeo | null | "error" = null;
let _ipGeoPromise: Promise<IpGeo | null> | null = null;

async function resolveIpGeo(): Promise<IpGeo | null> {
  if (_ipGeoCache === "error") return null;
  if (_ipGeoCache) return _ipGeoCache;
  if (_ipGeoPromise) return _ipGeoPromise;
  _ipGeoPromise = fetch("https://ipwho.is/", { signal: AbortSignal.timeout(5000) })
    .then((r) => r.json())
    .then((d) => {
      if (
        typeof d?.latitude === "number" &&
        typeof d?.longitude === "number" &&
        Number.isFinite(d.latitude) &&
        Number.isFinite(d.longitude)
      ) {
        _ipGeoCache = d as IpGeo;
        return _ipGeoCache;
      }
      _ipGeoCache = "error";
      return null;
    })
    .catch(() => {
      _ipGeoCache = "error";
      return null;
    });
  return _ipGeoPromise;
}

// ── Bounding-box helpers ──────────────────────────────────────────────────────

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

// ── Map initialisation ────────────────────────────────────────────────────────

async function initMap() {
  if (!container || destroyed) return;

  const [maplibreGl, pmtilesLib] = await Promise.all([import("maplibre-gl"), import("pmtiles")]);
  const { Map: MapLibreMap, NavigationControl } = maplibreGl;
  const { Protocol } = pmtilesLib;

  if (destroyed) return;

  // Register the pmtiles:// protocol handler once per page lifetime.
  const proto = new Protocol();
  maplibreGl.addProtocol("pmtiles", proto.tile.bind(proto));

  const dark = getResolved() === "dark";
  const styleUrl = dark ? "https://tiles.openfreemap.org/styles/dark" : "https://tiles.openfreemap.org/styles/liberty";

  // ── Determine initial viewport ─────────────────────────────────────────────
  const hasGps = points.length > 0;
  let initCenter: [number, number];
  let initZoom: number;

  if (hasGps) {
    // Centre on the midpoint of the track; fitBounds will correct it on load.
    const [minLon, minLat, maxLon, maxLat] = bbox();
    initCenter = [(minLon + maxLon) / 2, (minLat + maxLat) / 2];
    initZoom = 12;
    usingIpFallback = false;
  } else {
    // IP-based fallback — fetch while the style loads (parallel).
    const geo = await resolveIpGeo();
    if (destroyed) return;
    if (geo) {
      initCenter = [geo.longitude, geo.latitude];
      initZoom = 10; // city-level view
      usingIpFallback = true;
      ipCity = [geo.city, geo.region, geo.country_code].filter(Boolean).join(", ");
    } else {
      // Total fallback: world centre
      initCenter = [0, 20];
      initZoom = 2;
      usingIpFallback = true;
      ipCity = "";
    }
  }

  map = new MapLibreMap({
    container,
    style: styleUrl,
    center: initCenter,
    zoom: initZoom,
    attributionControl: false,
    pitchWithRotate: false,
  });

  map.addControl(new NavigationControl({ showCompass: false }), "top-right");

  map.on("load", () => {
    if (!map || destroyed) return;

    if (hasGps) {
      // ── GPS track ────────────────────────────────────────────────────
      const coords = points.map((p) => [p.lon, p.lat]);

      map.addSource("track", {
        type: "geojson",
        data: { type: "Feature", properties: {}, geometry: { type: "LineString", coordinates: coords } },
      });
      map.addLayer({
        id: "track-line",
        type: "line",
        source: "track",
        layout: { "line-join": "round", "line-cap": "round" },
        paint: { "line-color": color, "line-width": 3, "line-opacity": 0.85 },
      });

      // Start dot (green)
      const first = points[0];
      map.addSource("start", {
        type: "geojson",
        data: { type: "Feature", properties: {}, geometry: { type: "Point", coordinates: [first.lon, first.lat] } },
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

      // End dot (red)
      const last = points[points.length - 1];
      map.addSource("end", {
        type: "geojson",
        data: { type: "Feature", properties: {}, geometry: { type: "Point", coordinates: [last.lon, last.lat] } },
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

      // Fit the full track into view.
      if (points.length > 1) {
        const [minLon, minLat, maxLon, maxLat] = bbox();
        map.fitBounds(
          [
            [minLon, minLat],
            [maxLon, maxLat],
          ],
          { padding: 32, maxZoom: 16, duration: 0 },
        );
      }
    } else if (usingIpFallback && initCenter[0] !== 0) {
      // ── IP-based approximate location marker ─────────────────────────
      // Dashed circle indicating the approximate area (~city level).
      map.addSource("ip-loc", {
        type: "geojson",
        data: { type: "Feature", properties: {}, geometry: { type: "Point", coordinates: initCenter } },
      });
      // Outer faint halo
      map.addLayer({
        id: "ip-halo",
        type: "circle",
        source: "ip-loc",
        paint: {
          "circle-radius": 40,
          "circle-color": color,
          "circle-opacity": 0.08,
          "circle-stroke-width": 1.5,
          "circle-stroke-color": color,
          "circle-stroke-opacity": 0.35,
        },
      });
      // Centre dot
      map.addLayer({
        id: "ip-dot",
        type: "circle",
        source: "ip-loc",
        paint: {
          "circle-radius": 5,
          "circle-color": color,
          "circle-opacity": 0.6,
          "circle-stroke-width": 2,
          "circle-stroke-color": "#fff",
          "circle-stroke-opacity": 0.8,
        },
      });
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

// Re-initialise when points change (lazy-loaded after row is expanded).
$effect(() => {
  // Depend on points identity so the effect re-runs when the array reference changes.
  const _dep = points;
  if (map) {
    map.remove();
    map = null;
  }
  if (!destroyed && container) {
    void initMap();
  }
});
</script>

<svelte:head>
  <link rel="stylesheet" href="https://unpkg.com/maplibre-gl@5/dist/maplibre-gl.css" />
</svelte:head>

<div class="flex flex-col gap-0.5">
  <div
    bind:this={container}
    class="w-full rounded-lg overflow-hidden border border-border dark:border-white/[0.06]"
    style="height:{height};"
    aria-label="Session location map"
  ></div>
  {#if usingIpFallback}
    <p class="text-[0.46rem] text-muted-foreground/40 text-right px-0.5">
      📍 Approximate location{ipCity ? ` · ${ipCity}` : ""} · no GPS recorded for this session
    </p>
  {/if}
</div>
