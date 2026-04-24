// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import XCTest
// Sources compiled alongside tests — no separate module import needed.

/// Tests that all Codable models correctly decode daemon API JSON responses
/// using snake_case → camelCase key conversion.
final class ModelDecodingTests: XCTestCase {

    private var decoder: JSONDecoder {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }

    // MARK: - FlowStateResult

    func testFlowStateDecoding() throws {
        let json = """
        {
            "in_flow": true,
            "score": 72.5,
            "duration_secs": 1800,
            "avg_focus": 68.3,
            "file_switches": 5,
            "edit_velocity": 12.4
        }
        """
        let result = try decoder.decode(FlowStateResult.self, from: Data(json.utf8))
        XCTAssertTrue(result.inFlow)
        XCTAssertEqual(result.score, 72.5, accuracy: 0.01)
        XCTAssertEqual(result.durationSecs, 1800)
        XCTAssertEqual(result.avgFocus!, 68.3, accuracy: 0.1)
        XCTAssertEqual(result.fileSwitches, 5)
        XCTAssertEqual(result.editVelocity, 12.4, accuracy: 0.1)
    }

    func testFlowStateNullFocus() throws {
        let json = """
        {
            "in_flow": false,
            "score": 0,
            "duration_secs": 0,
            "avg_focus": null,
            "file_switches": 0,
            "edit_velocity": 0
        }
        """
        let result = try decoder.decode(FlowStateResult.self, from: Data(json.utf8))
        XCTAssertFalse(result.inFlow)
        XCTAssertNil(result.avgFocus)
    }

    // MARK: - FatigueAlert

    func testFatigueAlertDecoding() throws {
        let json = """
        {
            "fatigued": true,
            "trend": [
                {"quarter": 1, "avg_focus": 75.0, "interactions": 100},
                {"quarter": 2, "avg_focus": 60.0, "interactions": 80}
            ],
            "focus_decline_pct": 20.0,
            "suggestion": "Take a 5-minute break",
            "continuous_work_mins": 90
        }
        """
        let result = try decoder.decode(FatigueAlert.self, from: Data(json.utf8))
        XCTAssertTrue(result.fatigued)
        XCTAssertEqual(result.trend.count, 2)
        XCTAssertEqual(result.trend[0].quarter, 1)
        XCTAssertEqual(result.focusDeclinePct, 20.0, accuracy: 0.1)
        XCTAssertEqual(result.suggestion, "Take a 5-minute break")
        XCTAssertEqual(result.continuousWorkMins, 90)
    }

    // MARK: - DeepWorkStreak

    func testDeepWorkStreakDecoding() throws {
        let json = """
        {
            "current_streak_days": 7,
            "longest_streak_days": 21,
            "today_deep_mins": 45,
            "today_qualifies": false,
            "threshold_mins": 60,
            "daily_history": [
                {"day_start": 1700000000, "deep_work_mins": 65, "avg_focus": 72.0, "qualified": true},
                {"day_start": 1700086400, "deep_work_mins": 30, "avg_focus": null, "qualified": false}
            ],
            "weekly_avg_deep_mins": 52.3
        }
        """
        let result = try decoder.decode(DeepWorkStreak.self, from: Data(json.utf8))
        XCTAssertEqual(result.currentStreakDays, 7)
        XCTAssertEqual(result.longestStreakDays, 21)
        XCTAssertEqual(result.todayDeepMins, 45)
        XCTAssertFalse(result.todayQualifies)
        XCTAssertEqual(result.dailyHistory.count, 2)
        XCTAssertTrue(result.dailyHistory[0].qualified)
        XCTAssertNil(result.dailyHistory[1].avgFocus)
        XCTAssertEqual(result.weeklyAvgDeepMins, 52.3, accuracy: 0.1)
    }

    // MARK: - DaemonStatus

