# macOS Desktop Widgets — Developer Guide

## Overview

NeuroSkill ships 12 native macOS desktop widgets via a WidgetKit extension. Widgets query the skill-daemon HTTP API on `localhost:18444` and display brain metrics, session status, calendar correlations, and biometric data directly on the desktop.

## Architecture

```
┌──────────────────────────────────────────────────────┐
│  macOS Desktop / Notification Center                 │
│  ┌──────────┐ ┌──────────┐ ┌────────────────────┐   │
│  │ Focus    │ │ Streak   │ │ Brain Dashboard    │   │
│  │ (small)  │ │ (small)  │ │ (medium)           │   │
│  └────┬─────┘ └────┬─────┘ └────────┬───────────┘   │
│       └─────────────┴───────────────┘                │
│                     │                                │
│           SkillWidgets.appex                         │
│           (WidgetKit extension)                      │
│                     │                                │
│            HTTP GET/POST to                          │
│            localhost:18444                            │
│                     │                                │
│           ┌─────────▼─────────┐                      │
│           │  skill-daemon     │                      │
│           │  (background)     │                      │
│           └───────────────────┘                      │
└──────────────────────────────────────────────────────┘
```

### Data flow

1. WidgetKit calls the `TimelineProvider.getTimeline()` method on a system-controlled schedule
2. The provider calls `DaemonClient.shared.fetchSnapshot()` which hits the daemon REST API
3. On success, the snapshot is cached to `~/.config/skill/daemon/widget-cache.json`
4. On failure, cached data is returned with a stale indicator
5. The provider returns a `Timeline` with a refresh policy (5-30 min depending on widget)

### Auth

The widget reads the daemon auth token from `~/.config/skill/daemon/auth.token` and sends it as `Authorization: Bearer <token>` on every request. The entitlements file grants read access to `~/.config/skill/daemon/` via a temporary sandbox exception.

## Widget Catalog

### Small (`.systemSmall`)

| Widget | Data source | Refresh |
|--------|------------|---------|
| **Focus Score** | `/v1/brain/flow-state` | 5m |
| **Deep Work Streak** | `/v1/brain/streak` | 10m |
| **Break Timer** | `/v1/brain/break-timing` + fatigue | 5m |
| **Session Status** | `/v1/api/status` | 5m |
| **Optimal Hours** | `/v1/brain/optimal-hours` | 30m |
| **Heart Rate** | `/v1/analysis/metrics` | 5m |
| **Cognitive Load** | `/v1/brain/cognitive-load` | 10m |

### Medium (`.systemMedium`)

| Widget | Data source | Refresh |
|--------|------------|---------|
| **Brain Dashboard** | flow + fatigue + streak + status | 5m |
| **Daily Report** | `/v1/brain/daily-report` | 15m |
| **Weekly Trend** | streak `daily_history` | 1h |
| **EEG Band Power** | `/v1/analysis/metrics` (band powers) | 5m |
| **Sleep Quality** | `/v1/analysis/sleep` | 1h |

### Large (`.systemLarge`)

| Widget | Data source | Refresh |
|--------|------------|---------|
| **Calendar Mind State** | calendar events + meeting recovery + optimal hours | 15m |

## Development

### Prerequisites

- Xcode 15+ with macOS 14+ SDK
- XcodeGen: `brew install xcodegen`
- Node.js / npm (for the Tauri dev flow)

### Quick start

```bash
# Build widgets and run tests
cd extensions/widgets
./build-widgets.sh --test

# Or via the CI suite
bash scripts/test-all.sh widgets
```

### Integrated dev flow

Widgets build automatically when you run the Tauri dev or build commands:

```bash
# Dev mode — builds widgets, starts daemon, launches Tauri app
npm run tauri dev

# Production build — builds widgets (release), builds Tauri, embeds .appex
npm run tauri build
```

The `scripts/tauri-build.js` wrapper:
1. Builds the widget extension before launching Tauri
2. After a successful `tauri build`, copies `SkillWidgets.appex` into `NeuroSkill.app/Contents/PlugIns/`

