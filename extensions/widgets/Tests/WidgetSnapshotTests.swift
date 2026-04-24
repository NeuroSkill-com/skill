// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import XCTest
// Sources compiled alongside tests — no separate module import needed.

/// Tests widget snapshot construction and derived properties.
final class WidgetSnapshotTests: XCTestCase {

    // MARK: - WidgetSnapshot

    func testOfflineSnapshot() {
        let snap = WidgetSnapshot.offline
        XCTAssertFalse(snap.daemonOnline)
        XCTAssertNil(snap.flow)
        XCTAssertNil(snap.fatigue)
        XCTAssertNil(snap.streak)
        XCTAssertNil(snap.status)
        XCTAssertEqual(snap.connectivityState, "offline")
    }

    func testConnectivityStateRecording() {
        let snap = WidgetSnapshot(
            flow: nil, fatigue: nil, streak: nil,
            status: DaemonStatus(state: "recording", deviceName: "Muse S", battery: 80, sampleCount: 1000, deviceError: nil, channelQuality: nil, eegChannelCount: nil, hasPpg: nil, csvPath: nil),
            daemonOnline: true, error: nil
        )
        XCTAssertEqual(snap.connectivityState, "recording")
    }

    func testConnectivityStateScanning() {
        let snap = WidgetSnapshot(
            flow: nil, fatigue: nil, streak: nil,
            status: DaemonStatus(state: "scanning", deviceName: nil, battery: 0, sampleCount: 0, deviceError: nil, channelQuality: nil, eegChannelCount: nil, hasPpg: nil, csvPath: nil),
            daemonOnline: true, error: nil
        )
        XCTAssertEqual(snap.connectivityState, "scanning")
    }

    func testConnectivityStateIdle() {
        let snap = WidgetSnapshot(
            flow: nil, fatigue: nil, streak: nil,
            status: DaemonStatus(state: "idle", deviceName: nil, battery: 0, sampleCount: 0, deviceError: nil, channelQuality: nil, eegChannelCount: nil, hasPpg: nil, csvPath: nil),
            daemonOnline: true, error: nil
        )
        XCTAssertEqual(snap.connectivityState, "idle")
    }

    // MARK: - FocusEntry

    func testFocusEntryPlaceholder() {
        let e = FocusEntry.placeholder
        XCTAssertTrue(e.daemonOnline)
        XCTAssertTrue(e.inFlow)
        XCTAssertEqual(e.score, 72)
        XCTAssertEqual(e.connectivityState, "recording")
    }

    func testFocusEntryOffline() {
        let e = FocusEntry.offline(date: .now)
        XCTAssertFalse(e.daemonOnline)
        XCTAssertFalse(e.inFlow)
        XCTAssertEqual(e.score, 0)
    }

    // MARK: - StreakEntry

    func testStreakEntryPlaceholder() {
        let e = StreakEntry.placeholder
        XCTAssertTrue(e.daemonOnline)
        XCTAssertEqual(e.streakDays, 12)
        XCTAssertEqual(e.todayMins, 45)
        XCTAssertEqual(e.thresholdMins, 60)
        XCTAssertFalse(e.qualifies)
    }

    // MARK: - BreakTimerEntry

    func testBreakTimerRemainingMins() {
        let e = BreakTimerEntry(
            date: .now, continuousWorkMins: 38, suggestedIntervalMins: 52,
            naturalCycleMins: 48, confidence: 0.8, fatigued: false,
            connectivityState: "recording", daemonOnline: true, error: nil
        )
        XCTAssertEqual(e.remainingMins, 14)
        XCTAssertFalse(e.isOverdue)
        XCTAssertEqual(e.progress, 38.0 / 52.0, accuracy: 0.01)
    }

    func testBreakTimerOverdue() {
        let e = BreakTimerEntry(
            date: .now, continuousWorkMins: 60, suggestedIntervalMins: 52,
            naturalCycleMins: nil, confidence: 0.5, fatigued: true,
            connectivityState: "recording", daemonOnline: true, error: nil
        )
        XCTAssertEqual(e.remainingMins, 0)
        XCTAssertTrue(e.isOverdue)
        XCTAssertEqual(e.progress, 1.0, accuracy: 0.01)
    }

    // MARK: - SessionStatusEntry

    func testSessionStatusElapsedTime() {
        // 460800 samples at 256 Hz = 1800 seconds = 30:00
        let e = SessionStatusEntry(
            date: .now, state: "recording", deviceName: "Muse S",
            battery: 72, sampleCount: 460800,
            channelQuality: ["good", "good", "fair", "good"],
            channelCount: 4, hasPpg: true, daemonOnline: true, error: nil
        )
        XCTAssertTrue(e.isRecording)
        XCTAssertTrue(e.isConnected)
        XCTAssertEqual(e.elapsedText, "30:00")
    }

    func testSessionStatusDisconnected() {
        let e = SessionStatusEntry(
            date: .now, state: "idle", deviceName: nil,
            battery: 0, sampleCount: 0,
            channelQuality: [], channelCount: 0,
            hasPpg: false, daemonOnline: true, error: nil
        )
        XCTAssertFalse(e.isRecording)
        XCTAssertFalse(e.isConnected)
        XCTAssertEqual(e.elapsedText, "0:00")
    }

    // MARK: - HeartRateEntry

