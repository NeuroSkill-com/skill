# Overview
Proactive Hooks let the app automatically trigger actions when your recent EEG patterns match specific keywords or brain states.

## What are Proactive Hooks?
A Proactive Hook is a rule that monitors your recent EEG label embeddings in real time. When the cosine distance between your recent brain-state embeddings and the hook's keyword embeddings drops below a configured threshold, the hook fires — sending a command, displaying a notification, triggering TTS, or broadcasting a WebSocket event. Hooks let you build closed-loop neuro-feedback automations without writing code.

## How It Works
Every few seconds the app computes EEG embeddings from your most recent brain data. These are compared against the keyword embeddings defined in each active hook using cosine similarity over the HNSW index. If any hook's distance threshold is met, the hook fires. A cooldown prevents the same hook from firing repeatedly in rapid succession. The matching is purely local — no data leaves your machine.

## Scenarios
Each hook can be scoped to a scenario — Cognitive, Emotional, Physical, or Any. Cognitive hooks target mental states like focus, distraction, or mental fatigue. Emotional hooks target affective states like stress, calm, or frustration. Physical hooks target bodily states like drowsiness or physical fatigue. 'Any' matches regardless of the inferred scenario category.

# Configuring a Hook
Each hook has several fields that control when and how it fires.

## Hook Name
A descriptive name for the hook (e.g. 'Deep Work Guard', 'Calm Recovery'). The name is used in the history log and WebSocket events. It must be unique across all hooks.

## Keywords
One or more keywords or short phrases that describe the brain state you want to detect (e.g. 'focus', 'deep work', 'stress', 'tired'). These are embedded using the same sentence-transformer model as your EEG labels. The hook fires when recent EEG embeddings are close to these keyword embeddings in the shared vector space.

## Keyword Suggestions
As you type a keyword, the app suggests related terms from your existing label history using both fuzzy string matching and semantic embedding similarity. Suggestions show a source badge — 'fuzzy' for string-based matches, 'semantic' for embedding-based matches, or 'fuzzy+semantic' for both. Use ↑/↓ arrow keys and Enter to quickly accept a suggestion.

## Distance Threshold
The maximum cosine distance (0–1) between recent EEG embeddings and the hook's keyword embeddings for the hook to fire. Lower values require a closer match (more strict), higher values fire more often (more lenient). Typical values range from 0.08 (very strict) to 0.25 (loose). Start around 0.12–0.16 and tune based on the suggestion tool.

## Distance Suggestion Tool
Click 'Suggest threshold' to analyse your recorded EEG data against the hook's keywords. The tool computes the distance distribution (min, p25, p50, p75, max) and recommends a threshold that balances sensitivity and specificity. A visual percentile bar shows where your current and suggested thresholds fall in the distribution. Click 'Apply' to use the suggested value.

## Recent Refs
The number of most-recent EEG embedding samples to compare against the hook's keywords (default: 12). Higher values smooth out transient spikes but increase detection latency. Lower values react faster but may fire on brief artifacts. Valid range: 10–20.

## Command
An optional command string broadcast in the WebSocket event when the hook fires (e.g. 'focus_reset', 'calm_breath'). External automation tools listening on the WebSocket can react to this command to trigger app-specific actions, notifications, or scripts.

## Payload Text
An optional human-readable message included in the hook's fire event (e.g. 'Take a 2-minute break.'). This text is shown in notifications and can be spoken aloud via TTS if voice guidance is enabled.

# Advanced
Tips, history, and integration with external tools.

## Quick Examples
The 'Quick examples' panel provides ready-made hook templates for common use cases: Deep Work Guard (cognitive focus reset), Calm Recovery (emotional stress relief), and Body Break (physical fatigue). Click any example to add it as a new hook with pre-filled keywords, scenario, threshold, and payload. Adjust the values to match your personal EEG patterns.

## Hook Fire History
The collapsible history log at the bottom of the Hooks panel records every hook fire event with timestamp, matched label, cosine distance, command, and keywords at the time of firing. Use it to audit hook behaviour, verify thresholds, and debug false positives. Expand any row to see full details. Pagination controls let you browse older events.

## WebSocket Events
When a hook fires, the app broadcasts a JSON event over the WebSocket API containing the hook name, command, text, matched label, distance, and timestamp. External clients can listen for these events to build custom automations — for example, dimming lights, pausing music, sending a Slack message, or logging to a personal dashboard.

## Tuning Tips
Start with one hook and a few keywords that match labels you have already recorded. Use the distance suggestion tool to set an initial threshold. Monitor the history log for a day and adjust: lower the threshold if you see false positives, raise it if the hook never fires. Adding more specific keywords (e.g. 'deep focus reading' vs. 'focus') generally improves precision. Avoid very short or generic single-word keywords unless you want broad matching.
