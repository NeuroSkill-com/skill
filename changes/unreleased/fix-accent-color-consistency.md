### Bugfixes

- **Accent color consistency**: All UI elements now honor the Appearance accent setting. Replaced hardcoded `oklch(0.58 0.24 293)` violet values in the markdown renderer CSS with `var(--color-violet-*)` tokens, converted every `purple-*` Tailwind class to the remapped `violet-*` family, and switched inline-style hex accent colors (`#8b5cf6`, `#a855f7`, `#c084fc`) to CSS custom properties across dashboard gauges, focus timer, compare page, and EEG indices.
