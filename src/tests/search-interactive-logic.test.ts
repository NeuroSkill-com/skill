// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
import { describe, expect, it } from "vitest";
import {
  buildDisplayGraph,
  closestScreenshot,
  collectScreenshotResults,
  serialiseEdgesForBackend,
  serialiseNodesForBackend,
} from "$lib/search/search-interactive-logic";
import type { GraphEdge, GraphNode } from "$lib/search/search-types";

function mkNode(id: string, kind: GraphNode["kind"] = "text_label", text = "test"): GraphNode {
  return { id, kind, text, distance: 0.1 };
}

function mkEdge(from: string, to: string): GraphEdge {
  return { from_id: from, to_id: to, distance: 0.1, kind: "text_sim" };
}

describe("buildDisplayGraph", () => {
  it("passes through nodes/edges when no dedup or screenshots", () => {
    const nodes = [mkNode("a"), mkNode("b")];
    const edges = [mkEdge("a", "b")];
    const result = buildDisplayGraph(nodes, edges, false, false, new Map(), () => "");
    expect(result.nodes).toHaveLength(2);
    expect(result.edges).toHaveLength(1);
  });

  it("injects screenshot nodes when enabled", () => {
    const nodes = [mkNode("a")];
    const edges: GraphEdge[] = [];
    const ssMap = new Map([
      [
        "a\0screenshot.png",
        { filename: "screenshot.png", appName: "Chrome", windowTitle: "Title", unixTs: 1000, similarity: 0.9 },
      ],
    ]);
    const result = buildDisplayGraph(nodes, edges, false, true, ssMap, (f) => `/img/${f}`);
    expect(result.nodes).toHaveLength(2);
    expect(result.nodes[1].kind).toBe("screenshot");
    expect(result.nodes[1].screenshot_url).toBe("/img/screenshot.png");
    expect(result.edges).toHaveLength(1);
    expect(result.edges[0].kind).toBe("screenshot_link");
  });

  it("skips screenshot nodes with missing parents", () => {
    const nodes = [mkNode("a")];
    const ssMap = new Map([
      [
        "missing\0screenshot.png",
        { filename: "screenshot.png", appName: "", windowTitle: "", unixTs: 1000, similarity: 0 },
      ],
    ]);
    const result = buildDisplayGraph(nodes, [], false, true, ssMap, () => "");
    expect(result.nodes).toHaveLength(1); // no screenshot added
    expect(result.edges).toHaveLength(0);
  });
});

describe("collectScreenshotResults", () => {
  it("deduplicates by filename", () => {
    const results = [
      { nodeId: "a", hits: [{ unix_ts: 1, filename: "f1.png", app_name: "A", window_title: "T", similarity: 0.9 }] },
      { nodeId: "b", hits: [{ unix_ts: 2, filename: "f1.png", app_name: "A", window_title: "T", similarity: 0.8 }] },
    ];
    const map = collectScreenshotResults(results);
    expect(map.size).toBe(1);
    expect(map.has("a\0f1.png")).toBe(true);
  });

  it("collects multiple unique screenshots", () => {
    const results = [
      {
        nodeId: "a",
        hits: [
          { unix_ts: 1, filename: "f1.png", app_name: "A", window_title: "T", similarity: 0.9 },
          { unix_ts: 2, filename: "f2.png", app_name: "B", window_title: "U", similarity: 0.7 },
        ],
      },
    ];
    const map = collectScreenshotResults(results);
    expect(map.size).toBe(2);
  });
});

describe("serialiseNodesForBackend", () => {
  it("nullifies missing optional fields", () => {
    const node = mkNode("test");
    const [serialised] = serialiseNodesForBackend([node]);
    expect(serialised.id).toBe("test");
    expect(serialised.parent_id).toBeNull();
    expect(serialised.filename).toBeNull();
    expect(serialised.ocr_text).toBeNull();
  });
});

describe("serialiseEdgesForBackend", () => {
  it("preserves edge fields", () => {
    const edge = mkEdge("a", "b");
    const [serialised] = serialiseEdgesForBackend([edge]);
    expect(serialised.from_id).toBe("a");
    expect(serialised.to_id).toBe("b");
    expect(serialised.kind).toBe("text_sim");
  });
});

describe("closestScreenshot", () => {
  it("returns the closest screenshot to target timestamp", () => {
    const screenshots = [
      { unix_ts: 100, filename: "a.png", app_name: "", window_title: "", similarity: 0 },
      { unix_ts: 200, filename: "b.png", app_name: "", window_title: "", similarity: 0 },
      { unix_ts: 300, filename: "c.png", app_name: "", window_title: "", similarity: 0 },
    ];
    expect(closestScreenshot(screenshots, 190)?.filename).toBe("b.png");
    expect(closestScreenshot(screenshots, 250)?.filename).toBe("b.png");
    expect(closestScreenshot(screenshots, 290)?.filename).toBe("c.png");
  });

  it("returns null for empty array", () => {
    expect(closestScreenshot([], 100)).toBeNull();
  });
});

// ── AI Summary prompt builder: EEG metrics display ────────────────────────

