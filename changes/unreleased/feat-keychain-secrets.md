### Features

- **System keychain for secrets**: API tokens and device credentials (`api_token`, Emotiv client ID/secret, IDUN API token) are now stored in the OS credential store (macOS Keychain, Linux Secret Service, Windows Credential Manager) instead of plaintext in `settings.json`. Secrets survive app reinstalls and build updates. Existing plaintext values are automatically migrated to the keychain on first launch and stripped from the JSON file.
