### Features

- **Implicit evidence collection for protocols**: Every protocol execution now produces structured measurement data using a standardised `px:start:`/`px:end:` label schema with pipe-separated metrics (bar, stress_index, relaxation, focus, hr, mood, faa, rmssd, deltas, outcome). The LLM captures before/after snapshots automatically and silently for every protocol, determining outcome as positive (≥10% target improvement), neutral, or negative. No user action required — evidence collection is invisible infrastructure.

- **Personal protocol effectiveness ranking**: After 5+ labeled protocol executions, the LLM aggregates outcomes via `search_labels "px:end"` to build a personal ranking by success rate and average metric delta. Surfaces time-of-day patterns, trigger-specific effectiveness, and modality preferences. Presents insights like "Cold water face splash is your most effective stress intervention — 92% success, average stress drop 31%."

- **Evidence-driven protocol selection**: New matching guidance rule — "Evidence first." Before suggesting any protocol, the LLM checks past effectiveness data. Leads with proven personal winners over generic recommendations. Retires consistent failures after 4+ negative outcomes. Explores new protocols occasionally even with strong data. Tracks modality preferences across interventions.

- **Implicit life-event labeling**: The LLM silently labels mentioned life events (caffeine intake, walks, meals, meetings, exercise, app switches, sleep quality) with EEG metric snapshots to build a complete personal effectiveness map beyond formal protocols. Privacy safeguards: full transparency if asked, user owns all local data, no inference of unmentioned events.
