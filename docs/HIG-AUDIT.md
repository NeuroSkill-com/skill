# Apple Human Interface Guidelines (HIG) Audit

> Audit date: 2026-03-28  
> Scope: NeuroSkill v0.0.74 — Tauri + Svelte macOS app

---

## ✅ Fixed in This Pass

### 1. Reduced Motion (`prefers-reduced-motion`)
**HIG: [Motion](https://developer.apple.com/design/human-interface-guidelines/motion)**  
Added `@media (prefers-reduced-motion: reduce)` rule in `app.css` that disables all CSS animations and transitions when the user has "Reduce motion" enabled in System Settings → Accessibility → Display.

### 2. Increased Contrast (`prefers-contrast`)
**HIG: [Accessibility — Color and contrast](https://developer.apple.com/design/human-interface-guidelines/accessibility#Color-and-contrast)**  
Added `@media (prefers-contrast: more)` rule that strengthens border colors when macOS "Increase contrast" is enabled, complementing the existing manual high-contrast toggle.

### 3. System Font Fallback
**HIG: [Typography](https://developer.apple.com/design/human-interface-guidelines/typography)**  
Added `-apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui` as fallback fonts after "Absans". If the custom font fails to load, the app falls back to the native system font.

### 4. Quit Dialog Button Labels
**HIG: [Alerts](https://developer.apple.com/design/human-interface-guidelines/alerts)**  
Changed quit confirmation from generic "Yes"/"No" to action-verb buttons ("Quit"/"Cancel") in all supported languages. HIG mandates that alert buttons describe the action, not just affirm/deny.

### 5. Confirm Action Button Order  
**HIG: [Alerts — Button placement](https://developer.apple.com/design/human-interface-guidelines/alerts)**  
Reordered `ConfirmAction` component so **Cancel** appears on the left (leading) and the **destructive action** appears on the right (trailing), matching macOS native alert conventions.

### 6. Toggle Switch Accessibility (`role="switch"`)
**HIG: [Toggles](https://developer.apple.com/design/human-interface-guidelines/toggles), [Accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility)**  
- Created reusable `ToggleSwitch` component (`src/lib/components/ui/toggle-switch/`) with proper `role="switch"` and `aria-checked` attributes.
- Added `role="switch"` + `aria-checked` to **all** toggle switches across the entire codebase:
  - `AppearanceTab` — high-contrast toggle
  - `GoalsTab` — DND enable + exit notification toggles
  - `CalibrationTab` — auto-start toggle
  - `SettingsTab` — auto-fit, active-window, input-activity, and all debug logging toggles
  - `TtsTab` — preload + TTS logging toggles
  - `UpdatesTab` — launch-at-login toggle
  - `LslTab` — auto-connect toggle
  - `ScreenshotToggleCard` — all 4 toggles (migrated to `ToggleSwitch` component)
  - `LlmServerSection`, `LlmInferenceSection`, `ChatToolsSection`, `AgentSkillsSection`, `SkillsRefreshSection` — already had `role="switch"` ✅

### 7. Toast Notification Position
**HIG: [Banners](https://developer.apple.com/design/human-interface-guidelines/notifications)**  
Moved toast notifications from top-right to top-center, matching macOS banner notification positioning.

---

## ⚠️ Remaining Recommendations (Not Yet Implemented)

### 8. Native Window Controls (Traffic Lights)
**HIG: [Windows — Anatomy](https://developer.apple.com/design/human-interface-guidelines/windows)**  
The app uses `decorations: false` with custom SVG close/maximize/minimize buttons. HIG mandates native macOS traffic lights (red/yellow/green). This is the **single largest HIG violation**.

**Recommendation:** On macOS, use Tauri's `titleBarStyle: "overlay"` or `"transparent"` with `hiddenTitle: true` to get native traffic lights while keeping custom titlebar content. The `CustomTitleBar.svelte` would only render center content and action buttons on macOS, not window controls.

### 9. Native System Font for UI
**HIG: [Typography](https://developer.apple.com/design/human-interface-guidelines/typography)**  
The custom "Absans" font makes the app feel non-native. Consider using the system font (`-apple-system`) as the primary body font, reserving "Absans" for branding elements (logo, headings).

### 10. Sidebar Navigation Pattern
**HIG: [Sidebars](https://developer.apple.com/design/human-interface-guidelines/sidebars)**  
The Settings page uses a sidebar navigation pattern which is good. However, HIG recommends using `NSTableView`-style selection highlighting (translucent accent-color capsule). The current active tab highlight could more closely match the native macOS sidebar selection style.

### 11. Segmented Controls
**HIG: [Segmented controls](https://developer.apple.com/design/human-interface-guidelines/segmented-controls)**  
The search mode switcher and history view mode buttons use custom segmented controls. HIG specifies that segments should have equal width and use the system accent color for selection. Current implementation is close but could match native metrics more precisely (28px height, 1px separator, system accent fill).

### 12. Sheets Instead of Separate Windows
**HIG: [Sheets](https://developer.apple.com/design/human-interface-guidelines/sheets)**  
Several secondary views (Labels, About, What's New) open as separate windows. HIG recommends using **sheets** (modal overlays attached to the parent window) for transient tasks. This would reduce window management overhead and feel more native.

### 13. Window Restoration
**HIG: [State restoration](https://developer.apple.com/design/human-interface-guidelines/launching#State-restoration)**  
HIG recommends apps restore window positions and sizes across launches. Consider persisting window geometry to `settings.json` and restoring on next launch.

### 14. Undo Support
**HIG: [Undo and redo](https://developer.apple.com/design/human-interface-guidelines/undo-and-redo)**  
The Edit menu includes Undo/Redo but these only apply to text fields. Consider adding undo support for destructive actions (delete session, delete label, delete calibration profile).

### 15. Haptic Feedback
**HIG: [Playing haptics](https://developer.apple.com/design/human-interface-guidelines/playing-haptics)**  
On MacBooks with Force Touch trackpads, provide haptic feedback for toggle switches, slider snapping, and destructive actions via `NSHapticFeedbackManager`.

---

### 8. Precise System Settings Deep Links
**HIG: Permissions should link directly to the relevant pane.**
- Updated all macOS `x-apple.systempreferences:` URLs to try macOS 13+ (Ventura) format first, with fallback to macOS 12- format:
  - Accessibility: `com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility`
  - Screen Recording: `com.apple.settings.PrivacySecurity.extension?Privacy_ScreenCapture`
  - Notifications: `com.apple.settings.Notifications`
  - Bluetooth: `com.apple.settings.Bluetooth`
- Added **3 new system settings commands**:
  - `open_calendar_settings` — opens Privacy → Calendars directly (for re-granting denied access)
  - `open_input_monitoring_settings` — opens Privacy → Input Monitoring (for keyboard/mouse tracking)
  - `open_focus_settings` — opens Focus / DND settings (macOS 13+: `com.apple.settings.Focus`)
- Improved Linux fallbacks to try KDE `systemsettings` when GNOME is unavailable
- Windows: fixed `ms-settings:` URLs to use proper `cmd /C start` pattern

---

## Files Modified

| File | Change |
|------|--------|
| `src/app.css` | Added `prefers-reduced-motion`, `prefers-contrast`, system font fallback |
| `src-tauri/src/quit.rs` | Action-verb buttons ("Quit"/"Cancel") instead of "Yes"/"No" |
| `src-tauri/src/window_cmds.rs` | Updated all macOS deep links to Ventura+ format with fallback; added `open_calendar_settings`, `open_input_monitoring_settings`, `open_focus_settings` |
| `src-tauri/src/lib.rs` | Registered new commands |
| `src/lib/components/ui/toggle-switch/ToggleSwitch.svelte` | New accessible toggle component |
| `src/lib/components/ui/toggle-switch/index.ts` | New export |
| `src/lib/components/ui/confirm-action/ConfirmAction.svelte` | Button order: Cancel left, Action right |
| `src/lib/components/ui/toast/toast-container.svelte` | Centered toast position |
| `src/lib/AppearanceTab.svelte` | Added `role="switch"` to high-contrast toggle |
| `src/lib/GoalsTab.svelte` | Added `role="switch"` to DND + exit notification toggles |
| `src/lib/CalibrationTab.svelte` | Added `role="switch"` to auto-start toggle |
| `src/lib/SettingsTab.svelte` | Added `role="switch"` to all 4 toggles (auto-fit, active-window, input-activity, logging) |
| `src/lib/TtsTab.svelte` | Added `role="switch"` to preload + logging toggles |
| `src/lib/UpdatesTab.svelte` | Added `role="switch"` to autostart toggle |
| `src/lib/LslTab.svelte` | Added `role="switch"` to auto-connect toggle |
| `src/lib/screenshots/ScreenshotToggleCard.svelte` | Migrated to `ToggleSwitch` component |
| `src/lib/PermissionsTab.svelte` | Added calendar settings deep-link button when permission denied |
