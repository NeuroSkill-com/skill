# TODO

- [ ] caching for crawled domains, or queries, with TTL and when to refresh. Maybe a unified list of domains and rules on when to refresh. Optimize for speed and precision.

- [ ] in the tool calling, allow to edit bash commands before they are executed.

- [ ] record SNR too, so we can filter out by it later.

- [x] tool-call self-healing: re-prompt the model when it emits a garbled tool call that cannot be parsed, injecting a corrective message with the raw output and asking it to re-emit in the correct format (use the existing multi-round loop).

- [x] parse `<function=name><parameter=key>value</parameter></function>` XML tool-call format (Llama-family models) in `extract_tool_calls()`.
