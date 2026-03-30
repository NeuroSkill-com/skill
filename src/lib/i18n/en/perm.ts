// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** English "perm" namespace — reference translation. */
const perm: Record<string, string> = {
  "perm.intro":
    "{app} uses a small number of optional OS permissions to enable features like keyboard/mouse activity timestamps and notifications. All data stays on your device.",
  "perm.granted": "Granted",
  "perm.denied": "Not Granted",
  "perm.unknown": "Unknown",
  "perm.notRequired": "Not Required",
  "perm.systemManaged": "Managed by OS",
  "perm.accessibility": "Accessibility",
  "perm.accessibilityDesc":
    "Keyboard and mouse activity tracking uses a CGEventTap (macOS) to record timestamps of the last key press and mouse event. No keystrokes or cursor positions are stored — only Unix-second timestamps. This requires Accessibility permission on macOS.",
  "perm.accessibilityOk": "Permission granted. Keyboard and mouse activity timestamps are being recorded.",
  "perm.accessibilityPending": "Checking permission status…",
  "perm.howToGrant": "How to grant this permission:",
  "perm.accessStep1": 'Click "Open Accessibility Settings" below.',
  "perm.accessStep2": "Find {app} in the list (or click the + button to add it).",
  "perm.accessStep3": "Toggle it on.",
  "perm.accessStep4": "Return here — the status will update automatically.",
  "perm.openAccessibilitySettings": "Open Accessibility Settings",
  "perm.bluetooth": "Bluetooth",
  "perm.bluetoothDesc":
    "Bluetooth is used to connect to your BCI headset (Muse, MW75 Neuro, OpenBCI Ganglion, IDUN Guardian, etc.). On macOS, the system will prompt for Bluetooth access the first time the app scans. On Linux and Windows no separate permission is needed.",
  "perm.openBluetoothSettings": "Open Bluetooth Settings",
  "perm.notifications": "Notifications",
  "perm.notificationsDesc":
    "Notifications are used to alert you when you reach your daily recording goal, and when a software update is available. On macOS and Windows, the OS will prompt for permission the first time a notification is sent.",
  "perm.openNotificationsSettings": "Open Notification Settings",
  "perm.matrix": "Permission Summary",
  "perm.feature": "Feature",
  "perm.matrixBluetooth": "Bluetooth (BCI device)",
  "perm.matrixKeyboardMouse": "Keyboard & mouse timestamps",
  "perm.matrixActiveWindow": "Active window tracking",
  "perm.matrixNotifications": "Notifications",
  "perm.matrixNone": "No permission needed",
  "perm.matrixAccessibility": "Accessibility required",
  "perm.matrixOsPrompt": "OS prompts on first use",
  "perm.legendNone": "No permission needed",
  "perm.legendRequired": "OS permission required — degrades silently if absent",
  "perm.legendPrompt": "OS prompts on first use",
  "perm.why": "Why does {app} need these?",
  "perm.whyBluetooth": "Bluetooth",
  "perm.whyBluetoothDesc": "To discover and stream data from your BCI headset over BLE.",
  "perm.whyAccessibility": "Accessibility",
  "perm.whyAccessibilityDesc":
    "To timestamp keyboard and mouse events for activity context. Only the time of the event is stored — never what was typed or where the cursor was.",
  "perm.whyNotifications": "Notifications",
  "perm.whyNotificationsDesc": "To notify you when you hit your daily recording goal and when updates are ready.",
  "perm.privacyNote":
    "All data is stored locally on your device and is never transmitted to any server. You can disable any feature in Settings → Activity Tracking.",
  "perm.screenRecording": "Screen Recording",
  "perm.screenRecordingDesc":
    "Required to capture other application windows for the screenshot embedding system. macOS redacts window content without this permission.",
  "perm.screenRecordingOk": "Screen recording permission is granted. Screenshot capture will work correctly.",
  "perm.screenRecordingStep1": "Open System Settings → Privacy & Security → Screen & System Audio Recording",
  "perm.screenRecordingStep2": "Find NeuroSkill™ in the list and enable it",
  "perm.screenRecordingStep3": "You may need to quit and relaunch the app for the change to take effect",
  "perm.openScreenRecordingSettings": "Open Screen Recording Settings",
  "perm.whyScreenRecording": "Screen Recording",
  "perm.whyScreenRecordingDesc":
    "To capture the active window for visual-similarity search and cross-modal EEG correlation. Only opt-in screenshots are stored — never continuous recording.",
  "perm.matrixScreenRecording": "Screenshot capture",
  "perm.matrixScreenRecordingReq": "Screen Recording required",
  "perm.calendar": "Calendar",
  "perm.calendarDesc":
    "Calendar tools can read events for scheduling context. Permission is requested by macOS when needed.",
  "perm.requestCalendarPermission": "Request Calendar Permission",
  "perm.openCalendarSettings": "Open Calendar Privacy Settings",
  "perm.location": "Location Services",
  "perm.locationDesc":
    "On macOS, Location Services uses CoreLocation (GPS / Wi-Fi / cell) for high-accuracy positioning. On Linux and Windows the app uses IP-based geolocation which needs no permission. If Location Services is denied or unavailable, the app falls back to IP geolocation automatically.",
  "perm.locationOk": "Location permission granted. CoreLocation will be used for high-accuracy fixes.",
  "perm.locationFallback": "Location not authorized — the app will use IP-based geolocation (city-level accuracy).",
  "perm.locationStep1": "Open System Settings → Privacy & Security → Location Services",
  "perm.locationStep2": "Find {app} in the list and enable it",
  "perm.locationStep3": "Return here — the status will update automatically",
  "perm.requestLocationPermission": "Request Location Permission",
  "perm.openLocationSettings": "Open Location Settings",
  "perm.whyLocation": "Location",
  "perm.whyLocationDesc":
    "To provide precise location context to the LLM and store GPS fixes alongside health data. Falls back to IP geolocation if denied.",
  "perm.matrixLocation": "Location (GPS / IP)",
  "perm.matrixLocationReq": "Location Services (optional — falls back to IP)",
  "perm.openInputMonitoringSettings": "Open Input Monitoring Settings",
  "perm.openFocusSettings": "Open Focus Settings",
  "perm.whyCalendar": "Calendar",
  "perm.whyCalendarDesc":
    "To provide scheduling context to the LLM tools so the AI can reference your upcoming events.",
  "perm.matrixCalendar": "Calendar events",
  "perm.matrixCalendarReq": "Calendar access required",
};

export default perm;
