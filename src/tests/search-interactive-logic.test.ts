// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
import { describe, expect, it } from "vitest";
import {
  buildDisplayGraph,
  closestScreenshot,
  collectScreenshotResults,
  serialiseEdgesForBackend,
  serialiseNodesForBackend,
} from "$lib/search-interactive-logic";
import type { GraphEdge, GraphNode } from "$lib/search-types";

function mkNode(id: string, kind = "text_label", text = "test"): GraphNode {
  return { id, kind, text, distance: 0.1 };
}

function mkEdge(from: string, to: string): GraphEdge {
  return { from_id: from, to_id: to, distance: 0.1, kind: "text_link" };
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
    expect(serialised.kind).toBe("text_link");
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
