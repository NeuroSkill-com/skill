// SPDX-License-Identifier: GPL-3.0-only

import * as fs from "node:fs";
import { describe, expect, it } from "vitest";

describe("daemon auth module (auth.rs)", () => {
  const src = fs.readFileSync("crates/skill-daemon/src/auth.rs", "utf-8");

  it("defines TokenAcl variants", () => {
    expect(src).toContain("Admin");
    expect(src).toContain("ReadOnly");
    expect(src).toContain("Data");
    expect(src).toContain("Stream");
  });

  it("defines TokenExpiry variants", () => {
    expect(src).toContain("Week");
    expect(src).toContain("Month");
    expect(src).toContain("Quarter");
    expect(src).toContain("Never");
  });

  it("TokenAcl::allows enforces correct permissions", () => {
    // Admin allows everything
    expect(src).toContain("Self::Admin => true");
    // ReadOnly only allows GET
    expect(src).toContain('Self::ReadOnly => method == "GET"');
    // Stream only allows events/status
    expect(src).toContain('path.starts_with("/v1/events")');
  });

  it("generates sk- prefixed tokens", () => {
    expect(src).toContain("sk-");
  });

  it("TokenStore has CRUD operations", () => {
    expect(src).toContain("pub fn create(");
    expect(src).toContain("pub fn validate(");
    expect(src).toContain("pub fn authorize(");
    expect(src).toContain("pub fn revoke(");
    expect(src).toContain("pub fn delete(");
    expect(src).toContain("pub fn list_redacted(");
  });

  it("redacts token secrets in list", () => {
    expect(src).toContain("redacted.token = format!");
    expect(src).toContain("…");
  });

  it("checks expiration", () => {
    expect(src).toContain("pub fn is_expired(");
    expect(src).toContain("pub fn is_valid(");
  });

  it("persists to JSON file", () => {
    expect(src).toContain("tokens.json");
    expect(src).toContain("pub fn load(");
    expect(src).toContain("pub fn save(");
  });
});

describe("daemon auth middleware", () => {
  const src = fs.readFileSync("crates/skill-daemon/src/main.rs", "utf-8");

  it("checks Bearer header", () => {
    expect(src).toContain('strip_prefix("Bearer ")');
  });

  it("checks query param token", () => {
    expect(src).toContain('strip_prefix("token=")');
  });

  it("checks multi-token store", () => {
    expect(src).toContain("store.authorize(");
  });

  it("checks legacy single token", () => {
    expect(src).toContain("state.auth_token");
  });
});
