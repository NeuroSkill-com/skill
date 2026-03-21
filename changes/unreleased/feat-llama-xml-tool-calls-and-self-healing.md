### Features

- **Llama XML tool-call parsing**: `extract_tool_calls()` now recognises the `<function=name><parameter=key>value</parameter></function>` XML format emitted by Llama-family models. Both `<parameter>` tag pairs and inline JSON bodies are supported. Stripping and streaming partial-tag handling are also included.

- **Tool-call self-healing**: When the LLM emits a garbled or malformed tool call that cannot be parsed, the orchestrator now detects the failed attempt, injects a corrective message containing the raw output, and asks the model to re-emit in the correct format. Up to 2 retry attempts are made before falling back to normal output. This significantly improves reliability with smaller local models.

### LLM

- **`detect_garbled_tool_call()`**: New public function that identifies malformed tool-call attempts in assistant output (broken `[TOOL_CALL]` blocks, incomplete XML `<function=` tags, or unbalanced JSON with tool-call keys).
- **`build_self_healing_message()`**: New public helper that constructs the corrective user message for the retry loop.
