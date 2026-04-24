// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import SwiftUI
import WidgetKit

@main
struct SkillWidgetBundle: WidgetBundle {
    var body: some Widget {
        // Core (5)
        FocusWidget()
        StreakWidget()
        BrainDashWidget()
        SessionStatusWidget()
        BreakTimerWidget()
        // Analysis (4)
        OptimalHoursWidget()
        DailyReportWidget()
        WeeklyTrendWidget()
        CalendarMindWidget()
        // Biometrics (3)
        HeartRateWidget()
        BandPowerWidget()
        SleepWidget()
    }
}
