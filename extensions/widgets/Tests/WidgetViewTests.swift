// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import SwiftUI
import XCTest
// Sources compiled alongside tests — no separate module import needed.

/// Tests that all widget views can be instantiated and rendered without crashing
/// for all entry states: placeholder, online, offline, and edge cases.
final class WidgetViewTests: XCTestCase {

    // MARK: - Focus Widget

    func testFocusWidgetOnline() {
        let entry = FocusEntry(
            date: .now, score: 72, inFlow: true, flowDurationSecs: 1800,
            connectivityState: "recording", deviceName: "Muse S", battery: 85,
            daemonOnline: true, error: nil
        )
        let view = FocusWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    func testFocusWidgetOffline() {
        let entry = FocusEntry.offline(date: .now)
        let view = FocusWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    func testFocusWidgetZeroScore() {
        let entry = FocusEntry(
            date: .now, score: 0, inFlow: false, flowDurationSecs: 0,
            connectivityState: "idle", deviceName: nil, battery: nil,
            daemonOnline: true, error: nil
        )
        let view = FocusWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    func testFocusWidgetMaxScore() {
        let entry = FocusEntry(
            date: .now, score: 100, inFlow: true, flowDurationSecs: 7200,
            connectivityState: "recording", deviceName: "Emotiv", battery: 100,
            daemonOnline: true, error: nil
        )
        let view = FocusWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    // MARK: - Streak Widget

    func testStreakWidgetOnline() {
        let view = StreakWidgetView(entry: .placeholder)
        XCTAssertNotNil(view.body)
    }

    func testStreakWidgetOffline() {
        let view = StreakWidgetView(entry: .offline(date: .now))
        XCTAssertNotNil(view.body)
    }

    func testStreakWidgetZeroDays() {
        let entry = StreakEntry(
            date: .now, streakDays: 0, longestStreak: 0,
            todayMins: 0, thresholdMins: 60,
            qualifies: false, weeklyAvg: 0,
            connectivityState: "idle", deviceName: nil,
            daemonOnline: true, error: nil
        )
        let view = StreakWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    func testStreakWidgetQualified() {
        let entry = StreakEntry(
            date: .now, streakDays: 30, longestStreak: 30,
            todayMins: 90, thresholdMins: 60,
            qualifies: true, weeklyAvg: 80,
            connectivityState: "recording", deviceName: "Muse S",
            daemonOnline: true, error: nil
        )
        let view = StreakWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    // MARK: - Brain Dashboard Widget

    func testBrainDashOnline() {
        let view = BrainDashWidgetView(entry: .placeholder)
        XCTAssertNotNil(view.body)
    }

    func testBrainDashOffline() {
        let view = BrainDashWidgetView(entry: .offline(date: .now))
        XCTAssertNotNil(view.body)
    }

    func testBrainDashFatigued() {
        let entry = BrainDashEntry(
            date: .now, focusScore: 35, inFlow: false, flowDurationSecs: 0,
            fatigued: true, continuousWorkMins: 120,
            breakSuggestion: "Take a 10-minute break",
            streakDays: 5, todayDeepMins: 80, thresholdMins: 60,
            connectivityState: "recording", deviceName: "Muse S", battery: 40,
            daemonOnline: true, error: nil
        )
        let view = BrainDashWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    // MARK: - Break Timer Widget

    func testBreakTimerOnline() {
        let view = BreakTimerWidgetView(entry: .placeholder)
        XCTAssertNotNil(view.body)
    }

    func testBreakTimerOverdue() {
        let entry = BreakTimerEntry(
            date: .now, continuousWorkMins: 70, suggestedIntervalMins: 52,
            naturalCycleMins: nil, confidence: 0.5, fatigued: true,
            connectivityState: "recording", daemonOnline: true, error: nil
        )
        let view = BreakTimerWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    func testBreakTimerOffline() {
        let view = BreakTimerWidgetView(entry: .offline(date: .now))
        XCTAssertNotNil(view.body)
    }

    // MARK: - Session Status Widget

    func testSessionStatusRecording() {
        let view = SessionStatusWidgetView(entry: .placeholder)
        XCTAssertNotNil(view.body)
    }

    func testSessionStatusDisconnected() {
        let entry = SessionStatusEntry(
            date: .now, state: "idle", deviceName: nil,
            battery: 0, sampleCount: 0,
            channelQuality: [], channelCount: 0,
            hasPpg: false, daemonOnline: true, error: nil
        )
        let view = SessionStatusWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    func testSessionStatusOffline() {
        let view = SessionStatusWidgetView(entry: .offline(date: .now))
        XCTAssertNotNil(view.body)
    }

    // MARK: - Optimal Hours Widget

    func testOptimalHoursOnline() {
        let view = OptimalHoursWidgetView(entry: .placeholder)
        XCTAssertNotNil(view.body)
    }

    func testOptimalHoursEmpty() {
        let entry = OptimalHoursEntry(
            date: .now, hours: [], bestHours: [], worstHours: [],
            currentHour: 12, daemonOnline: true, error: nil
        )
        let view = OptimalHoursWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    func testOptimalHoursOffline() {
        let view = OptimalHoursWidgetView(entry: .offline(date: .now))
        XCTAssertNotNil(view.body)
    }

    // MARK: - Daily Report Widget

    func testDailyReportOnline() {
        let view = DailyReportWidgetView(entry: .placeholder)
        XCTAssertNotNil(view.body)
    }

    func testDailyReportOffline() {
        let view = DailyReportWidgetView(entry: .offline(date: .now))
        XCTAssertNotNil(view.body)
    }

    func testDailyReportNoPeriods() {
        let entry = DailyReportEntry(
            date: .now, overallFocus: nil, productivityScore: 0,
            bestPeriod: "", periods: [],
            todayDeepMins: 0, continuousWorkMins: 0,
            daemonOnline: true, error: nil
        )
        let view = DailyReportWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    // MARK: - Weekly Trend Widget

    func testWeeklyTrendOnline() {
        let view = WeeklyTrendWidgetView(entry: .placeholder)
        XCTAssertNotNil(view.body)
    }

    func testWeeklyTrendOffline() {
        let view = WeeklyTrendWidgetView(entry: .offline(date: .now))
        XCTAssertNotNil(view.body)
    }

    // MARK: - Heart Rate Widget

    func testHeartRateOnline() {
        let view = HeartRateWidgetView(entry: .placeholder)
        XCTAssertNotNil(view.body)
    }

    func testHeartRateNoPpg() {
        let entry = HeartRateEntry(
            date: .now, hr: nil, rmssd: nil, stressIndex: nil,
            respiratoryRate: nil, spo2: nil,
            hasPpg: false, deviceName: nil, daemonOnline: true, error: nil
        )
        let view = HeartRateWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    func testHeartRateOffline() {
        let view = HeartRateWidgetView(entry: .offline(date: .now))
        XCTAssertNotNil(view.body)
    }

    // MARK: - Cognitive Load Widget

    func testCognitiveLoadOnline() {
        let view = CognitiveLoadWidgetView(entry: .placeholder)
        XCTAssertNotNil(view.body)
    }

    func testCognitiveLoadEmpty() {
        let entry = CognitiveLoadEntry(
            date: .now, overallLoad: 0, topItems: [],
            daemonOnline: true, error: nil
        )
        let view = CognitiveLoadWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    func testCognitiveLoadOffline() {
        let view = CognitiveLoadWidgetView(entry: .offline(date: .now))
        XCTAssertNotNil(view.body)
    }

    // MARK: - Calendar Mind State Widget

    func testCalendarMindOnline() {
        let view = CalendarMindWidgetView(entry: .placeholder)
        XCTAssertNotNil(view.body)
    }

    func testCalendarMindOffline() {
        let view = CalendarMindWidgetView(entry: .offline(date: .now))
        XCTAssertNotNil(view.body)
    }

    func testCalendarMindEmpty() {
        let entry = CalendarMindEntry(
            date: .now, slots: [], currentHour: 10, avgRecoverySecs: 0,
            anticipations: [], overallFocus: 72, inFlow: true,
            daemonOnline: true, error: nil
        )
        let view = CalendarMindWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    func testCalendarMindManySlots() {
        let now = Int64(Date().timeIntervalSince1970)
        var slots: [CalendarSlot] = []
        for i in 0..<4 {
            let start = now + Int64(i * 1800)
            let end = start + 1500
            let isEvt = (i % 2 == 0)
            slots.append(CalendarSlot(
                startUtc: start, endUtc: end,
                title: isEvt ? "Meeting" : "Work",
                isEvent: isEvt, avgFocus: 65, wasInFlow: false,
                recoverySecs: isEvt ? 360 : nil,
                anticipationDrop: isEvt ? 10 : nil,
                platform: isEvt ? "zoom" : nil
            ))
        }
        let entry = CalendarMindEntry(
            date: .now, slots: slots, currentHour: 14, avgRecoverySecs: 480,
            anticipations: [AnticipationInsight(icon: "arrow.clockwise", text: "Test")],
            overallFocus: 65, inFlow: false,
            daemonOnline: true, error: nil
        )
        let view = CalendarMindWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    // MARK: - Configurable Widget

    func testConfigurableWidgetFocus() {
        let entry = ConfigurableEntry(date: .now, metric: .focus, snapshot: nil,
                                       daemonOnline: true, error: nil)
        let view = ConfigurableWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    func testConfigurableWidgetOffline() {
        let entry = ConfigurableEntry(date: .now, metric: .streak, snapshot: nil,
                                       daemonOnline: false, error: .daemonOffline)
        let view = ConfigurableWidgetView(entry: entry)
        XCTAssertNotNil(view.body)
    }

    // MARK: - EEG Band Power Widget

    func testBandPowerSmallOnline() {
        let view = BandPowerSmallView(entry: .placeholder)
        XCTAssertNotNil(view.body)
    }

    func testBandPowerSmallOffline() {
        let view = BandPowerSmallView(entry: .offline(date: .now))
        XCTAssertNotNil(view.body)
    }

    func testBandPowerMediumOnline() {
        let view = BandPowerMediumView(entry: .placeholder)
        XCTAssertNotNil(view.body)
    }

    func testBandPowerEntryBands() {
        let e = BandPowerEntry.placeholder
        XCTAssertEqual(e.bands.count, 5)
        XCTAssertTrue(e.hasData)
    }

    // MARK: - Sleep Widget

    func testSleepSmallOnline() {
        let view = SleepSmallView(entry: .placeholder)
        XCTAssertNotNil(view.body)
    }

    func testSleepSmallOffline() {
        let view = SleepSmallView(entry: .offline(date: .now))
        XCTAssertNotNil(view.body)
    }

    func testSleepMediumOnline() {
        let view = SleepMediumView(entry: .placeholder)
        XCTAssertNotNil(view.body)
    }

    func testSleepEntryQuality() {
        let e = SleepEntry.placeholder
        XCTAssertTrue(e.hasData)
        XCTAssertEqual(e.quality, 78, accuracy: 1)
        XCTAssertEqual(e.qualityColor, SkillColors.flowGreen)
        XCTAssertEqual(e.stageBars.count, 4)
    }

    func testSleepDuration() {
        let e = SleepEntry.placeholder
        XCTAssertEqual(e.durationText, "7h 12m")
    }

    // MARK: - Shared Components

    func testArcGauge() {
        let gauge = ArcGauge(score: 75, size: 80, lineWidth: 7, label: "Test")
        XCTAssertNotNil(gauge.body)
    }

    func testStatusPillFilled() {
        let pill = StatusPill(icon: "bolt.fill", text: "Test", color: .green, filled: true)
        XCTAssertNotNil(pill.body)
    }

    func testStatusPillOutline() {
        let pill = StatusPill(icon: "bolt", text: "Test", color: .gray, filled: false)
        XCTAssertNotNil(pill.body)
    }

    func testConnectivityBadge() {
        let badge = ConnectivityBadge(state: "recording", deviceName: "Muse S", battery: 85)
        XCTAssertNotNil(badge.body)
    }

    func testConnectivityBadgeLowBattery() {
        let badge = ConnectivityBadge(state: "connected", deviceName: nil, battery: 10)
        XCTAssertNotNil(badge.body)
    }

    func testOfflineViewCompact() {
        let view = OfflineView(compact: true)
        XCTAssertNotNil(view.body)
    }

    func testOfflineViewFull() {
        let view = OfflineView(compact: false)
        XCTAssertNotNil(view.body)
    }

    func testSlimProgressBar() {
        let bar = SlimProgressBar(progress: 0.75, fillColor: .blue, leftLabel: "L", rightLabel: "R")
        XCTAssertNotNil(bar.body)
    }

    func testMetricRow() {
        let row = MetricRow(icon: "flame.fill", iconColor: .orange, label: "Streak", value: "7d")
        XCTAssertNotNil(row.body)
    }

    func testBrandHeader() {
        let header = BrandHeader(subtitle: "Test")
        XCTAssertNotNil(header.body)
    }
}
