### Server

- **Daemon routes extraction**: Move iroh HTTP routes into `skill-daemon-routes` as the first non-stub extracted module, and document remaining extraction blockers in the crate and `docs/architecture.md`.

### CLI

- **Typed daemon client checks**: Migrate tokens, chat rename/cancel-tool, and device prefer/forget/retry/status call sites off `daemonInvoke`; fix `tokens.ts` ACL wire types to snake_case; add `npm run check:typed-daemon-clients`.

### Docs

- **LLM and hooks ownership docs**: Rewrite `docs/LLM.md` and `docs/HOOKS.md` for RLX-in-daemon ownership, correct the `docs/AI.md` llama.cpp claim, and align Node prerequisite docs to >=20 (`docs/DEVELOPMENT.md`, `package.json` engines).

### Build

- **Workspace cleanup**: Remove duplicate `Cargo.toml` workspace members (`skill-daemon`, `skill-daemon-common`).