### Xcode previews

For visual iteration without running the daemon:

```bash
cd extensions/widgets
xcodegen generate
open SkillWidgets.xcodeproj
```

Every widget file has `#Preview` blocks with placeholder data. Open any widget Swift file and use Xcode's Canvas preview pane.

### Manual build

```bash
cd extensions/widgets

# Debug (ad-hoc signed)
./build-widgets.sh

# Release (Developer ID signed)
./build-widgets.sh --release --sign "Developer ID Application: Your Name (TEAMID)"

# Build + embed into app bundle
./build-widgets.sh --embed /path/to/NeuroSkill.app

# Build + run tests
./build-widgets.sh --test
```

## Testing

### Automated (93 tests)

```bash
# Run all widget tests
cd extensions/widgets
xcodegen generate
xcodebuild test -project SkillWidgets.xcodeproj -scheme SkillWidgetsTests \
  -configuration Debug -derivedDataPath .build \
  -arch "$(uname -m)" ONLY_ACTIVE_ARCH=YES CODE_SIGN_IDENTITY="-"
```

**Test suites:**

| Suite | Count | What it tests |
|-------|-------|---------------|
| `ModelDecodingTests` | 15 | JSON decode fidelity for all API response types (snake_case mapping, optionals, arrays) |
| `WidgetSnapshotTests` | 22 | Entry computed properties, connectivity states, derived values, edge cases |
| `WidgetViewTests` | 56 | Every view renders without crash in all states (online, offline, empty, edge cases, shared components) |

### Manual testing

#### Testing different states

| State | How to trigger |
|-------|---------------|
| Online + recording | Connect a device and start a session |
| Online + idle | App running, no session active |
| Offline (cached) | Quit the app — widgets show last-known data with "Xm ago" |
| Offline (no cache) | `rm ~/.config/skill/daemon/widget-cache.json` then quit |
| Token missing | `mv ~/.config/skill/daemon/auth.token ~/.config/skill/daemon/auth.token.bak` |
| API error | Stop the daemon but keep the token file |
| Deep links | Tap any widget — should open the app to the correct page |

#### Force-refresh widgets

```bash
# Kill the widget process (system respawns it)
killall SkillWidgets
```

#### Inspect widget logs

```bash
# Stream widget extension logs
log stream --predicate 'subsystem == "com.neuroskill.skill.widgets"' --level debug
```

## File Structure

```
extensions/widgets/
├── Sources/
│   ├── SkillWidgetBundle.swift        # Entry point — registers all 12 widgets
│   ├── DesignSystem.swift             # Colors, ArcGauge, StatusPill, MetricRow, etc.
│   ├── Models.swift                   # Codable models matching daemon API
│   ├── DaemonClient.swift             # HTTP client for daemon REST API
│   ├── SnapshotCache.swift            # File-based data caching for offline fallback
│   ├── InteractiveIntents.swift       # AppIntent buttons (Start/Stop Session, Take Break)
│   ├── ConfigurableWidget.swift       # AppIntent-configured metric picker widget
│   ├── FocusWidget.swift              # Focus Score widget
│   ├── StreakWidget.swift             # Deep Work Streak widget
│   ├── BreakTimerWidget.swift         # Break Timer widget
│   ├── SessionStatusWidget.swift      # Session Status widget
│   ├── OptimalHoursWidget.swift       # Optimal Hours widget
│   ├── HeartRateWidget.swift          # Heart Rate / HRV widget
│   ├── CognitiveLoadWidget.swift      # Cognitive Load widget
│   ├── BrainDashWidget.swift          # Brain Dashboard widget
│   ├── DailyReportWidget.swift        # Daily Report widget
│   ├── WeeklyTrendWidget.swift        # Weekly Trend widget
│   ├── CalendarMindWidget.swift       # Calendar Mind State widget
│   ├── BandPowerWidget.swift          # EEG Band Power widget
│   ├── SleepWidget.swift              # Sleep Quality widget
│   ├── {en,de,es,fr,he,ja,ko,uk,zh-Hans}.lproj/  # 9 locales
│   ├── Assets.xcassets/               # App icon, colors
│   ├── Info.plist                     # Extension metadata
│   ├── SkillWidgets.entitlements      # Release entitlements (with App Group)
│   └── SkillWidgets.debug.entitlements # Debug entitlements (without App Group)
├── Tests/
│   ├── ModelDecodingTests.swift       # API response decoding tests
│   ├── WidgetSnapshotTests.swift      # Entry logic and computed property tests
│   └── WidgetViewTests.swift          # View instantiation smoke tests
├── project.yml                        # XcodeGen project spec
├── build-widgets.sh                   # Build script (build, test, embed, sign)
└── .gitignore                         # Ignores generated .xcodeproj and .build/
```

