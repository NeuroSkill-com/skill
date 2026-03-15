### Bugfixes

- **Fix clippy warnings and a11y lint in history page**: Resolved 9 clippy warnings — `let_and_return` in ws_commands.rs, `double_ended_iterator_last` and `collapsible_str_replace` in llm.rs, `iter_cloned_collect` in hermes/mw75 sessions, `needless_borrow` in settings_cmds.rs. Converted label dot `<span>` elements to `<button>` with `aria-label`, `onfocus`/`onblur` handlers for keyboard accessibility.
