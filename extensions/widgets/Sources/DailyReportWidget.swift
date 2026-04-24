// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import SwiftUI
import WidgetKit

// MARK: - Timeline Provider

struct DailyReportProvider: TimelineProvider {
    func placeholder(in context: Context) -> DailyReportEntry { .placeholder }

    func getSnapshot(in context: Context, completion: @escaping (DailyReportEntry) -> Void) {
        if context.isPreview { completion(.placeholder); return }
        Task { completion(await fetchEntry()) }
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<DailyReportEntry>) -> Void) {
        Task {
            let entry = await fetchEntry()
            let next = Calendar.current.date(byAdding: .minute, value: 15, to: entry.date)!
            completion(Timeline(entries: [entry], policy: .after(next)))
        }
    }

    private func fetchEntry() async -> DailyReportEntry {
        guard let report = try? await DaemonClient.shared.fetchDailyReport() else {
            return .offline(date: .now)
        }
        let fatigue = try? await DaemonClient.shared.fetchFatigue()
        let streak = try? await DaemonClient.shared.fetchStreak()

        return DailyReportEntry(
            date: .now,
            overallFocus: report.overallFocus,
            productivityScore: report.productivityScore,
            bestPeriod: report.bestPeriod,
            periods: report.periods,
            todayDeepMins: streak?.todayDeepMins ?? 0,
            continuousWorkMins: fatigue?.continuousWorkMins ?? 0,
            daemonOnline: true, error: nil
        )
    }
}

// MARK: - Entry

struct DailyReportEntry: TimelineEntry {
    let date: Date
    let overallFocus: Float?
    let productivityScore: Float
    let bestPeriod: String
    let periods: [PeriodSummary]
    let todayDeepMins: UInt32
    let continuousWorkMins: UInt64
    let daemonOnline: Bool
    let error: WidgetError?

    static let placeholder = DailyReportEntry(
        date: .now, overallFocus: 68, productivityScore: 74,
        bestPeriod: "morning",
        periods: [
            PeriodSummary(period: "morning", avgFocus: 78, churn: 200, interactions: 150, filesTouched: 12, undos: 8),
            PeriodSummary(period: "afternoon", avgFocus: 62, churn: 180, interactions: 120, filesTouched: 8, undos: 14),
            PeriodSummary(period: "evening", avgFocus: 45, churn: 90, interactions: 60, filesTouched: 4, undos: 6),
        ],
        todayDeepMins: 142, continuousWorkMins: 38, daemonOnline: true, error: nil
    )

    static func offline(date: Date, error: WidgetError = .daemonOffline) -> DailyReportEntry {
        DailyReportEntry(
            date: date, overallFocus: nil, productivityScore: 0,
            bestPeriod: "", periods: [], todayDeepMins: 0,
            continuousWorkMins: 0, daemonOnline: false, error: error
        )
    }
}

// MARK: - Widget View

struct DailyReportWidgetView: View {
    let entry: DailyReportEntry

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
            BrandHeader(subtitle: L("daily.today"))
                .padding(.bottom, 8)

            HStack(alignment: .top, spacing: 14) {
                // Left: Key metrics
                VStack(spacing: 8) {
                    // Focus score
                    VStack(spacing: 1) {
                        Text("\(Int(entry.overallFocus ?? 0))")
                            .font(.system(size: 28, weight: .bold, design: .rounded))
                            .foregroundStyle(SkillColors.scoreColor(entry.overallFocus ?? 0))
                        Text(L("daily.avgFocus"))
                            .font(.system(size: 8, weight: .medium))
                            .foregroundStyle(SkillColors.textTertiary)
                            .textCase(.uppercase)
                    }

                    // Productivity
                    HStack(spacing: 4) {
                        Image(systemName: "chart.line.uptrend.xyaxis")
                            .font(.system(size: 9))
                            .foregroundStyle(SkillColors.scoreColor(entry.productivityScore))
                        Text("\(Int(entry.productivityScore))%")
                            .font(.system(size: 12, weight: .bold, design: .rounded))
                            .foregroundStyle(SkillColors.scoreColor(entry.productivityScore))
                    }

                    // Deep work total
                    HStack(spacing: 3) {
                        Image(systemName: "timer")
                            .font(.system(size: 8))
                        Text("\(entry.todayDeepMins)m")
                            .font(.system(size: 10, weight: .semibold, design: .rounded))
                    }
                    .foregroundStyle(SkillColors.textSecondary)
                }
                .frame(width: 72)

                // Divider
                Rectangle()
                    .fill(SkillColors.cardStroke)
                    .frame(width: 1)
                    .padding(.vertical, 4)

                // Right: Period breakdown bars
                VStack(alignment: .leading, spacing: 6) {
                    // Best period callout
                    if !entry.bestPeriod.isEmpty {
                        HStack(spacing: 3) {
                            Image(systemName: "star.fill")
                                .font(.system(size: 7))
                                .foregroundStyle(SkillColors.warmOrange)
                            Text("\(L("daily.best")): \(localizedPeriod(entry.bestPeriod))")
                                .font(.system(size: 9, weight: .semibold))
                                .foregroundStyle(SkillColors.textSecondary)
                        }
                    }

                    // Period bars
                    ForEach(entry.periods, id: \.period) { p in
                        periodBar(p)
                    }
                }
            }
        }
    }

    private func periodBar(_ p: PeriodSummary) -> some View {
        let focus = p.avgFocus ?? 0
        let maxWidth: CGFloat = 100
        let barWidth = max(CGFloat(focus) / 100.0 * maxWidth, 4)

        return VStack(alignment: .leading, spacing: 2) {
            HStack {
                Text(localizedPeriod(p.period))
                    .font(.system(size: 8.5, weight: .medium))
                    .foregroundStyle(SkillColors.textTertiary)
                    .frame(width: 50, alignment: .leading)
                Spacer()
                Text("\(Int(focus))")
                    .font(.system(size: 8.5, weight: .bold, design: .rounded))
                    .foregroundStyle(SkillColors.scoreColor(focus))
            }

            RoundedRectangle(cornerRadius: 2)
                .fill(
                    LinearGradient(
                        colors: [SkillColors.scoreColor(focus).opacity(0.5), SkillColors.scoreColor(focus)],
                        startPoint: .leading, endPoint: .trailing
                    )
                )
                .frame(width: barWidth, height: 4)
        }
    }

    private func localizedPeriod(_ period: String) -> String {
        switch period.lowercased() {
        case "morning":   return L("daily.morning")
        case "afternoon": return L("daily.afternoon")
        case "evening":   return L("daily.evening")
        default:          return period
        }
    }
}

// MARK: - Widget

struct DailyReportWidget: Widget {
    let kind = "com.neuroskill.skill.widget.daily-report"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: DailyReportProvider()) { entry in
            DailyReportWidgetView(entry: entry)
        }
        .configurationDisplayName(L("widget.daily.name"))
        .description(L("widget.daily.desc"))
        .supportedFamilies([.systemMedium])
    }
}

// MARK: - Previews

#Preview("Daily Report", as: .systemMedium) {
    DailyReportWidget()
} timeline: {
    DailyReportEntry.placeholder
}

#Preview("Daily Report — Offline", as: .systemMedium) {
    DailyReportWidget()
} timeline: {
    DailyReportEntry.offline(date: .now)
}
