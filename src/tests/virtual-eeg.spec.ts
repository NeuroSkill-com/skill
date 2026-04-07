/**
 * Playwright e2e tests for the Virtual Devices window.
 *
 * The Virtual EEG settings tab was removed; the dedicated /virtual-devices
 * route is now the entry point.  Full coverage lives in virtual-device-e2e.spec.ts.
 * This file is kept as a thin redirect alias so existing CI references don't break.
 *
 * Run:  npx playwright test src/tests/virtual-eeg.spec.ts
 */

// All meaningful tests have moved to virtual-device-e2e.spec.ts.
// Re-exporting nothing so Playwright doesn't error on an empty file.
export {};
