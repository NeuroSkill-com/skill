// SPDX-License-Identifier: GPL-3.0-only
// Shared Tauri mock builder for screenshot/GIF automation.
// Auto-extracted — do not edit manually, update take-screenshots.mjs instead.

function buildTauriMock(theme = "light") {
  return `
// ── Mock Tauri IPC globals ─────────────────────────────────────────────────
// Uses the same shape as @tauri-apps/api/mocks for full compatibility.
window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
window.__TAURI_EVENT_PLUGIN_INTERNALS__ = window.__TAURI_EVENT_PLUGIN_INTERNALS__ || {};

window.__TAURI_INTERNALS__.metadata = {
  currentWindow: { label: "main" },
  currentWebview: { windowLabel: "main", label: "main" },
};

// Callback registry (needed for Channel, listen, etc.)
const _callbacks = new Map();
let _cbId = 1;
window.__TAURI_INTERNALS__.transformCallback = (cb, once) => {
  const id = _cbId++;
  _callbacks.set(id, (data) => {
    if (once) _callbacks.delete(id);
    return cb && cb(data);
  });
  return id;
};
window.__TAURI_INTERNALS__.unregisterCallback = (id) => _callbacks.delete(id);
window.__TAURI_INTERNALS__.runCallback = (id, data) => {
  const cb = _callbacks.get(id);
  if (cb) cb(data);
};
window.__TAURI_INTERNALS__.callbacks = _callbacks;

window.__TAURI_INTERNALS__.convertFileSrc = (path) => path;

// Event plugin — mock listen/unlisten so pages using listen() don't crash
window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener = () => {};

window.__TAURI_INTERNALS__.invoke = async (cmd, args) => {
  // Handle event plugin commands (listen, emit, unlisten)
  if (cmd === "plugin:event|listen")   return args?.handler ?? 0;
  if (cmd === "plugin:event|unlisten") return null;
  if (cmd === "plugin:event|emit")     return null;
  // Handle notification plugin
  if (cmd.startsWith("plugin:notification|")) return null;
  if (cmd.startsWith("plugin:"))       return null;
  return window.__SKILL_MOCK_INVOKE__(cmd, args);
};

// ── Mock invoke handler ────────────────────────────────────────────────────
window.__SKILL_MOCK_INVOKE__ = (cmd, args) => {
  const MOCKS = {
    // ── Theme / settings ─────────────────────────────────────────────────
    get_theme_and_language: () => ["${theme}", "en"],
    get_accent_color:       () => "violet",
    set_theme:              () => null,
    set_accent_color:       () => null,
    set_language:           () => null,
    show_main_window:       () => null,
    get_app_name:           () => "NeuroSkill",

    // ── Device status ────────────────────────────────────────────────────
    get_status: () => ({
      state: "connected",
      device_name: "Muse 2 Demo",
      device_id: "00:55:DA:B7:DE:AD",
      serial_number: "DEMO-1234-ABCD",
      mac_address: "00-55-DA-B7-DE-AD",
      csv_path: "/data/session_20260318_120000.csv",
      sample_count: 48200,
      battery: 72,
      eeg: [820.5, 815.2, 818.7, 822.1],
      paired_devices: [
        { id: "00:55:DA:B7:DE:AD", name: "Muse 2 Demo", last_seen: ${Date.now() / 1000 | 0} },
      ],
      bt_error: null,
      target_name: "Muse 2 Demo",
      filter_config: { sample_rate: 256, low_pass_hz: 50, high_pass_hz: 1, notch: "Hz60", notch_bandwidth_hz: 2 },
      channel_quality: ["good", "good", "good", "fair"],
      retry_attempt: 0,
      retry_countdown_secs: 0,
      ppg: [1200, 3400, 2800],
      ppg_sample_count: 12800,
      accel: [0.02, -0.01, 1.0],
      gyro: [0.5, -0.3, 0.1],
      fuel_gauge_mv: 3850,
      temperature_raw: 2048,
      device_kind: "muse",
      hardware_version: "p21",
    }),

    get_devices: () => [
      { id: "00:55:DA:B7:DE:AD", name: "Muse 2 Demo",   last_rssi: -55, is_paired: true,  is_preferred: true },
      { id: "00:55:DA:B7:BE:EF", name: "Muse S Gen 2",  last_rssi: -72, is_paired: true,  is_preferred: false },
    ],

    // ── Band powers ──────────────────────────────────────────────────────
    get_latest_bands: () => ({
      delta: [0.35, 0.32, 0.33, 0.34],
      theta: [0.22, 0.25, 0.24, 0.23],
      alpha: [0.28, 0.30, 0.29, 0.27],
      beta:  [0.10, 0.09, 0.11, 0.10],
      gamma: [0.05, 0.04, 0.03, 0.06],
    }),

    // ── EEG model ────────────────────────────────────────────────────────
    get_eeg_model_status: () => ({
      weights_found: true,
      downloading_weights: false,
      download_progress: 1.0,
      download_status_msg: null,
      download_retry_attempt: 0,
      download_retry_in_secs: 0,
      encoder_loaded: true,
      model_name: "NeuroGPT-base",
      model_size_mb: 245,
    }),
    get_eeg_model_config: () => ({
      model_id: "neurogpt-base",
      quantize: false,
      gpu_layers: 0,
    }),

    // ── GPU stats ────────────────────────────────────────────────────────
    // Values are fractions 0–1 (the UI multiplies by 100 to display %).
    get_gpu_stats: () => ({
      name: "NVIDIA RTX 4090",
      render: 0.28,
      tiler: 0.25,
      overall: 0.30,
      utilisation: 0.30,
      memory_used_mb: 3200,
      memory_total_mb: 24576,
      temperature_c: 58,
    }),

    // ── Filter config ────────────────────────────────────────────────────
    get_filter_config: () => ({
      sample_rate: 256,
      low_pass_hz: 50,
      high_pass_hz: 1,
      notch: "Hz60",
      notch_bandwidth_hz: 2,
    }),

    // ── History ──────────────────────────────────────────────────────────
    list_session_days: () => ["20260318", "20260317", "20260316", "20260315", "20260314"],
    list_sessions_for_day: (args) => {
      const day = args?.day || "20260318";
      const dayBase = new Date(
        parseInt(day.slice(0,4)), parseInt(day.slice(4,6))-1, parseInt(day.slice(6,8)),
        8, 0, 0
      ).getTime() / 1000;
      return [
        {
          csv_file: "session_" + day + "_120000.csv",
          csv_path: "/data/session_" + day + "_120000.csv",
          session_start_utc: dayBase + 14400,
          session_end_utc:   dayBase + 18000,
          device_name: "Muse 2 Demo",
          serial_number: "DEMO-1234-ABCD",
          battery_pct: 72,
          total_samples: 921600,
          sample_rate_hz: 256,
          file_size_bytes: 185000000,
          labels: [
            { id: 1, text: "Deep focus — coding", context: "Work", eeg_start: dayBase + 14400, eeg_end: dayBase + 15000, created_at: dayBase + 14400, embedding_model: "neurogpt-base" },
            { id: 2, text: "Meditation", context: "Break", eeg_start: dayBase + 15600, eeg_end: dayBase + 16200, created_at: dayBase + 15600, embedding_model: "neurogpt-base" },
            { id: 3, text: "Reading", context: "Study", eeg_start: dayBase + 16800, eeg_end: dayBase + 17400, created_at: dayBase + 16800, embedding_model: "neurogpt-base" },
          ],
        },
        {
          csv_file: "session_" + day + "_080000.csv",
          csv_path: "/data/session_" + day + "_080000.csv",
          session_start_utc: dayBase,
          session_end_utc:   dayBase + 3600,
          device_name: "Muse 2 Demo",
          serial_number: "DEMO-1234-ABCD",
          battery_pct: 85,
          total_samples: 921600,
          sample_rate_hz: 256,
          file_size_bytes: 192000000,
          labels: [
            { id: 4, text: "Morning meditation", context: "Routine", eeg_start: dayBase, eeg_end: dayBase + 1200, created_at: dayBase, embedding_model: "neurogpt-base" },
          ],
        },
      ];
    },
    get_session_metrics: () => ({
      n_epochs: 720,
      rel_delta: 0.33, rel_theta: 0.23, rel_alpha: 0.28, rel_beta: 0.11, rel_gamma: 0.05,
      relaxation: 0.62, engagement: 0.58, faa: 0.12,
      tar: 1.22, bar: 0.39, dtr: 1.43, tbr: 2.09,
      pse: 0.85, apf: 10.2, sef95: 28.5, spectral_centroid: 14.3, bps: 0.72, snr: 12.5,
      coherence: 0.65, mu_suppression: 0.42, mood: 0.58,
      hjorth_activity: 150, hjorth_mobility: 0.35, hjorth_complexity: 1.8,
      permutation_entropy: 0.82, higuchi_fd: 1.52, dfa_exponent: 0.68,
      sample_entropy: 1.45, pac_theta_gamma: 0.28, laterality_index: 0.08,
      hr: 68, rmssd: 42, sdnn: 55, pnn50: 32, lf_hf_ratio: 1.2,
      respiratory_rate: 14, spo2_estimate: 98, perfusion_index: 0.8, stress_index: 35,
      meditation: 0.62, cognitive_load: 0.45, drowsiness: 0.15,
      blink_count: 180, blink_rate: 15,
      head_pitch: 5, head_roll: 2, stillness: 0.85, nod_count: 12, shake_count: 3,
    }),
    get_csv_metrics: () => ({
      n_rows: 720,
      summary: {
        n_epochs: 720,
        rel_delta: 0.33, rel_theta: 0.23, rel_alpha: 0.28, rel_beta: 0.11, rel_gamma: 0.05,
        relaxation: 0.62, engagement: 0.58, faa: 0.12,
        tar: 1.22, bar: 0.39, dtr: 1.43, tbr: 2.09,
        pse: 0.85, apf: 10.2, sef95: 28.5, spectral_centroid: 14.3, bps: 0.72, snr: 12.5,
        coherence: 0.65, mu_suppression: 0.42, mood: 0.58,
        hjorth_activity: 150, hjorth_mobility: 0.35, hjorth_complexity: 1.8,
        permutation_entropy: 0.82, higuchi_fd: 1.52, dfa_exponent: 0.68,
        sample_entropy: 1.45, pac_theta_gamma: 0.28, laterality_index: 0.08,
        hr: 68, rmssd: 42, sdnn: 55, pnn50: 32, lf_hf_ratio: 1.2,
        respiratory_rate: 14, spo2_estimate: 98, perfusion_index: 0.8, stress_index: 35,
        meditation: 0.62, cognitive_load: 0.45, drowsiness: 0.15,
        blink_count: 180, blink_rate: 15,
        head_pitch: 5, head_roll: 2, stillness: 0.85, nod_count: 12, shake_count: 3,
      },
      timeseries: Array.from({ length: 60 }, (_, i) => ({
        t: ${Date.now() / 1000 - 7200 | 0} + i * 60,
        rd: 0.33 + Math.cos(i/15)*0.04, rt: 0.22 + Math.sin(i/12)*0.05,
        ra: 0.25 + Math.sin(i/10)*0.08, rb: 0.10 + Math.cos(i/8)*0.03,
        rg: 0.05 + Math.sin(i/6)*0.02,
        focus: 0.65 + Math.cos(i/8)*0.12,
        relaxation: 0.55 + Math.sin(i/10)*0.15,
        engagement: 0.58 + Math.sin(i/9)*0.1,
        faa: 0.12 + Math.cos(i/14)*0.08,
        med: 0.55 + Math.sin(i/10)*0.15,
        cog: 0.4 + Math.cos(i/12)*0.1,
        drow: 0.12 + Math.sin(i/20)*0.08,
        tar: 1.22, bar: 0.39, dtr: 1.43, tbr: 2.09,
        pse: 0.85, apf: 10.2, sef95: 28.5, sc: 14.3, bps: 0.72, snr: 12.5,
        coherence: 0.65, mu: 0.42, mood: 0.58,
        ha: 150, hm: 0.35, hc: 1.8,
        pe: 0.82, hfd: 1.52, dfa: 0.68, se: 1.45, pac: 0.28, lat: 0.08,
        hr: 68 + Math.sin(i/5)*5, rmssd: 42, sdnn: 55, pnn50: 32, lf_hf: 1.2,
        resp: 14, spo2: 98, perf: 0.8, stress: 35,
        blinks: 3, blink_r: 15,
        pitch: 5, roll: 2, still: 0.85, nods: 0, shakes: 0,
        gpu: 0, gpu_render: 0, gpu_tiler: 0,
      })),
    }),
    get_session_timeseries: () => {
      const rows = [];
      const base = ${Date.now() / 1000 - 7200 | 0};
      for (let i = 0; i < 60; i++) {
        rows.push({
          ts: base + i * 60,
          alpha: 0.25 + Math.sin(i / 10) * 0.08,
          beta:  0.10 + Math.cos(i / 8) * 0.03,
          theta: 0.22 + Math.sin(i / 12) * 0.05,
          delta: 0.33 + Math.cos(i / 15) * 0.04,
          gamma: 0.05 + Math.sin(i / 6) * 0.02,
          meditation: 0.55 + Math.sin(i / 10) * 0.15,
          focus: 0.65 + Math.cos(i / 8) * 0.12,
          drowsiness: 0.12 + Math.sin(i / 20) * 0.08,
          cognitive_load: 0.4 + Math.cos(i / 12) * 0.1,
        });
      }
      return rows;
    },
    get_history_stats: () => ({
      total_sessions: 42,
      total_secs: 151200,
      this_week_secs: 21600,
      last_week_secs: 18000,
    }),
    query_annotations: () => {
      const labels = [];
      const texts = ["Deep focus — coding", "Meditation session", "Reading research paper",
                     "Music listening — classical", "Relaxation break"];
      for (let i = 0; i < 5; i++) {
        labels.push({
          id: i + 1,
          text: texts[i],
          context: "Work session",
          eeg_start: ${Date.now() / 1000 - 7200 | 0} + i * 600,
          eeg_end:   ${Date.now() / 1000 - 7200 | 0} + i * 600 + 300,
          created_at: ${Date.now() / 1000 - 7200 | 0} + i * 600,
          embedding_model: "neurogpt-base",
        });
      }
      return labels;
    },

    // ── Labels ───────────────────────────────────────────────────────────
    search_labels_by_text: () => [
      { id: 1, text: "Deep focus — coding", context: "Work session", similarity: 0.92, eeg_start: ${Date.now() / 1000 - 7200 | 0}, eeg_end: ${Date.now() / 1000 - 6600 | 0}, created_at: ${Date.now() / 1000 - 7200 | 0}, date: "20260318" },
      { id: 2, text: "Meditation — morning", context: "Routine", similarity: 0.85, eeg_start: ${Date.now() / 1000 - 18000 | 0}, eeg_end: ${Date.now() / 1000 - 16800 | 0}, created_at: ${Date.now() / 1000 - 18000 | 0}, date: "20260318" },
      { id: 3, text: "Reading research paper", context: "Study", similarity: 0.78, eeg_start: ${Date.now() / 1000 - 93600 | 0}, eeg_end: ${Date.now() / 1000 - 92400 | 0}, created_at: ${Date.now() / 1000 - 93600 | 0}, date: "20260317" },
    ],

    // ── About ────────────────────────────────────────────────────────────
    get_about_info: () => ({
      name: "NeuroSkill",
      version: "0.0.42",
      tagline: "Open-source EEG-powered cognitive assistant",
      website: "https://neuroskill.com",
      websiteLabel: "neuroskill.com",
      repoUrl: "https://github.com/neuroskill/skill",
      discordUrl: "https://discord.gg/neuroskill",
      license: "GPL-3.0-only",
      licenseName: "GNU General Public License v3.0",
      licenseUrl: "https://www.gnu.org/licenses/gpl-3.0.html",
      copyright: "Copyright (C) 2026 NeuroSkill.com",
      authors: [
        ["Alice Neuhaus", "Lead Developer"],
        ["Bob Cortex", "EEG Signal Processing"],
        ["Carol Synapse", "UI/UX Design"],
      ],
      acknowledgements: "Built with Tauri, SvelteKit, and open EEG research.",
      iconDataUrl: "data:image/png;base64,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",
    }),

    // ── Chat / LLM ──────────────────────────────────────────────────────
    get_llm_server_status: () => ({ status: "running", model: "llama-3.2-3b-q4", port: 8080 }),
    get_llm_config: () => ({
      model_id: "llama-3.2-3b-q4",
      n_gpu_layers: 99,
      context_size: 8192,
      port: 8080,
      auto_start: true,
    }),
    get_llm_catalog: () => ({
      entries: [
        { filename: "qwen3.5-4b-q4_k_m.gguf", name: "Qwen 3.5 4B Q4", size_bytes: 2800000000, state: "downloaded", progress: 1.0, family: "qwen35-4b", family_id: "qwen35-4b", family_name: "Qwen 3.5 4B", quant: "Q4_K_M", params: "4B", context_length: 8192, is_mmproj: false, mmproj: null },
        { filename: "llama-3.2-3b-q4.gguf", name: "Llama 3.2 3B Q4", size_bytes: 2048000000, state: "downloaded", progress: 1.0, family: "llama", family_id: "llama-3.2-3b", family_name: "Llama 3.2 3B", quant: "Q4_K_M", params: "3B", context_length: 8192, is_mmproj: false, mmproj: null },
        { filename: "phi-3-mini-q4.gguf",   name: "Phi-3 Mini Q4",   size_bytes: 2300000000, state: "downloaded", progress: 1.0, family: "phi", family_id: "phi-3-mini", family_name: "Phi-3 Mini", quant: "Q4_K_M", params: "3.8B", context_length: 4096, is_mmproj: false, mmproj: null },
        { filename: "gemma-2b-q4.gguf",     name: "Gemma 2B Q4",     size_bytes: 1500000000, state: "not_downloaded", progress: 0, family: "gemma", family_id: "gemma-2b", family_name: "Gemma 2B", quant: "Q4_K_M", params: "2B", context_length: 8192, is_mmproj: false, mmproj: null },
      ],
      active_model: "qwen3.5-4b-q4_k_m.gguf",
      active_mmproj: "",
    }),
    get_last_chat_session: () => ({
      id: 1,
      title: "Demo Chat Session",
      messages: [
        { id: 1, role: "user",      content: "What does my EEG data show about my focus today?", timestamp: ${Date.now() - 300000} },
        { id: 2, role: "assistant", content: "Based on your recent EEG session, your **alpha/beta ratio** suggests good sustained focus during the past hour. Your frontal asymmetry index is slightly left-dominant, which typically correlates with approach motivation and engagement.\\n\\nKey observations:\\n- **Alpha power**: 0.28 (above your baseline)\\n- **Beta power**: 0.11 (moderate, steady)\\n- **Focus score**: 71%\\n- **Meditation score**: 62%\\n\\nYour cognitive load appears moderate at 45%, suggesting you\\'re engaged but not overwhelmed.", timestamp: ${Date.now() - 280000} },
      ],
      created_at: ${Date.now() - 3600000},
      archived: false,
    }),
    list_chat_sessions: () => [
      { id: 1, title: "Demo Chat Session", created_at: ${Date.now() - 3600000}, message_count: 2, archived: false },
      { id: 2, title: "Sleep Analysis", created_at: ${Date.now() - 86400000}, message_count: 6, archived: false },
      { id: 3, title: "Focus Techniques", created_at: ${Date.now() - 172800000}, message_count: 4, archived: false },
    ],

    // ── Search ───────────────────────────────────────────────────────────
    stream_search_embeddings: (args) => {
      // args.onProgress is the raw Channel object; Channel.id is the
      // callback ID registered via transformCallback.
      const ch = args?.onProgress;
      const chId = ch && typeof ch === "object" && typeof ch.id === "number"
        ? ch.id : null;
      if (chId == null) return null;

      const now = ${Date.now() / 1000 | 0};
      const queryCount = 8;
      const days = ["20260318", "20260317"];
      const labels = [
        { id: 1, text: "Deep focus — coding", context: "Work", eeg_start: now - 3600, eeg_end: now - 3000, created_at: now - 3600, embedding_model: "neurogpt-base" },
        { id: 2, text: "Meditation", context: "Break", eeg_start: now - 7200, eeg_end: now - 6600, created_at: now - 7200, embedding_model: "neurogpt-base" },
      ];
      const send = (idx, msg) => {
        window.__TAURI_INTERNALS__.runCallback(chId, { index: idx, message: msg });
      };

      setTimeout(() => {
        send(0, { kind: "started", query_count: queryCount, searched_days: days });

        for (let q = 0; q < queryCount; q++) {
          const qTime = now - 7200 + q * 300;
          const neighbors = [];
          for (let n = 0; n < 5; n++) {
            const nTime = now - 86400 + q * 600 + n * 120;
            neighbors.push({
              timestamp_unix: nTime,
              date: n < 3 ? "20260318" : "20260317",
              distance: 0.05 + n * 0.08 + Math.random() * 0.02,
              device_name: "Muse 2 Demo",
              labels: n === 0 ? [labels[q % 2]] : [],
              metrics: {
                relaxation: 0.55 + Math.random() * 0.2,
                engagement: 0.50 + Math.random() * 0.2,
                meditation: 0.60 + Math.random() * 0.15,
                alpha: 0.25 + Math.random() * 0.1,
                beta: 0.10 + Math.random() * 0.05,
              },
            });
          }
          send(1 + q, {
            kind: "result",
            done_count: q + 1,
            entry: { timestamp_unix: qTime, neighbors },
          });
        }
        send(1 + queryCount, { kind: "done", total: queryCount });
      }, 50);
      return null;
    },

    // ── Calibration ──────────────────────────────────────────────────────
    list_calibration_profiles: () => [
      {
        id: "default", name: "Default Profile",
        actions: [
          { label: "Eyes closed — relax", duration_secs: 60 },
          { label: "Eyes open — focus on cross", duration_secs: 60 },
          { label: "Deep breathing", duration_secs: 60 },
          { label: "Mental arithmetic", duration_secs: 60 },
          { label: "Music listening", duration_secs: 60 },
        ],
        break_duration_secs: 10,
        loop_count: 3,
        auto_start: false,
        last_calibration_utc: ${Date.now() / 1000 - 86400 | 0},
      },
      {
        id: "evening", name: "Evening Profile",
        actions: [
          { label: "Relaxation", duration_secs: 120 },
          { label: "Meditation", duration_secs: 120 },
        ],
        break_duration_secs: 15,
        loop_count: 2,
        auto_start: false,
        last_calibration_utc: ${Date.now() / 1000 - 172800 | 0},
      },
    ],
    get_active_calibration: () => ({
      id: "default", name: "Default Profile",
      actions: [
        { label: "Eyes closed — relax", duration_secs: 60 },
        { label: "Eyes open — focus on cross", duration_secs: 60 },
        { label: "Deep breathing", duration_secs: 60 },
        { label: "Mental arithmetic", duration_secs: 60 },
        { label: "Music listening", duration_secs: 60 },
      ],
      break_duration_secs: 10,
      loop_count: 3,
      auto_start: false,
      last_calibration_utc: ${Date.now() / 1000 - 86400 | 0},
    }),

    // ── Downloads ────────────────────────────────────────────────────────
    get_llm_downloads: () => [
      { repo: "Qwen/Qwen3.5-4B-GGUF", filename: "qwen3.5-4b-q4_k_m.gguf", quant: "Q4_K_M", size_gb: 2.8, description: "Qwen 3.5 4B", is_mmproj: false, state: "downloaded", status_msg: null, progress: 1.0, initiated_at_unix: ${Date.now() / 1000 - 86400 | 0}, local_path: "/models/qwen3.5-4b-q4_k_m.gguf" },
      { repo: "meta-llama/Llama-3.2-3B-GGUF", filename: "llama-3.2-3b-q4.gguf", quant: "Q4_K_M", size_gb: 2.0, description: "Llama 3.2 3B", is_mmproj: false, state: "downloaded", status_msg: null, progress: 1.0, initiated_at_unix: ${Date.now() / 1000 - 172800 | 0}, local_path: "/models/llama-3.2-3b-q4.gguf" },
      { repo: "microsoft/Phi-3-mini-GGUF", filename: "phi-3-mini-q4.gguf", quant: "Q4_K_M", size_gb: 2.3, description: "Phi-3 Mini", is_mmproj: false, state: "downloading", status_msg: "Downloading...", progress: 0.65, initiated_at_unix: ${Date.now() / 1000 - 600 | 0}, local_path: null },
    ],

    // ── Focus modes ──────────────────────────────────────────────────────
    list_focus_modes: () => [
      { id: "pomodoro",   name: "Pomodoro",   work_mins: 25, break_mins: 5 },
      { id: "deep_work",  name: "Deep Work",  work_mins: 50, break_mins: 10 },
      { id: "short_focus", name: "Short Focus", work_mins: 15, break_mins: 5 },
    ],

    // ── Hooks ────────────────────────────────────────────────────────────
    get_hooks: () => [
      { id: 1, name: "Focus Alert", enabled: true, trigger: "focus_drop", threshold: 0.3, action: "tts", keywords: ["focus", "attention"] },
      { id: 2, name: "Break Reminder", enabled: true, trigger: "duration", threshold: 3600, action: "notification", keywords: ["break"] },
    ],
    get_hook_statuses: () => [
      { id: 1, last_triggered: ${Date.now() / 1000 - 1800 | 0}, trigger_count: 3 },
      { id: 2, last_triggered: ${Date.now() / 1000 - 900 | 0}, trigger_count: 7 },
    ],
    get_hook_log: () => [],

    // ── TTS ──────────────────────────────────────────────────────────────
    get_neutts_config: () => ({ enabled: true, backbone_repo: "neuphonic/neutts-nano-q4-gguf", gguf_file: "", voice_preset: "jo", ref_wav_path: "", ref_text: "", voice: "en-nova", speed: 1.0, pitch: 1.0 }),
    tts_list_neutts_voices: () => [
      { id: "en-nova",  name: "Nova",  lang: "en", gender: "female" },
      { id: "en-onyx",  name: "Onyx",  lang: "en", gender: "male" },
      { id: "en-echo",  name: "Echo",  lang: "en", gender: "male" },
    ],

    // ── Screenshots ──────────────────────────────────────────────────────
    get_screenshot_config: () => ({
      enabled: true, interval_secs: 300, ocr_enabled: true, max_storage_mb: 5000,
    }),
    get_screenshot_metrics: () => ({
      total_screenshots: 1284,
      total_size_mb: 2340,
      ocr_processed: 1200,
      embedded: 1100,
    }),

    // ── Embeddings ───────────────────────────────────────────────────────
    list_embedding_models: () => [
      { id: "neurogpt-base", name: "NeuroGPT Base", size_mb: 245, loaded: true },
    ],
    list_embedding_sessions: () => {
      const now = ${Date.now() / 1000 | 0};
      return [
        { start_utc: now - 7200, end_utc: now - 3600, n_epochs: 720, day: "20260318" },
        { start_utc: now - 93600, end_utc: now - 90000, n_epochs: 720, day: "20260317" },
        { start_utc: now - 180000, end_utc: now - 176400, n_epochs: 720, day: "20260316" },
        { start_utc: now - 266400, end_utc: now - 262800, n_epochs: 540, day: "20260315" },
      ];
    },

    // ── Sleep ────────────────────────────────────────────────────────────
    get_sleep_config: () => ({ enabled: true, bedtime_hour: 23, waketime_hour: 7 }),

    // ── UMAP ─────────────────────────────────────────────────────────────
    get_umap_config: () => ({
      n_neighbors: 15, min_dist: 0.1, n_components: 3, metric: "cosine",
    }),

    // ── DND ──────────────────────────────────────────────────────────────
    get_dnd_config: () => ({ enabled: false, schedule: [] }),

    // ── Permissions ──────────────────────────────────────────────────────
    get_active_window_tracking: () => true,
    get_input_activity_tracking: () => true,

    // ── OpenBCI ──────────────────────────────────────────────────────────
    get_openbci_config: () => ({ port: "", baud_rate: 115200, channels: 8 }),

    // ── Device API ───────────────────────────────────────────────────────
    get_device_api_config: () => ({ ws_enabled: false, ws_port: 9001, http_enabled: false, http_port: 9002 }),

    // ── WS config ────────────────────────────────────────────────────────
    get_ws_config: () => ({ enabled: false, port: 9001 }),

    // ── Logging ──────────────────────────────────────────────────────────
    get_log_config: () => ({ level: "info", file_logging: true }),

    // ── Update check ─────────────────────────────────────────────────────
    get_update_check_interval: () => 86400,

    // ── Autostart ────────────────────────────────────────────────────────
    get_autostart_enabled: () => false,

    // ── Goals ────────────────────────────────────────────────────────────
    get_daily_goal: () => ({ target_mins: 60, notified_today: false }),

    // ── LLM logs ─────────────────────────────────────────────────────────
    get_llm_logs: () => [],

    // ── Model hardware fit ───────────────────────────────────────────────
    get_model_hardware_fit: () => [],

    // ── Session params ───────────────────────────────────────────────────
    get_session_params: () => ({ auto_record: true }),

    // ── Composite scores (dashboard) ─────────────────────────────────────
    get_composite_scores: () => ({
      meditation: 0.62, focus: 0.71, drowsiness: 0.15, cognitive_load: 0.45,
    }),

    // ── What's New ───────────────────────────────────────────────────────
    get_whats_new_version: () => "0.0.42",
    dismiss_whats_new: () => null,

    // ── Onboarding ───────────────────────────────────────────────────────
    complete_onboarding: () => null,
    get_onboarding_model_download_order: () => ["zuna", "llm", "ocr"],
    check_screen_recording_permission: () => true,
    check_ocr_models_ready: () => true,
    download_ocr_models: () => true,

    // ── Catch-all open_* window commands (no-op in screenshot mode) ──────
    open_settings_window: () => null,
    open_help_window: () => null,
    open_history_window: () => null,
    open_label_window: () => null,
    open_labels_window: () => null,
    open_search_window: () => null,
    open_compare_window: () => null,
    open_downloads_window: () => null,
    open_calibration_window: () => null,
    open_focus_timer_window: () => null,
    open_onboarding_window: () => null,
    open_updates_window: () => null,
    open_api_window: () => null,
    open_whats_new_window: () => null,
    open_bt_settings: () => null,
    open_chat_window: () => null,
    open_session_window: () => null,
    open_model_tab: () => null,
    open_skill_dir: () => null,

    // ── Subscription stubs ───────────────────────────────────────────────
    subscribe_eeg: () => null,
    subscribe_ppg: () => null,
    subscribe_imu: () => null,

    // ── Misc set/action stubs ────────────────────────────────────────────
    set_preferred_device: () => null,
    retry_connect: () => null,
    cancel_retry: () => null,
    set_goal_notified_date: () => null,
    set_screenshot_config: () => ({ changed: false }),
    set_filter_config: () => null,
    set_eeg_model_config: () => null,
    set_hooks: () => null,
    set_neutts_config: () => null,
    set_llm_config: () => null,
    set_sleep_config: () => null,
    set_umap_config: () => null,
    set_dnd_config: () => null,
    set_log_config: () => null,
    set_openbci_config: () => null,
    set_device_api_config: () => null,
    set_ws_config: () => null,
    set_daily_goal: () => null,
    set_active_window_tracking: () => null,
    set_input_activity_tracking: () => null,
    set_autostart_enabled: () => null,
    set_update_check_interval: () => null,
    set_active_calibration: () => null,
    set_session_params: () => null,
    set_tts_preload: () => null,
    set_data_dir: () => null,
    set_embedding_model: () => null,
    set_embedding_overlap: () => null,
    set_llm_active_mmproj: () => null,
    tts_init: () => null,
    tts_set_voice: () => null,

    // ── Sleep stages ─────────────────────────────────────────────────────
    get_sleep_stages: () => ({ epochs: [], summary: null }),

    // ── Screenshots dir ──────────────────────────────────────────────────
    get_screenshots_dir: () => ["/data/screenshots", 0],

    // ── Session lookup ───────────────────────────────────────────────────
    find_session_for_timestamp: () => ({ csv_path: "/data/session_20260318_120000.csv" }),
    open_compare_window_with_sessions: () => null,

    // ── Search screenshots ───────────────────────────────────────────────
    search_screenshots_by_text: () => [
      { timestamp: 20260318120000, unix_ts: ${Date.now() / 1000 - 3600 | 0}, filename: "20260318/20260318120000.webp", app_name: "VS Code", window_title: "main.rs — skill", ocr_text: "fn main() { let config = load_config();", similarity: 0.92 },
      { timestamp: 20260318113000, unix_ts: ${Date.now() / 1000 - 5400 | 0}, filename: "20260318/20260318113000.webp", app_name: "Firefox", window_title: "Rust Documentation", ocr_text: "The Rust Programming Language — structs and enums", similarity: 0.85 },
      { timestamp: 20260318110000, unix_ts: ${Date.now() / 1000 - 7200 | 0}, filename: "20260318/20260318110000.webp", app_name: "Terminal", window_title: "cargo build", ocr_text: "Compiling skill-screenshots v0.0.1", similarity: 0.78 },
    ],
    get_screenshots_around: () => [],
    estimate_screenshot_reembed: () => null,
    rebuild_screenshot_embeddings: () => null,

    // ── Jobs ─────────────────────────────────────────────────────────────
    poll_job: () => ({ status: "complete", result: null }),
    enqueue_umap_compare: () => ({ job_id: "mock" }),

    // ── Pair/forget ──────────────────────────────────────────────────────
    pair_device: () => [],
    forget_device: () => ({}),

    // ── LLM model management ─────────────────────────────────────────────
    download_llm_model: () => null,
    delete_llm_model: () => null,
    cancel_llm_download: () => null,
    pause_llm_download: () => null,
    resume_llm_download: () => null,
    switch_llm_model: () => null,
    refresh_llm_catalog: () => null,
    start_llm_server: () => null,
    stop_llm_server: () => null,

    // ── Label CRUD ───────────────────────────────────────────────────────
    submit_label: () => null,
    update_label: () => null,
    delete_label: () => null,
    reembed_all_labels: () => null,

    // ── Calibration CRUD ─────────────────────────────────────────────────
    create_calibration_profile: () => ({ id: 99, name: "New" }),
    update_calibration_profile: () => null,
    delete_calibration_profile: () => null,
    record_calibration_completed: () => null,
    emit_calibration_event: () => null,

    // ── Chat CRUD ────────────────────────────────────────────────────────
    load_chat_session: () => ({ id: 1, title: "Demo", messages: [], created_at: ${Date.now() / 1000 | 0}, archived: false }),
    save_chat_message: () => null,
    save_chat_tool_calls: () => null,
    rename_chat_session: () => null,
    delete_chat_session: () => null,
    archive_chat_session: () => null,
    unarchive_chat_session: () => null,
    abort_llm_stream: () => null,
    cancel_tool_call: () => null,
    chat_completions_ipc: () => null,

    // ── Session CRUD ─────────────────────────────────────────────────────
    delete_session: () => null,
    open_session_for_timestamp: () => null,

    // ── TTS ──────────────────────────────────────────────────────────────
    tts_speak: () => null,
    tts_unload: () => null,

    // ── DND ──────────────────────────────────────────────────────────────
    test_dnd: () => null,

    // ── Weights ──────────────────────────────────────────────────────────
    trigger_weights_download: () => null,
    cancel_weights_download: () => null,

    // ── Connect OpenBCI ──────────────────────────────────────────────────
    connect_openbci: () => null,

    // ── Suggest hooks ────────────────────────────────────────────────────
    suggest_hook_keywords: () => [],
    suggest_hook_distances: () => ({ min: 0.1, max: 0.5, avg: 0.3 }),

    // ── Permissions ──────────────────────────────────────────────────────
    open_accessibility_settings: () => null,
    open_notifications_settings: () => null,
    open_screen_recording_settings: () => null,

    // ── Window close ─────────────────────────────────────────────────────
    close_calibration_window: () => null,
    close_label_window: () => null,

    // ── Missing mocks ─────────────────────────────────────────────────────

    // API page
    get_ws_port: () => 9001,
    get_ws_clients: () => [
      { peer: "127.0.0.1:52341", connected_at: ${Date.now() / 1000 - 120 | 0} },
      { peer: "192.168.1.42:58302", connected_at: ${Date.now() / 1000 - 3600 | 0} },
    ],
    get_ws_request_log: () => [
      { timestamp: ${Date.now() / 1000 - 10 | 0}, peer: "127.0.0.1:52341", command: "status", ok: true },
      { timestamp: ${Date.now() / 1000 - 25 | 0}, peer: "127.0.0.1:52341", command: "search", ok: true },
      { timestamp: ${Date.now() / 1000 - 60 | 0}, peer: "192.168.1.42:58302", command: "label", ok: true },
      { timestamp: ${Date.now() / 1000 - 90 | 0}, peer: "127.0.0.1:52341", command: "say", ok: true },
      { timestamp: ${Date.now() / 1000 - 180 | 0}, peer: "192.168.1.42:58302", command: "status", ok: false },
    ],

    // Session page
    list_sessions: () => {
      const now = ${Date.now() / 1000 | 0};
      return [
        { csv_path: "/data/session_20260318_120000.csv", session_start_utc: now - 7200, session_end_utc: now - 3600, device_name: "Muse 2 Demo", battery_pct: 72, total_samples: 921600 },
        { csv_path: "/data/session_20260317_080000.csv", session_start_utc: now - 93600, session_end_utc: now - 90000, device_name: "Muse 2 Demo", battery_pct: 85, total_samples: 921600 },
      ];
    },

    // Chat
    new_chat_session: () => ({ id: 99, title: "New Chat", messages: [], created_at: ${Date.now() / 1000 | 0}, archived: false }),

    // App version
    get_app_version: () => "0.0.42",

    // Goals
    get_goal_notified_date: () => null,
    get_daily_recording_mins: () => 42,

    // Labels
    get_recent_labels: () => [
      { id: 1, text: "Deep focus - coding", context: "Work", eeg_start: ${Date.now() / 1000 - 3600 | 0}, eeg_end: ${Date.now() / 1000 - 3000 | 0}, created_at: ${Date.now() / 1000 - 3600 | 0} },
      { id: 2, text: "Meditation", context: "Break", eeg_start: ${Date.now() / 1000 - 7200 | 0}, eeg_end: ${Date.now() / 1000 - 6600 | 0}, created_at: ${Date.now() / 1000 - 7200 | 0} },
    ],
    get_stale_label_count: () => 0,

    // Toast
    show_toast_from_frontend: () => null,

    // Dot/SVG save
    save_dot_file: () => null,
    save_svg_file: () => null,

    // UMAP compare
    compute_umap_compare: () => ({ points: [], labels: [] }),

    // Additional settings stubs
    get_data_dir: () => "/data",
    get_embedding_model: () => "neurogpt-base",
    get_embedding_overlap: () => 0.5,
    get_tts_preload: () => false,
    tts_list_voices: () => [],
    tts_get_voice: () => null,
    get_active_window: () => ({ app: "VS Code", title: "main.rs" }),
    get_last_input_activity: () => ${Date.now() / 1000 | 0},
    get_dnd_active: () => false,
    list_serial_ports: () => [],
    list_focus_modes: () => [
      { id: "pomodoro", name: "Pomodoro", work_mins: 25, break_mins: 5 },
      { id: "deep_work", name: "Deep Work", work_mins: 50, break_mins: 10 },
    ],

    // Shortcuts
    get_api_shortcut: () => "CmdOrCtrl+Shift+A",
    get_calibration_shortcut: () => "CmdOrCtrl+Shift+C",
    get_focus_timer_shortcut: () => "CmdOrCtrl+Shift+F",
    get_help_shortcut: () => "CmdOrCtrl+Shift+H",
    get_history_shortcut: () => "CmdOrCtrl+Shift+I",
    get_label_shortcut: () => "CmdOrCtrl+Shift+L",
    get_search_shortcut: () => "CmdOrCtrl+Shift+S",
    get_settings_shortcut: () => "CmdOrCtrl+,",
    get_theme_shortcut: () => "CmdOrCtrl+Shift+T",

    // Whats new
    get_whats_new_seen_version: () => "0.0.42",

    // Archived chats
    list_archived_chat_sessions: () => [],

    // Hook log count
    get_hook_log_count: () => 0,

    // Permissions
    check_accessibility_permission: () => true,

    // Pick file
    pick_ref_wav_file: () => null,

    // Session params
    open_session_for_timestamp: () => null,

    // X window (Linux only)
    open_x_window: () => null,

    // Additional embedding/model stubs
    get_eeg_model_config: () => ({ model_id: "neurogpt-base", quantize: false, gpu_layers: 0 }),
  };

  if (MOCKS[cmd]) return MOCKS[cmd](args);

  // Unknown command — log and return null to avoid crashes
  console.warn("[screenshot-mock] unhandled invoke:", cmd, args);
  return null;
};

// ── Mock notification API ──────────────────────────────────────────────────
window.__TAURI_PLUGIN_NOTIFICATION__ = {
  isPermissionGranted: async () => true,
  requestPermission: async () => "granted",
  sendNotification: () => {},
};
`;
}

export { buildTauriMock };
