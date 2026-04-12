// SPDX-License-Identifier: GPL-3.0-only
// Typed client for iroh remote-access tunnel management (/v1/iroh/).

import { daemonGet, daemonPost } from "./http";

export interface IrohInfo {
  online: boolean;
  endpoint_id?: string;
}

export interface IrohTotp {
  id: string;
  name: string;
  created_at: number;
  revoked_at?: number | null;
  last_used_at?: number | null;
}

export interface IrohClient {
  id: string;
  name: string;
  endpoint_id: string;
  totp_id: string;
  scope: string;
  permissions: IrohPermissions;
  created_at: number;
  revoked_at?: number | null;
  last_connected_at?: number | null;
  last_ip?: string | null;
  last_country?: string | null;
  last_city?: string | null;
  last_locale?: string | null;
  device_model?: string | null;
}

export interface IrohPermissions {
  scope: string;
  groups: string[];
  allow: string[];
  deny: string[];
}

export interface IrohScopeGroup {
  id: string;
  label: string;
  description: string;
  dangerous: boolean;
  commands: string[];
}

export interface PhoneInviteResponse {
  qr_png_base64?: string;
  payload?: unknown;
}

export function getIrohInfo(): Promise<IrohInfo> {
  return daemonGet<IrohInfo>("/v1/iroh/info");
}

export function listIrohTotp(): Promise<{ totp: IrohTotp[] }> {
  return daemonGet<{ totp: IrohTotp[] }>("/v1/iroh/totp");
}

export function createIrohTotp(name: string): Promise<IrohTotp> {
  return daemonPost<IrohTotp>("/v1/iroh/totp", { name });
}

export function getIrohTotpQr(id: string): Promise<{ qr_png_base64: string }> {
  return daemonGet<{ qr_png_base64: string }>(`/v1/iroh/totp/${encodeURIComponent(id)}/qr`);
}

export function revokeIrohTotp(id: string): Promise<{ ok: boolean }> {
  return daemonPost<{ ok: boolean }>(`/v1/iroh/totp/${encodeURIComponent(id)}/revoke`, {});
}

export function listIrohClients(): Promise<{ clients: IrohClient[] }> {
  return daemonGet<{ clients: IrohClient[] }>("/v1/iroh/clients");
}

export function registerIrohClient(body: {
  endpoint_id: string;
  otp: string;
  name?: string;
  totp_id?: string;
  scope?: string;
}): Promise<IrohClient> {
  return daemonPost<IrohClient>("/v1/iroh/clients/register", body);
}

export function revokeIrohClient(id: string): Promise<{ ok: boolean }> {
  return daemonPost<{ ok: boolean }>(`/v1/iroh/clients/${encodeURIComponent(id)}/revoke`, {});
}

export function setIrohClientScope(id: string, scope: string): Promise<{ ok: boolean }> {
  return daemonPost<{ ok: boolean }>(`/v1/iroh/clients/${encodeURIComponent(id)}/scope`, { scope });
}

export function getIrohClientPermissions(id: string): Promise<IrohPermissions> {
  return daemonGet<IrohPermissions>(`/v1/iroh/clients/${encodeURIComponent(id)}/permissions`);
}

export function getIrohScopeGroups(): Promise<{ groups: IrohScopeGroup[] }> {
  return daemonGet<{ groups: IrohScopeGroup[] }>("/v1/iroh/scope-groups");
}

export function phoneInvite(body?: { scope?: string }): Promise<PhoneInviteResponse> {
  return daemonPost<PhoneInviteResponse>("/v1/iroh/phone-invite", body ?? {});
}
