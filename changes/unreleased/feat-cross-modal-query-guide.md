### LLM

- **Cross-modal query guide in tool description**: Added a decision guide to the `skill` tool description that maps question patterns to the correct command and direction (e.g. "What was on screen during EEG?" → `screenshots_for_eeg`, "How was my brain when I saw X?" → `eeg_for_screenshots`). This helps the LLM pick the right cross-modal bridging command instead of guessing.

### Docs

- **Cross-modal workflow examples in skills**: Replaced the bash-only Cross-Modal Workflows section in the screenshots SKILL.md with a direction-based query guide table and multi-step LLM tool-call examples (JSON). Added Cross-Modal Follow-Ups sections to the search and labels SKILL.md files showing how to chain commands across modalities.