    func testDaemonStatusDecoding() throws {
        let json = """
        {
            "state": "recording",
            "device_name": "Muse S",
            "battery": 85.0,
            "sample_count": 460800,
            "device_error": null,
            "channel_quality": ["good", "good", "fair", "good"],
            "eeg_channel_count": 4,
            "has_ppg": true,
            "csv_path": "/tmp/session.csv"
        }
        """
        let result = try decoder.decode(DaemonStatus.self, from: Data(json.utf8))
        XCTAssertEqual(result.state, "recording")
        XCTAssertEqual(result.deviceName, "Muse S")
        XCTAssertEqual(result.battery, 85.0, accuracy: 0.1)
        XCTAssertEqual(result.sampleCount, 460800)
        XCTAssertNil(result.deviceError)
        XCTAssertEqual(result.channelQuality, ["good", "good", "fair", "good"])
        XCTAssertEqual(result.eegChannelCount, 4)
        XCTAssertEqual(result.hasPpg, true)
    }

    func testDaemonStatusMinimalFields() throws {
        let json = """
        {
            "state": "idle",
            "device_name": null,
            "battery": 0,
            "sample_count": 0,
            "device_error": null
        }
        """
        let result = try decoder.decode(DaemonStatus.self, from: Data(json.utf8))
        XCTAssertEqual(result.state, "idle")
        XCTAssertNil(result.deviceName)
        XCTAssertNil(result.channelQuality)
        XCTAssertNil(result.hasPpg)
    }

    // MARK: - OptimalHoursResult

    func testOptimalHoursDecoding() throws {
        let json = """
        {
            "hours": [
                {"hour": 9, "score": 0.85, "avg_focus": 78.0, "total_churn": 200, "interactions": 150},
                {"hour": 14, "score": 0.65, "avg_focus": 62.0, "total_churn": 180, "interactions": 120}
            ],
            "best_hours": [9, 10],
            "worst_hours": [18, 19]
        }
        """
        let result = try decoder.decode(OptimalHoursResult.self, from: Data(json.utf8))
        XCTAssertEqual(result.hours.count, 2)
        XCTAssertEqual(result.hours[0].hour, 9)
        XCTAssertEqual(result.hours[0].score, 0.85, accuracy: 0.01)
        XCTAssertEqual(result.bestHours, [9, 10])
        XCTAssertEqual(result.worstHours, [18, 19])
    }

    // MARK: - BreakTimingResult

    func testBreakTimingDecoding() throws {
        let json = """
        {
            "natural_cycle_mins": 48,
            "focus_curve": [
                {"ts": 1700000000, "avg_focus": 75.0, "churn": 100}
            ],
            "suggested_break_interval_mins": 52,
            "confidence": 0.82
        }
        """
        let result = try decoder.decode(BreakTimingResult.self, from: Data(json.utf8))
        XCTAssertEqual(result.naturalCycleMins, 48)
        XCTAssertEqual(result.focusCurve.count, 1)
        XCTAssertEqual(result.suggestedBreakIntervalMins, 52)
        XCTAssertEqual(result.confidence, 0.82, accuracy: 0.01)
    }

    func testBreakTimingNullCycle() throws {
        let json = """
        {
            "natural_cycle_mins": null,
            "focus_curve": [],
            "suggested_break_interval_mins": 52,
            "confidence": 0.0
        }
        """
        let result = try decoder.decode(BreakTimingResult.self, from: Data(json.utf8))
        XCTAssertNil(result.naturalCycleMins)
    }

    // MARK: - DailyBrainReport

    func testDailyReportDecoding() throws {
        let json = """
        {
            "day_start": 1700000000,
            "periods": [
                {
                    "period": "morning",
                    "avg_focus": 78.0,
                    "churn": 200,
                    "interactions": 150,
                    "files_touched": 12,
                    "undos": 8
                }
            ],
            "overall_focus": 68.5,
            "productivity_score": 74.0,
            "best_period": "morning"
        }
        """
        let result = try decoder.decode(DailyBrainReport.self, from: Data(json.utf8))
        XCTAssertEqual(result.dayStart, 1700000000)
        XCTAssertEqual(result.periods.count, 1)
        XCTAssertEqual(result.periods[0].period, "morning")
        XCTAssertEqual(result.periods[0].filesTouched, 12)
        XCTAssertEqual(result.overallFocus!, 68.5, accuracy: 0.1)
        XCTAssertEqual(result.productivityScore, 74.0, accuracy: 0.1)
        XCTAssertEqual(result.bestPeriod, "morning")
    }