## Adding a New Widget

### 1. Create the widget file

Copy the pattern from an existing widget (e.g., `FocusWidget.swift`). Every widget needs:

- A `TimelineProvider` with `placeholder`, `getSnapshot`, `getTimeline`
- A `TimelineEntry` struct with `daemonOnline: Bool` and `error: WidgetError?`
- A SwiftUI `View` that handles online, offline, and empty states
- A `Widget` definition with `kind`, `configurationDisplayName`, `description`, `supportedFamilies`
- `#Preview` blocks for Xcode Canvas

### 2. Register it

Add the widget to `SkillWidgetBundle.swift`. The bundle supports up to 12 widgets directly.

### 3. Add API methods

If the widget needs new daemon data, add a method to `DaemonClient.swift` and a `Codable` model to `Models.swift`.

### 4. Localize

Add strings to all 9 `.lproj/Localizable.strings` files. Use the `L("key")` helper in views.

### 5. Test

Add tests to:
- `ModelDecodingTests.swift` — JSON decode with snake_case sample
- `WidgetSnapshotTests.swift` — entry computed properties
- `WidgetViewTests.swift` — view renders in online, offline, empty states

### 6. Build and verify

```bash
./build-widgets.sh --test
```

## Deep Links

Widgets use the `neuroskill://` URL scheme. Tapping a widget opens the app to the relevant section.

| URL | App route |
|-----|-----------|
| `neuroskill://dashboard` | `/` (main dashboard) |
| `neuroskill://devices` | `/settings` |
| `neuroskill://activity` | `/history` |
| `neuroskill://session` | `/session` |
| `neuroskill://heart-rate` | `/` |
| `neuroskill://settings` | `/settings` |
| `neuroskill://calibration` | `/calibration` |
| `neuroskill://focus-timer` | `/focus-timer` |

The handler is in `src-tauri/src/deeplink.rs`, wired via the single-instance plugin callback.

## Widget Reload

The Tauri app triggers `WidgetCenter.shared.reloadAllTimelines()` when:
- A session starts (`start_session`)
- A session stops (`cancel_session_sync`)
- A device reconnects (`retry_connect`)

Implementation: `src-tauri/src/widget_reload.rs` — uses objc2 to call `WGWidgetCenter` via runtime class lookup. Weak-linked via `build.rs` so it's a no-op on macOS < 14.

## Entitlements

**Debug** (`SkillWidgets.debug.entitlements`):
- App sandbox
- Network client (HTTP to daemon)
- Temporary file exception for `~/.config/skill/daemon/`

**Release** (`SkillWidgets.entitlements`):
- All of the above, plus:
- App Group `group.com.neuroskill.skill` (requires provisioning profile)

## Code Signing

For development: ad-hoc signing (`CODE_SIGN_IDENTITY="-"`) — no provisioning profile needed.

For distribution: use `--sign` flag with your Developer ID:
```bash
./build-widgets.sh --release --sign "Developer ID Application: NeuroSkill Inc (TEAMID)"
```

The App Group entitlement in the release entitlements file requires a provisioning profile with the `group.com.neuroskill.skill` App Group registered in the Apple Developer portal.
