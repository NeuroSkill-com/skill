### Features

- **skill-headless: Headless / Headful modes**: Replaced the `visible: bool` flag with a `Mode` enum (`Mode::Headless` and `Mode::Headful`). Headless mode positions the window off-screen so nothing is ever shown to the user while still giving the webview real pixel dimensions. Headful mode shows the window on-screen for debugging, demos, or interactive automation. In headless mode, `SetViewport` ensures the window stays off-screen after resize.