describe("AI Summary EEG metrics display", () => {
  // This mirrors the inline logic in +page.svelte lines 3010-3024
  function buildEegDetail(n: GraphNode): string {
    const ts = n.timestamp_unix ? new Date(n.timestamp_unix * 1000).toLocaleString() : "unknown";
    const m = (n.eeg_metrics ?? {}) as Record<string, number | null>;
    const parts = [`t=${ts}`, `dist=${n.distance.toFixed(3)}`];
    if (m.engagement != null) parts.push(`eng=${(m.engagement as number).toFixed(2)}`);
    if (m.relaxation != null) parts.push(`rel=${(m.relaxation as number).toFixed(2)}`);
    if (m.snr != null) parts.push(`snr=${(m.snr as number).toFixed(1)}`);
    if (m.rel_alpha != null) parts.push(`α=${(m.rel_alpha as number).toFixed(3)}`);
    if (m.rel_beta != null) parts.push(`β=${(m.rel_beta as number).toFixed(3)}`);
    if (m.rel_theta != null) parts.push(`θ=${(m.rel_theta as number).toFixed(3)}`);
    if (m.hr != null && (m.hr as number) > 0) parts.push(`hr=${(m.hr as number).toFixed(0)}`);
    if (n.relevance_score != null) parts.push(`relevance=${n.relevance_score.toFixed(3)}`);
    if (n.session_id) parts.push(`session=${n.session_id}`);
    if (!m.engagement && !m.relaxation && !m.snr) parts.push("(no EEG metrics stored)");
    return parts.join(", ");
  }

  it("shows metrics when eeg_metrics is populated", () => {
    const node: GraphNode = {
      id: "ep0_0",
      kind: "eeg_point",
      distance: 0.5,
      timestamp_unix: 1772578071,
      eeg_metrics: { engagement: 50.0, relaxation: 30.0, snr: 15.0, rel_alpha: 0.025 },
      relevance_score: 0.4,
      session_id: "20260303_22h",
    };
    const detail = buildEegDetail(node);
    expect(detail).toContain("eng=50.00");
    expect(detail).toContain("rel=30.00");
    expect(detail).toContain("snr=15.0");
    expect(detail).toContain("α=0.025");
    expect(detail).not.toContain("(no EEG metrics stored)");
  });

  it("shows '(no EEG metrics stored)' when eeg_metrics is null", () => {
    const node: GraphNode = {
      id: "ep0_0",
      kind: "eeg_point",
      distance: 0.997,
      timestamp_unix: 1772578071,
      eeg_metrics: null,
      relevance_score: 0.499,
      session_id: "20260303_22h",
    };
    const detail = buildEegDetail(node);
    expect(detail).toContain("(no EEG metrics stored)");
    expect(detail).not.toContain("eng=");
  });

  it("shows '(no EEG metrics stored)' when eeg_metrics is empty object", () => {
    const node: GraphNode = {
      id: "ep0_0",
      kind: "eeg_point",
      distance: 0.5,
      timestamp_unix: 1772578071,
      eeg_metrics: {},
    };
    const detail = buildEegDetail(node);
    expect(detail).toContain("(no EEG metrics stored)");
  });

  it("shows '(no EEG metrics stored)' when all metrics are zero", () => {
    const node: GraphNode = {
      id: "ep0_0",
      kind: "eeg_point",
      distance: 0.5,
      timestamp_unix: 1772578071,
      eeg_metrics: { engagement: 0, relaxation: 0, snr: 0 },
    };
    const detail = buildEegDetail(node);
    // Note: engagement=0 is falsy → triggers the "(no EEG metrics stored)" check
    expect(detail).toContain("(no EEG metrics stored)");
  });

  it("includes hr when present and > 0", () => {
    const node: GraphNode = {
      id: "ep0_0",
      kind: "eeg_point",
      distance: 0.5,
      timestamp_unix: 1772578071,
      eeg_metrics: { engagement: 50, relaxation: 30, snr: 15, hr: 72 },
    };
    const detail = buildEegDetail(node);
    expect(detail).toContain("hr=72");
  });
});

// ── New node/edge kind tests ────────────────────────────────────────────────

describe("new graph node kinds", () => {
  it("accepts file_activity kind", () => {
    const node = mkNode("fa1", "file_activity", "main.rs");
    expect(node.kind).toBe("file_activity");
  });

  it("accepts meeting kind", () => {
    const node = mkNode("mtg1", "meeting", "Zoom call");
    expect(node.kind).toBe("meeting");
  });

  it("mkNode with all supported kinds doesn't throw", () => {
    const kinds: GraphNode["kind"][] = [
      "query",
      "text_label",
      "eeg_point",
      "found_label",
      "screenshot",
      "file_activity",
      "meeting",
    ];
    for (const k of kinds) {
      expect(() => mkNode(`node_${k}`, k)).not.toThrow();
    }
  });
});

describe("new graph edge kinds", () => {
  it("accepts file_activity_prox kind", () => {
    const edge: GraphEdge = { from_id: "a", to_id: "fa1", distance: 0, kind: "file_activity_prox" };
    expect(edge.kind).toBe("file_activity_prox");
  });

  it("accepts meeting_prox kind", () => {
    const edge: GraphEdge = { from_id: "a", to_id: "mtg1", distance: 0, kind: "meeting_prox" };
    expect(edge.kind).toBe("meeting_prox");
  });

  it("all edge kinds are valid", () => {
    const kinds: GraphEdge["kind"][] = [
      "text_sim",
      "eeg_bridge",
      "eeg_sim",
      "label_prox",
      "screenshot_link",
      "file_activity_prox",
      "meeting_prox",
    ];
    for (const k of kinds) {
      const edge: GraphEdge = { from_id: "a", to_id: "b", distance: 0.1, kind: k };
      expect(edge.kind).toBe(k);
    }
  });
});

describe("file_activity node fields", () => {
  it("supports file_path and language fields", () => {
    const node: GraphNode = {
      id: "fa1",
      kind: "file_activity",
      text: "main.rs",
      distance: 0,
      file_path: "/src/main.rs",
      language: "rust",
      was_modified: true,
      lines_added: 10,
      lines_removed: 3,
    };
    expect(node.file_path).toBe("/src/main.rs");
    expect(node.language).toBe("rust");
    expect(node.was_modified).toBe(true);
    expect(node.lines_added).toBe(10);
    expect(node.lines_removed).toBe(3);
  });
});
