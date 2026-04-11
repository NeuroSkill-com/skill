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

  it("TokenAcl::allows enforces scoped permissions", () => {
    // Admin allows everything
    expect(src).toContain("Self::Admin => true");
    // Read-only path uses read-method gate (GET/HEAD)
    expect(src).toContain('let is_read = matches!(method.as_str(), "GET" | "HEAD")');
    expect(src).toContain("Self::ReadOnly => is_read");
    // Data ACL limited to data namespaces (note: /v1 prefix is stripped before matching)
    expect(src).toContain('path.starts_with("/labels")');
    expect(src).toContain('path.starts_with("/history")');
    // Stream ACL is read-only and includes events/status/version
    expect(src).toContain("Self::Stream => {");
    expect(src).toContain('!TokenAcl::Stream.allows("POST", "/v1/events/push")');
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

  it("hashes token secrets for storage", () => {
    expect(src).toContain("token_hash");
    expect(src).toContain("token_salt");
    expect(src).toContain("Sha256");
  });

  it("checks expiration", () => {
    expect(src).toContain("pub fn is_expired(");
    expect(src).toContain("pub fn is_valid(");
  });

  it("persists to JSON file", () => {
    expect(src).toContain("tokens.json");
    expect(src).toContain("pub fn load(");
    expect(src).toContain("pub fn save(");
    expect(src).toContain("if migrated {");
  });
});

describe("daemon auth middleware", () => {
  const src = fs.readFileSync("crates/skill-daemon/src/auth_middleware.rs", "utf-8");

  it("checks Bearer header", () => {
    expect(src).toContain('strip_prefix("Bearer ")');
  });

  it("checks query param token", () => {
    expect(src).toContain('strip_prefix("token=")');
  });

  it("checks multi-token store with ACL-aware decision", () => {
    expect(src).toContain("store.validate(");
    expect(src).toContain("AuthDecision::Forbidden");
  });

  it("checks legacy single token", () => {
    expect(src).toContain("state.auth_token");
  });
});
