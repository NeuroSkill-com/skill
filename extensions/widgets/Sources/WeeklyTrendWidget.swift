// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import SwiftUI
import WidgetKit

// MARK: - Timeline Provider

struct WeeklyTrendProvider: TimelineProvider {
    func placeholder(in context: Context) -> WeeklyTrendEntry { .placeholder }

    func getSnapshot(in context: Context, completion: @escaping (WeeklyTrendEntry) -> Void) {
        if context.isPreview { completion(.placeholder); return }
        Task { completion(await fetchEntry()) }
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<WeeklyTrendEntry>) -> Void) {
        Task {
            let entry = await fetchEntry()
            // Refresh hourly — weekly data is slow-moving
            let next = Calendar.current.date(byAdding: .hour, value: 1, to: entry.date)!
            completion(Timeline(entries: [entry], policy: .after(next)))
        }
    }

    private func fetchEntry() async -> WeeklyTrendEntry {
        guard let streak = try? await DaemonClient.shared.fetchStreak() else {
            return .offline(date: .now)
        }
        // Use last 7 days from daily_history
        let recent = Array(streak.dailyHistory.suffix(7))
        let dayPoints = recent.map { day in
            DayPoint(
                dayStart: day.dayStart,
                focus: day.avgFocus ?? 0,
                deepMins: day.deepWorkMins
            )
        }

        return WeeklyTrendEntry(
            date: .now,
            days: dayPoints,
            weeklyAvgDeepMins: streak.weeklyAvgDeepMins,
            currentStreak: streak.currentStreakDays,
            daemonOnline: true, error: nil
        )
    }
}

// MARK: - Entry

struct DayPoint: Identifiable {
    let id = UUID()
    let dayStart: UInt64
    let focus: Float
    let deepMins: UInt32

    var dayLabel: String {
        let date = Date(timeIntervalSince1970: TimeInterval(dayStart))
        let formatter = DateFormatter()
        formatter.dateFormat = "E"
        return String(formatter.string(from: date).prefix(2))
    }
}

struct WeeklyTrendEntry: TimelineEntry {
    let date: Date
    let days: [DayPoint]
    let weeklyAvgDeepMins: Float
    let currentStreak: UInt32
    let daemonOnline: Bool
    let error: WidgetError?

    var avgFocus: Float {
        let focusValues = days.compactMap { $0.focus > 0 ? $0.focus : nil }
        guard !focusValues.isEmpty else { return 0 }
        return focusValues.reduce(0, +) / Float(focusValues.count)
    }

    var trend: Float {
        guard days.count >= 4 else { return 0 }
        let mid = days.count / 2
        let firstHalf = days[..<mid].compactMap { $0.focus > 0 ? $0.focus : nil }
        let secondHalf = days[mid...].compactMap { $0.focus > 0 ? $0.focus : nil }
        guard !firstHalf.isEmpty, !secondHalf.isEmpty else { return 0 }
        let avgFirst = firstHalf.reduce(0, +) / Float(firstHalf.count)
        let avgSecond = secondHalf.reduce(0, +) / Float(secondHalf.count)
        return avgSecond - avgFirst
    }

    static let placeholder: WeeklyTrendEntry = {
        let days = (0..<7).map { i in
            DayPoint(
                dayStart: UInt64(Date().timeIntervalSince1970) - UInt64((6 - i) * 86400),
                focus: [55, 62, 70, 65, 78, 72, 68][i],
                deepMins: [40, 55, 68, 52, 75, 62, 58][i]
            )
        }
        return WeeklyTrendEntry(
            date: .now, days: days, weeklyAvgDeepMins: 58.6,
            currentStreak: 7, daemonOnline: true, error: nil
        )
    }()

    static func offline(date: Date, error: WidgetError = .daemonOffline) -> WeeklyTrendEntry {
        WeeklyTrendEntry(date: date, days: [], weeklyAvgDeepMins: 0,
                         currentStreak: 0, daemonOnline: false, error: error)
    }
}

// MARK: - Widget View

struct WeeklyTrendWidgetView: View {
    let entry: WeeklyTrendEntry

    private var trendIcon: String {
        if entry.trend > 3 { return "arrow.up.right" }
        if entry.trend < -3 { return "arrow.down.right" }
        return "arrow.right"
    }

