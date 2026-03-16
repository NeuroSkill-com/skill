### Bugfixes

- **Windows CI: fix trademark character encoding**: Replace literal `™` (U+2122) with PowerShell escape `$([char]0x2122)` in the Windows release workflow to prevent `Unexpected token` parse errors.
