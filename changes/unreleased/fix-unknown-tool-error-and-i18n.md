### Bugfixes

- **Unknown tool calls show misleading "disabled" error**: When the LLM hallucinates non-existent tool names (e.g. "status", "neuroskill-status"), the error now correctly says "unsupported tool" with guidance to use available tools, instead of the misleading "tool disabled in settings".

### UI

- **Tool card i18n fallback for unknown tools**: The chat tool card now shows the raw tool name instead of a raw i18n key (e.g. "status" instead of "chat.tools.status") when the LLM calls an unrecognized tool.
