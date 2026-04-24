// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import Foundation

// MARK: - Flow State

struct FlowStateResult: Codable {
    let inFlow: Bool
    let score: Float
    let durationSecs: UInt64
    let avgFocus: Float?
    let fileSwitches: UInt32
    let editVelocity: Float
}

// MARK: - Fatigue

struct FatigueBucket: Codable {
    let quarter: UInt8
    let avgFocus: Float
    let interactions: UInt64
}

struct FatigueAlert: Codable {
    let fatigued: Bool
    let trend: [FatigueBucket]
    let focusDeclinePct: Float
    let suggestion: String
    let continuousWorkMins: UInt64
}

// MARK: - Deep Work Streak

struct DayDeepWork: Codable {
    let dayStart: UInt64
    let deepWorkMins: UInt32
    let avgFocus: Float?
    let qualified: Bool
}

struct DeepWorkStreak: Codable {
    let currentStreakDays: UInt32
    let longestStreakDays: UInt32
    let todayDeepMins: UInt32
    let todayQualifies: Bool
    let thresholdMins: UInt32
    let dailyHistory: [DayDeepWork]
    let weeklyAvgDeepMins: Float
}

// MARK: - Daemon Status

struct DaemonStatus: Codable {
    let state: String
    let deviceName: String?
    let battery: Float
    let sampleCount: UInt64
    let deviceError: String?
    let channelQuality: [String]?
    let eegChannelCount: Int?
    let hasPpg: Bool?
    let csvPath: String?
}

// MARK: - Optimal Hours

struct HourScore: Codable {
    let hour: UInt8
    let score: Float
    let avgFocus: Float?
    let totalChurn: UInt64
    let interactions: UInt64
}

struct OptimalHoursResult: Codable {
    let hours: [HourScore]
    let bestHours: [UInt8]
    let worstHours: [UInt8]
}

// MARK: - Break Timing

struct FocusCurveBucket: Codable {
    let ts: UInt64
    let avgFocus: Float
    let churn: UInt64
}

struct BreakTimingResult: Codable {
    let naturalCycleMins: UInt32?
    let focusCurve: [FocusCurveBucket]
    let suggestedBreakIntervalMins: UInt32
    let confidence: Float
}

// MARK: - Daily Brain Report

struct PeriodSummary: Codable {
    let period: String
    let avgFocus: Float?
    let churn: UInt64
    let interactions: UInt64
    let filesTouched: UInt32
    let undos: UInt64
}

struct DailyBrainReport: Codable {
    let dayStart: UInt64
    let periods: [PeriodSummary]
    let overallFocus: Float?
    let productivityScore: Float
    let bestPeriod: String
}

// MARK: - Cognitive Load

struct CognitiveLoadRow: Codable {
    let key: String
    let avgFocus: Float?
    let avgUndos: Float
    let interactions: UInt64
    let totalSecs: UInt64
    let loadScore: Float
}

// MARK: - Session Metrics (band powers + PPG/cardiac)

struct SessionMetrics: Codable {
    // Band powers
    let relDelta: Double?
    let relTheta: Double?
    let relAlpha: Double?
    let relBeta: Double?
    let relGamma: Double?
    let relHighGamma: Double?
    // Composite scores
    let relaxation: Double?
    let engagement: Double?
    // PPG/cardiac
    let hr: Double?
    let rmssd: Double?
    let sdnn: Double?
    let pnn50: Double?
    let lfHfRatio: Double?
    let respiratoryRate: Double?
    let spo2Estimate: Double?
    let stressIndex: Double?
}

// MARK: - Sleep Analysis

struct SleepSummary: Codable {
    let totalEpochs: Int
    let wakeEpochs: Int
    let n1Epochs: Int
    let n2Epochs: Int
    let n3Epochs: Int
    let remEpochs: Int
    let epochSecs: Double
}

struct SleepStages: Codable {
    let summary: SleepSummary
}

// MARK: - Meeting Recovery

struct MeetingRecoveryRow: Codable {
    let meetingId: Int64
    let title: String
    let platform: String
    let meetingDurationSecs: UInt64
    let recoverySecs: UInt64?
}

struct MeetingRecoveryResult: Codable {
    let meetings: [MeetingRecoveryRow]
    let avgRecoverySecs: UInt64
}

// MARK: - Calendar Event

struct CalendarEvent: Codable {
    let id: String
    let title: String
    let startUtc: Int64
    let endUtc: Int64
    let allDay: Bool
    let location: String?
    let calendar: String?
    let status: String
    let recurrence: String?
}

// MARK: - Calendar Mind State (composed for the widget)

/// A single time slot in the calendar widget — either a calendar event or a gap.
struct CalendarSlot: Identifiable {
    let id = UUID()
    let startUtc: Int64
    let endUtc: Int64
    let title: String
    let isEvent: Bool            // true = calendar event, false = gap
    let avgFocus: Float?
    let wasInFlow: Bool
    let recoverySecs: UInt64?    // post-meeting recovery time
    let anticipationDrop: Float? // focus drop BEFORE event (statistically significant)
    let platform: String?        // meeting platform if detected
}

// MARK: - Widget Snapshot (combined data for timeline entries)

struct WidgetSnapshot {
    let flow: FlowStateResult?
    let fatigue: FatigueAlert?
    let streak: DeepWorkStreak?
    let status: DaemonStatus?
    let daemonOnline: Bool
    let error: WidgetError?

    init(flow: FlowStateResult?, fatigue: FatigueAlert?, streak: DeepWorkStreak?,
         status: DaemonStatus?, daemonOnline: Bool, error: WidgetError? = nil) {
        self.flow = flow; self.fatigue = fatigue; self.streak = streak
        self.status = status; self.daemonOnline = daemonOnline; self.error = error
    }

    /// Daemon connectivity state string for ConnectivityBadge.
    var connectivityState: String {
        guard daemonOnline, let st = status else { return "offline" }
        switch st.state {
        case "recording":  return "recording"
        case "connected":  return "connected"
        case "scanning":   return "scanning"
        default:           return "idle"
        }
    }

    static let offline = WidgetSnapshot(flow: nil, fatigue: nil, streak: nil, status: nil, daemonOnline: false)

    static func offline(error: WidgetError) -> WidgetSnapshot {
        WidgetSnapshot(flow: nil, fatigue: nil, streak: nil, status: nil, daemonOnline: false, error: error)
    }
}
