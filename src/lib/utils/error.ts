// SPDX-License-Identifier: GPL-3.0-only

/** Convert unknown thrown values to a display-safe message. */
export function toErrorMessage(err: unknown, fallback = "failed"): string {
  if (err instanceof Error && err.message) return err.message;
  if (typeof err === "string" && err.length > 0) return err;
  if (err == null) return fallback;
  return String(err);
}

/** Build a localized prefixed error message (e.g. "Error: ..."). */
export function formatPrefixedError(prefix: string, err: unknown, fallback = "failed"): string {
  return `${prefix}: ${toErrorMessage(err, fallback)}`;
}
