### Features

- **VS Code extension joins the validation prompt loop**: new `ValidationManager` (`extensions/vscode/src/validation.ts`) polls `/v1/validation/should-prompt` every 90 s and renders whichever channel the daemon decides to fire.
  - **KSS** (1–9 sleepiness) — full QuickPick with Karolinska wording, plus Snooze 30m / Don't ask today / Stop these prompts escape hatches in-line. POSTs the answer to `/v1/validation/kss` echoing the daemon's `prompt_id` so the prompt log can mark it answered.
  - **NASA-TLX** — fallback path: shows an information message offering to open the form in the Tauri app (deep link `neuroskill://validation/tlx`) plus the same escape hatches.
  - **PVT** — weekly nudge: offers to deep-link into the Tauri PVT panel (`neuroskill://validation/pvt`) or skip a week (snoozes for 6 days so the next reminder fires on cadence).
- **Two new commands**: `NeuroSkill: Open Validation Settings…` (deep links into the Tauri preferences pane) and `NeuroSkill: Check for Validation Prompt Now` (forces a single scheduler poll — useful for opt-in onboarding).
- **`DaemonClient.patch()`**: extension's daemon client gained a PATCH method so the "Stop these prompts" escape hatch can flip `enabled = false` on the persistent config.

### i18n

- **22 new `validation.*` keys per bundle** in all 9 languages (`bundle.l10n.{de,en,es,fr,he,ja,ko,uk,zh-cn}.json`): KSS prompt + 9 score labels with translated Karolinska wording, TLX/PVT prompts, all four escape-hatch labels, the disabled-permanently acknowledgement.
- **Two new `cmd.*` keys** for the validation commands in `package.nls.json`.