    private var trendColor: Color {
        if entry.trend > 3 { return SkillColors.flowGreen }
        if entry.trend < -3 { return SkillColors.alertRed }
        return SkillColors.textTertiary
    }

    var body: some View {
        if !entry.daemonOnline {
            OfflineView(error: entry.error ?? .daemonOffline)
                .widgetURL(WidgetDeepLink.activity)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        } else {
            dashContent
                .widgetURL(WidgetDeepLink.activity)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        }
    }

    private var dashContent: some View {
        VStack(spacing: 0) {
            // Header
            BrandHeader(subtitle: L("weekly.subtitle"))
                .padding(.bottom, 6)

            HStack(alignment: .center, spacing: 14) {
                // Left: Summary stats
                VStack(spacing: 8) {
                    // Weekly average focus
                    VStack(spacing: 1) {
                        HStack(spacing: 4) {
                            Text("\(Int(entry.avgFocus))")
                                .font(.system(size: 26, weight: .bold, design: .rounded))
                                .foregroundStyle(SkillColors.scoreColor(entry.avgFocus))
                            Image(systemName: trendIcon)
                                .font(.system(size: 12, weight: .bold))
                                .foregroundStyle(trendColor)
                        }
                        Text(L("weekly.avgFocus"))
                            .font(.system(size: 8, weight: .medium))
                            .foregroundStyle(SkillColors.textTertiary)
                            .textCase(.uppercase)
                    }

                    // Deep work avg
                    HStack(spacing: 3) {
                        Image(systemName: "timer")
                            .font(.system(size: 8))
                        Text("\(Int(entry.weeklyAvgDeepMins))m/\(L("weekly.day"))")
                            .font(.system(size: 10, weight: .semibold, design: .rounded))
                    }
                    .foregroundStyle(SkillColors.textSecondary)

                    // Streak
                    if entry.currentStreak > 0 {
                        HStack(spacing: 3) {
                            Image(systemName: "flame.fill")
                                .font(.system(size: 8))
                                .foregroundStyle(SkillColors.warmOrange)
                            Text("\(entry.currentStreak)\(L("streak.day").prefix(1))")
                                .font(.system(size: 10, weight: .semibold, design: .rounded))
                                .foregroundStyle(SkillColors.textSecondary)
                        }
                    }
                }
                .frame(width: 78)

                // Divider
                Rectangle()
                    .fill(SkillColors.cardStroke)
                    .frame(width: 1)
                    .padding(.vertical, 6)

                // Right: Sparkline bar chart
                sparklineChart
            }
        }
    }

    private var sparklineChart: some View {
        let maxFocus = entry.days.map(\.focus).max() ?? 100

        return VStack(spacing: 4) {
            // Bars
            HStack(alignment: .bottom, spacing: 4) {
                ForEach(entry.days) { day in
                    let norm = maxFocus > 0 ? CGFloat(day.focus / maxFocus) : 0
                    VStack(spacing: 2) {
                        RoundedRectangle(cornerRadius: 3)
                            .fill(
                                LinearGradient(
                                    colors: [SkillColors.scoreColor(day.focus).opacity(0.5), SkillColors.scoreColor(day.focus)],
                                    startPoint: .bottom, endPoint: .top
                                )
                            )
                            .frame(width: 14, height: max(norm * 50, 4))
                            .shadow(color: SkillColors.scoreColor(day.focus).opacity(0.2), radius: 2, x: 0, y: 1)

                        Text(day.dayLabel)
                            .font(.system(size: 7, weight: .medium, design: .rounded))
                            .foregroundStyle(SkillColors.textTertiary)
                    }
                }
            }
            .frame(maxHeight: 65)
        }
    }
}

// MARK: - Widget

struct WeeklyTrendWidget: Widget {
    let kind = "com.neuroskill.skill.widget.weekly-trend"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: WeeklyTrendProvider()) { entry in
            WeeklyTrendWidgetView(entry: entry)
        }
        .configurationDisplayName(L("widget.weekly.name"))
        .description(L("widget.weekly.desc"))
        .supportedFamilies([.systemMedium])
    }
}

// MARK: - Previews

#Preview("Weekly Trend", as: .systemMedium) {
    WeeklyTrendWidget()
} timeline: {
    WeeklyTrendEntry.placeholder
}

#Preview("Weekly Trend — Offline", as: .systemMedium) {
    WeeklyTrendWidget()
} timeline: {
    WeeklyTrendEntry.offline(date: .now)
}
