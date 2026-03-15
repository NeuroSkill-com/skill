### Docs

- **Update SKILL.md for new devices, tools, and screenshots**: Added "Supported Devices" section documenting Muse, OpenBCI Ganglion, MW75 Neuro (12ch), and Hermes V1 (8ch) with channel counts and sample rates. Updated `status` JSON example to show device-agnostic fields (`eeg_channels`, multi-device name examples, dynamic signal quality keys). Added "Built-in Tool Calling" section to the LLM docs covering bash, read/write/edit, web search/fetch tools with safety info. Added "Screenshots (UI-only Feature)" section documenting capture, CLIP vision embedding, OCR, and dual HNSW search. Updated table of contents numbering.

### CLI

- **Dynamic signal quality rendering**: `status` command now renders signal quality for any number of EEG channels (4/8/12) instead of hardcoding Muse's tp9/af7/af8/tp10 keys. Added device support note and tool-calling documentation to CLI header comment.
