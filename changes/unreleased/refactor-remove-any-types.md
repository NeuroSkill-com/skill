### Refactor

- **Remove `any` types from core interfaces**: Replaced all `any` annotations in `chat-types.ts`, `search-types.ts`, and `chat-utils.ts` with proper types (`Record<string, unknown>`, `unknown`, discriminated union `ContentPart`). Added `typeof` guards in `detectToolDanger` for safe property access. Added explicit casts at `JobPollResult` consumption sites in search and compare pages.
