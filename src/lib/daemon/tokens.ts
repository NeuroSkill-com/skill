// SPDX-License-Identifier: GPL-3.0-only
// Typed client for daemon access-token management (/v1/auth/tokens).

import { daemonGet, daemonPost } from "./http";

export type TokenAcl = "Admin" | "ReadOnly" | "Data" | "Stream";
export type TokenExpiry = "Week" | "Month" | "Quarter" | "Never";

export interface ApiToken {
  id: string;
  name: string;
  acl: string;
  preview?: string;
  created_at: number;
  expires_at: number | null;
  last_used_at: number | null;
  revoked: boolean;
  /** Only present in the create response (never in list). */
  token?: string;
}

export function listAuthTokens(): Promise<ApiToken[]> {
  return daemonGet<ApiToken[]>("/v1/auth/tokens");
}

export function createAuthToken(name: string, acl: TokenAcl, expiry: TokenExpiry): Promise<ApiToken> {
  return daemonPost<ApiToken>("/v1/auth/tokens", { name, acl, expiry });
}

export function revokeAuthToken(id: string): Promise<{ ok: boolean }> {
  return daemonPost<{ ok: boolean }>("/v1/auth/tokens/revoke", { id });
}

export function deleteAuthToken(id: string): Promise<{ ok: boolean }> {
  return daemonPost<{ ok: boolean }>("/v1/auth/tokens/delete", { id });
}

export function refreshDefaultToken(): Promise<{ ok: boolean; token: string }> {
  return daemonPost<{ ok: boolean; token: string }>("/v1/auth/default-token/refresh", {});
}