    func testHeartRateStressLevels() {
        let low = HeartRateEntry(
            date: .now, hr: 65, rmssd: 50, stressIndex: 30,
            respiratoryRate: 14, spo2: 98, hasPpg: true,
            deviceName: "Muse S", daemonOnline: true, error: nil
        )
        XCTAssertTrue(low.hasData)
        XCTAssertEqual(low.stressColor, SkillColors.flowGreen)

        let moderate = HeartRateEntry(
            date: .now, hr: 75, rmssd: 35, stressIndex: 80,
            respiratoryRate: 16, spo2: 97, hasPpg: true,
            deviceName: nil, daemonOnline: true, error: nil
        )
        XCTAssertEqual(moderate.stressColor, SkillColors.warmOrange)

        let high = HeartRateEntry(
            date: .now, hr: 90, rmssd: 20, stressIndex: 200,
            respiratoryRate: 20, spo2: 96, hasPpg: true,
            deviceName: nil, daemonOnline: true, error: nil
        )
        XCTAssertEqual(high.stressColor, SkillColors.alertRed)
    }

    func testHeartRateNoPpg() {
        let e = HeartRateEntry(
            date: .now, hr: nil, rmssd: nil, stressIndex: nil,
            respiratoryRate: nil, spo2: nil, hasPpg: false,
            deviceName: nil, daemonOnline: true, error: nil
        )
        XCTAssertFalse(e.hasData)
    }

    // MARK: - CognitiveLoadEntry

    func testCognitiveLoadLevels() {
        let light = CognitiveLoadEntry(date: .now, overallLoad: 25, topItems: [], daemonOnline: true, error: nil)
        XCTAssertEqual(light.loadColor, SkillColors.flowGreen)

        let moderate = CognitiveLoadEntry(date: .now, overallLoad: 55, topItems: [], daemonOnline: true, error: nil)
        XCTAssertEqual(moderate.loadColor, SkillColors.warmOrange)

        let heavy = CognitiveLoadEntry(date: .now, overallLoad: 85, topItems: [], daemonOnline: true, error: nil)
        XCTAssertEqual(heavy.loadColor, SkillColors.alertRed)
    }

    // MARK: - WeeklyTrendEntry

    func testWeeklyTrendCalculations() {
        let days = (0..<7).map { i in
            DayPoint(
                dayStart: UInt64(Date().timeIntervalSince1970) - UInt64((6 - i) * 86400),
                focus: [50, 55, 60, 65, 70, 75, 80][i],
                deepMins: [40, 45, 50, 55, 60, 65, 70][i]
            )
        }
        let entry = WeeklyTrendEntry(
            date: .now, days: days, weeklyAvgDeepMins: 55,
            currentStreak: 7, daemonOnline: true, error: nil
        )
        // Average focus: (50+55+60+65+70+75+80)/7 = 65
        XCTAssertEqual(entry.avgFocus, 65, accuracy: 0.1)
        // Trend: positive (later days have higher focus)
        XCTAssertGreaterThan(entry.trend, 0)
    }

    func testWeeklyTrendNoData() {
        let entry = WeeklyTrendEntry.offline(date: .now)
        XCTAssertEqual(entry.avgFocus, 0)
        XCTAssertEqual(entry.trend, 0)
    }

    // MARK: - DailyReportEntry

    func testDailyReportPlaceholder() {
        let e = DailyReportEntry.placeholder
        XCTAssertTrue(e.daemonOnline)
        XCTAssertEqual(e.overallFocus, 68)
        XCTAssertEqual(e.productivityScore, 74)
        XCTAssertEqual(e.periods.count, 3)
        XCTAssertEqual(e.bestPeriod, "morning")
    }

    // MARK: - CalendarMindEntry

    func testCalendarMindPlaceholder() {
        let e = CalendarMindEntry.placeholder
        XCTAssertTrue(e.daemonOnline)
        XCTAssertFalse(e.slots.isEmpty)
        XCTAssertEqual(e.anticipations.count, 2)
        XCTAssertGreaterThan(e.avgRecoverySecs, 0)
    }

    func testCalendarMindOffline() {
        let e = CalendarMindEntry.offline(date: .now)
        XCTAssertFalse(e.daemonOnline)
        XCTAssertTrue(e.slots.isEmpty)
        XCTAssertEqual(e.error?.title, WidgetError.daemonOffline.title)
    }

    func testCalendarSlotTypes() {
        let e = CalendarMindEntry.placeholder
        let eventSlots = e.slots.filter(\.isEvent)
        let gapSlots = e.slots.filter { !$0.isEvent }
        XCTAssertGreaterThan(eventSlots.count, 0)
        XCTAssertGreaterThan(gapSlots.count, 0)
    }

    func testCalendarAnticipationDrop() {
        let e = CalendarMindEntry.placeholder
        let withAnticipation = e.slots.filter { $0.anticipationDrop != nil }
        XCTAssertGreaterThan(withAnticipation.count, 0)
        for slot in withAnticipation {
            XCTAssertGreaterThan(slot.anticipationDrop!, 0)
        }
    }

    // MARK: - Duration Formatting

    func testFormatDuration() {
        XCTAssertEqual(formatDuration(secs: 0), "0m")
        XCTAssertEqual(formatDuration(secs: 300), "5m")
        XCTAssertEqual(formatDuration(secs: 3600), "1h")
        XCTAssertEqual(formatDuration(secs: 5400), "1h 30m")
    }
}