    // MARK: - CognitiveLoadRow

    func testCognitiveLoadDecoding() throws {
        let json = """
        [
            {
                "key": "Rust",
                "avg_focus": 55.0,
                "avg_undos": 3.2,
                "interactions": 240,
                "total_secs": 3600,
                "load_score": 72.0
            },
            {
                "key": "TypeScript",
                "avg_focus": null,
                "avg_undos": 1.8,
                "interactions": 180,
                "total_secs": 2400,
                "load_score": 48.0
            }
        ]
        """
        let result = try decoder.decode([CognitiveLoadRow].self, from: Data(json.utf8))
        XCTAssertEqual(result.count, 2)
        XCTAssertEqual(result[0].key, "Rust")
        XCTAssertEqual(result[0].loadScore, 72.0, accuracy: 0.1)
        XCTAssertNil(result[1].avgFocus)
    }

    // MARK: - SessionMetrics

    func testSessionMetricsDecoding() throws {
        let json = """
        {
            "hr": 68.5,
            "rmssd": 42.0,
            "sdnn": 55.0,
            "pnn50": 18.5,
            "lf_hf_ratio": 1.2,
            "respiratory_rate": 14.0,
            "spo2_estimate": 98.0,
            "stress_index": 65.0
        }
        """
        let result = try decoder.decode(SessionMetrics.self, from: Data(json.utf8))
        XCTAssertEqual(result.hr!, 68.5, accuracy: 0.1)
        XCTAssertEqual(result.rmssd!, 42.0, accuracy: 0.1)
        XCTAssertEqual(result.stressIndex!, 65.0, accuracy: 0.1)
        XCTAssertEqual(result.spo2Estimate!, 98.0, accuracy: 0.1)
    }

    func testSessionMetricsAllNull() throws {
        let json = "{}"
        let result = try decoder.decode(SessionMetrics.self, from: Data(json.utf8))
        XCTAssertNil(result.hr)
        XCTAssertNil(result.rmssd)
        XCTAssertNil(result.stressIndex)
    }

    // MARK: - MeetingRecoveryResult

    func testMeetingRecoveryDecoding() throws {
        let json = """
        {
            "meetings": [
                {
                    "meeting_id": 42,
                    "title": "Sprint Planning",
                    "platform": "zoom",
                    "meeting_duration_secs": 1800,
                    "recovery_secs": 480
                },
                {
                    "meeting_id": 43,
                    "title": "1:1 with Alex",
                    "platform": "teams",
                    "meeting_duration_secs": 1200,
                    "recovery_secs": null
                }
            ],
            "avg_recovery_secs": 420
        }
        """
        let result = try decoder.decode(MeetingRecoveryResult.self, from: Data(json.utf8))
        XCTAssertEqual(result.meetings.count, 2)
        XCTAssertEqual(result.meetings[0].title, "Sprint Planning")
        XCTAssertEqual(result.meetings[0].platform, "zoom")
        XCTAssertEqual(result.meetings[0].recoverySecs, 480)
        XCTAssertNil(result.meetings[1].recoverySecs)
        XCTAssertEqual(result.avgRecoverySecs, 420)
    }

    // MARK: - CalendarEvent

    func testCalendarEventDecoding() throws {
        let json = """
        {
            "id": "ev-123",
            "title": "Team Standup",
            "start_utc": 1700000000,
            "end_utc": 1700001800,
            "all_day": false,
            "location": "Zoom",
            "calendar": "Work",
            "status": "confirmed",
            "recurrence": "FREQ=DAILY"
        }
        """
        let result = try decoder.decode(CalendarEvent.self, from: Data(json.utf8))
        XCTAssertEqual(result.id, "ev-123")
        XCTAssertEqual(result.title, "Team Standup")
        XCTAssertFalse(result.allDay)
        XCTAssertEqual(result.location, "Zoom")
        XCTAssertEqual(result.recurrence, "FREQ=DAILY")
    }
}
