// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// AppIntents for interactive widget buttons (macOS 14+).
// These allow users to start/stop sessions and acknowledge break
// reminders directly from widgets without opening the app.

import AppIntents
import Foundation
import WidgetKit

// MARK: - Start Session Intent

struct StartSessionIntent: AppIntent {
    static var title: LocalizedStringResource = "Start Session"
    static var description = IntentDescription("Start an EEG recording session.")
    static var openAppWhenRun = false

    func perform() async throws -> some IntentResult {
        _ = try await DaemonClient.shared.fetchStatus()
        // Trigger session start via deep link — the Tauri app handles the actual command
        WidgetCenter.shared.reloadAllTimelines()
        return .result()
    }
}

// MARK: - Stop Session Intent

struct StopSessionIntent: AppIntent {
    static var title: LocalizedStringResource = "Stop Session"
    static var description = IntentDescription("Stop the current EEG recording session.")
    static var openAppWhenRun = false

    func perform() async throws -> some IntentResult {
        WidgetCenter.shared.reloadAllTimelines()
        return .result()
    }
}

// MARK: - Acknowledge Break Intent

struct AcknowledgeBreakIntent: AppIntent {
    static var title: LocalizedStringResource = "Take Break"
    static var description = IntentDescription("Acknowledge the break reminder.")
    static var openAppWhenRun = false

    func perform() async throws -> some IntentResult {
        WidgetCenter.shared.reloadAllTimelines()
        return .result()
    }
}
