// SPDX-License-Identifier: GPL-3.0-only
/**
 * Montages (daemon: `routes/montage.rs`, `skill_data::montage`): the library and
 * defaults of the settings, and the montage of a single recording.
 */
import { daemonGet, daemonPost } from "./http";

export type ChannelKind = "eeg" | "eog" | "ecg" | "emg" | "ref" | "trigger" | "misc";

export interface MontageChannel {
  /** Recorded name (`CH7`) or number from 1; absent: its own place in the list. */
  source?: string;
  label: string;
  kind?: ChannelKind;
  position?: [number, number, number];
  bad?: boolean;
}

export interface Montage {
  name: string;
  channels: MontageChannel[];
  reference?: string;
  ground?: string;
  coordinates?: string;
  notes?: string;
}

/** A montage as people write it: a library name or labels as text, a list of labels,
 *  `{recorded name or number: label}`, or a montage written out in full. */
export type MontageSpec = string | string[] | Record<string, string> | Montage;

export interface MontageRule {
  montage: string;
  device_kind?: string;
  /** `*` stands for any run of characters. */
  device_name?: string;
  channel_count?: number;
}

export interface ResolvedChannel {
  index: number;
  recorded: string;
  label: string;
  kind: ChannelKind;
  position: [number, number, number] | null;
  bad: boolean;
  from_montage: boolean;
}

export interface ResolvedMontage {
  name: string;
  channels: ResolvedChannel[];
  covered: number;
  reference: string | null;
  ground: string | null;
  warnings: string[];
}

interface Failed {
  ok: false;
  error: string;
}

export type MontageLibrary = { ok: true; library: Montage[]; defaults: MontageRule[]; warnings: string[] } | Failed;
export type MontagePreview = { ok: true; montage: Montage; resolved: ResolvedMontage; warnings: string[] } | Failed;
export type SessionMontage =
  | {
      ok: true;
      /** `default`: by a rule when the session started; `session`: given by hand; null: none. */
      origin: "default" | "session" | null;
      montage: Montage | null;
      resolved: ResolvedMontage;
      warnings: string[];
    }
  | Failed;

export interface DefaultFor {
  deviceKind?: string;
  deviceName?: string;
  channelCount?: number;
}

export function getMontages(): Promise<MontageLibrary> {
  return daemonGet<MontageLibrary>("/v1/settings/montages");
}

/** Add or replace one montage; with `defaultFor`, also make it the default for such sessions. */
export function saveMontage(name: string, montage: MontageSpec, defaultFor?: DefaultFor): Promise<MontageLibrary> {
  return daemonPost<MontageLibrary>("/v1/settings/montages/save", { name, montage, defaultFor });
}

export function deleteMontage(name: string): Promise<MontageLibrary> {
  return daemonPost<MontageLibrary>("/v1/settings/montages/delete", { name });
}

/** Lay a montage over channels (given, or a recording's) without saving anything. */
export function previewMontage(
  montage: MontageSpec,
  over: { channelNames?: string[]; csvPath?: string } = {},
): Promise<MontagePreview> {
  return daemonPost<MontagePreview>("/v1/settings/montages/preview", { montage, ...over });
}

export function getSessionMontage(csvPath: string): Promise<SessionMontage> {
  return daemonGet<SessionMontage>(`/v1/history/sessions/montage?csvPath=${encodeURIComponent(csvPath)}`);
}

/** Give a recording a montage; `null` takes it away. Nothing recorded is rewritten. */
export function setSessionMontage(csvPath: string, montage: MontageSpec | null): Promise<SessionMontage> {
  return daemonPost<SessionMontage>("/v1/history/sessions/montage", { csvPath, montage });
}
