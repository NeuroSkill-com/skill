// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Synonym map for Cmd-K search. Maps abbreviations and colloquial terms
// to their expanded forms so fuzzy matching can find the right setting.
//
// The actual search index is auto-generated from i18n files:
//   npx tsx scripts/build-settings-index.ts
//   → src/lib/generated/settings-search-index.json

export const SYNONYMS: Record<string, string> = {
  dnd: "do not disturb",
  bt: "bluetooth",
  ble: "bluetooth",
  gpu: "graphics processing unit gpu",
  cpu: "central processing unit cpu",
  mic: "microphone",
  tts: "text to speech voice",
  stt: "speech to text",
  llm: "language model ai chat",
  ocr: "optical character recognition",
  fps: "frame rate capture interval",
  ws: "websocket server",
  api: "application programming interface",
  hf: "huggingface",
  lsl: "lab streaming layer",
  eeg: "electroencephalography brainwave",
  emg: "electromyography muscle",
  ecg: "electrocardiography heart",
  exg: "electrophysiology signal eeg emg ecg",
  umap: "dimensionality reduction visualization scatter",
  hnsw: "approximate nearest neighbor index",
  qr: "qr code pair device",
  mcp: "model context protocol tools",
  a11y: "accessibility",
  hc: "high contrast",
  kbd: "keyboard shortcuts hotkeys",
  notif: "notification alert",
  perms: "permissions access",
  vol: "volume audio sound",
  vram: "video memory gpu",
  ctx: "context size tokens",
  kv: "key value cache quantization",
  snr: "signal noise ratio",
  dark: "dark mode theme color",
  light: "light mode theme color",
  font: "font size text zoom",
  wifi: "network connection wireless",
  quiet: "do not disturb focus silence",
  mute: "do not disturb notifications silent",
  privacy: "permissions tracking access",
};
