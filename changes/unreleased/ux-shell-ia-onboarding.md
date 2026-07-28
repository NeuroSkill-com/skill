### UI

- **Primary app shell navigation**: Add a Live / Find / Ask / History destinations bar across main feature windows while keeping Add Label in the main titlebar.
- **Settings information architecture**: Group tabs by Signal, Intelligence, Capture & Privacy, Automation, and App, with a collapsible Advanced section and Devices as the default landing tab.
- **Live dashboard view modes**: Add Waveform / Physiology / State switching with progressive disclosure based on connected modality support.
- **Onboarding flow simplification**: Reduce core first-run path to connect -> fit -> tray -> done, require research-use acknowledgement, and move calibration/models/permissions to optional setup from Done.
- **Getting Started deep links**: Make checklist items open Devices, Calibration, Goals, Downloads, Search, and API targets directly.
- **Find UX updates**: Add example chips, prioritize Interactive query + Search action, and move pipeline/filters under Advanced with titlebar source labeling.
- **Command palette IA**: Add a Primary section for Live / Find / Ask / History and separate Add Label from Browse Labels actions.
- **Labels discoverability alignment**: Standardize Browse Labels naming for window title and `labels.openLabels`; change History toggle wording to "Show labels"; localize titlebar Help/Reload labels.
- **Shell discoverability improvements**: Show accelerator hints for Find / Ask / History and include shortcuts in tooltips.
- **Shell interaction fix**: Render primary nav inside `#main-content` so it is not blocked by the draggable titlebar region.
- **Chart accessibility**: Add hatch patterns plus active scheme colors on band-power tiles so differentiation does not rely on color alone.
- **Window-title consistency**: Open labels window as "Browse Labels" from Rust side and align `labels.title` text.

### i18n

- **Locale completion for new IA strings**: Translate shell, settings-group, live-view, onboarding, Find, and command-palette additions across de, es, fr, he, ja, ko, uk, zh and replace auto-synced English fallbacks.

### Bugfixes

- **Chat cancel handler shadowing**: Fix recursive symbol shadowing so chat tool cancel calls the daemon client correctly.
- **Tokens copy action typing**: Update Tokens tab Copy behavior to use typed token access and `common.copy` label wiring.
