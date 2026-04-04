// SPDX-License-Identifier: GPL-3.0-only
// One-off daemon client functions that don't warrant their own module.

import { daemonPost } from "./http";

export async function deleteSession(csvPath: string): Promise<void> {
  await daemonPost("/v1/history/sessions/delete", { csv_path: csvPath });
}

export async function submitLabel(args: Record<string, unknown>): Promise<void> {
  await daemonPost("/v1/labels", args);
}

export async function setExgInferenceDevice(device: string): Promise<void> {
  await daemonPost("/v1/settings/exg-inference-device", { value: device });
}

export async function downloadOcrModels(): Promise<void> {
  await daemonPost("/v1/settings/screenshot/download-ocr", {});
}
