### i18n

- **Convert all Unicode escapes to literal UTF-8**: Replaced all `\uXXXX` escape sequences in i18n locale files (en, de, fr, uk, he) with their literal UTF-8 characters. This makes the translation strings human-readable in source and avoids encoding issues across platforms.
