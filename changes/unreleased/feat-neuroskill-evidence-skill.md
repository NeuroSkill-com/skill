### Features

- **neuroskill-evidence standalone skill**: New skill defining the implicit evidence collection and personal effectiveness engine used across all intervention-delivering skills. Includes the standardised `px:` label schema (`px:start`, `px:end`, `px:note`, `px:skip`, `px:auto`) with pipe-separated key=value context format, required fields (8 core EEG metrics), outcome determination rules by trigger type, mandatory before/after measurement flow, life-event implicit labeling (caffeine, meals, walks, meetings, sleep, exercise, app switches), hook trigger tracking, evidence aggregation by data maturity, personal protocol ranking algorithm (success_rate × avg_delta_target), 10 evidence-driven selection rules, evidence surfacing and tone guidelines, privacy safeguards, complete protocol name reference (100+ snake_case names), and a quick-reference integration pattern for other skills.

### Refactor

- **Extracted evidence collection from neuroskill-protocols**: Removed the ~190-line inline Evidence Collection section from the protocols skill and replaced it with a reference to the standalone neuroskill-evidence skill. The protocols skill now depends on the evidence skill for all measurement, labeling, and ranking rules, allowing any other skill to also follow the same evidence framework.
