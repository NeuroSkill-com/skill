// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import SwiftUI
import WidgetKit

// MARK: - Timeline Provider

struct OptimalHoursProvider: TimelineProvider {
    func placeholder(in context: Context) -> OptimalHoursEntry { .placeholder }

    func getSnapshot(in context: Context, completion: @escaping (OptimalHoursEntry) -> Void) {
        if context.isPreview { completion(.placeholder); return }
        Task { completion(await fetchEntry()) }
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<OptimalHoursEntry>) -> Void) {
        Task {
            let entry = await fetchEntry()
            // Refresh every 30 minutes — hourly data is slow-moving
            let next = Calendar.current.date(byAdding: .minute, value: 30, to: entry.date)!
            completion(Timeline(entries: [entry], policy: .after(next)))
        }
    }

    private func fetchEntry() async -> OptimalHoursEntry {
        guard let result = try? await DaemonClient.shared.fetchOptimalHours() else {
            return .offline(date: .now)
        }
        return OptimalHoursEntry(
            date: .now,
            hours: result.hours,
            bestHours: result.bestHours,
            worstHours: result.worstHours,
            currentHour: UInt8(Calendar.current.component(.hour, from: .now)),
            daemonOnline: true, error: nil
        )
    }
}

// MARK: - Entry

struct OptimalHoursEntry: TimelineEntry {
    let date: Date
    let hours: [HourScore]
    let bestHours: [UInt8]
    let worstHours: [UInt8]
    let currentHour: UInt8
    let daemonOnline: Bool
    let error: WidgetError?

    static let placeholder: OptimalHoursEntry = {
        let hours = (6...22).map { h in
            HourScore(
                hour: UInt8(h),
                score: Float([0.3, 0.5, 0.7, 0.9, 0.85, 0.6, 0.4, 0.55, 0.75, 0.8, 0.65, 0.5, 0.35, 0.45, 0.6, 0.7, 0.5][h - 6]),
                avgFocus: Float([35, 50, 68, 82, 78, 55, 40, 52, 72, 76, 60, 48, 32, 42, 58, 66, 45][h - 6]),
                totalChurn: 100, interactions: 50
            )
        }
        return OptimalHoursEntry(
            date: .now, hours: hours, bestHours: [9, 10, 14], worstHours: [18, 13, 6],
            currentHour: 10, daemonOnline: true, error: nil
        )
    }()

    static func offline(date: Date, error: WidgetError = .daemonOffline) -> OptimalHoursEntry {
        OptimalHoursEntry(date: date, hours: [], bestHours: [], worstHours: [],
                          currentHour: 0, daemonOnline: false, error: error)
    }
}

// MARK: - Widget View

struct OptimalHoursWidgetView: View {
    let entry: OptimalHoursEntry

    private var workHours: [HourScore] {
        entry.hours.filter { $0.hour >= 6 && $0.hour <= 22 }
    }

    private var maxScore: Float {
        workHours.map(\.score).max() ?? 1
    }

    private var peakText: String {
        guard let best = entry.bestHours.first else { return "--" }
        return formatHour(best)
    }

    var body: some View {
        if !entry.daemonOnline {
            OfflineView(compact: true, error: entry.error ?? .daemonOffline)
                .widgetURL(WidgetDeepLink.activity)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        } else {
            content
                .widgetURL(WidgetDeepLink.activity)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        }
    }

    private var content: some View {
        VStack(spacing: 5) {
            // Header
            HStack(spacing: 4) {
                Image(systemName: "brain.filled.head.profile")
                    .font(.system(size: 8, weight: .medium))
                    .foregroundStyle(SkillColors.brand)
                Text(L("hours.title"))
                    .font(.system(size: 8, weight: .bold, design: .rounded))
                    .foregroundStyle(SkillColors.brand)
                    .kerning(1.0)
                    .textCase(.uppercase)
                Spacer()
            }

            Spacer(minLength: 0)

            // Peak hour callout
            VStack(spacing: 1) {
                Text(L("hours.peak"))
                    .font(.system(size: 8, weight: .medium))
                    .foregroundStyle(SkillColors.textTertiary)
                    .textCase(.uppercase)
                    .kerning(0.5)
                Text(peakText)
                    .font(.system(size: 22, weight: .bold, design: .rounded))
                    .foregroundStyle(SkillColors.flowGreen)
            }

            Spacer(minLength: 0)

            // Heat strip
            HStack(spacing: 1.5) {
                ForEach(workHours, id: \.hour) { h in
                    let norm = maxScore > 0 ? CGFloat(h.score / maxScore) : 0
                    RoundedRectangle(cornerRadius: 2)
                        .fill(barColor(norm: norm))
                        .frame(height: max(norm * 28, 3))
                        .opacity(h.hour == entry.currentHour ? 1.0 : 0.75)
                        .overlay(
                            h.hour == entry.currentHour
                                ? RoundedRectangle(cornerRadius: 2)
                                    .stroke(Color.white.opacity(0.6), lineWidth: 1)
                                : nil
                        )
                }
            }
            .frame(height: 30)

            // Hour axis labels
            HStack {
                Text("6")
                    .font(.system(size: 7, design: .rounded))
                    .foregroundStyle(SkillColors.textTertiary)
                Spacer()
                Text("12")
                    .font(.system(size: 7, design: .rounded))
                    .foregroundStyle(SkillColors.textTertiary)
                Spacer()
                Text("22")
                    .font(.system(size: 7, design: .rounded))
                    .foregroundStyle(SkillColors.textTertiary)
            }
        }
        .padding(.top, 2)
    }

    private func barColor(norm: CGFloat) -> Color {
        if norm > 0.7 { return SkillColors.flowGreen }
        if norm > 0.4 { return SkillColors.warmOrange }
        return SkillColors.alertRed.opacity(0.7)
    }

    private func formatHour(_ h: UInt8) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "ha"
        var comps = DateComponents()
        comps.hour = Int(h)
        let date = Calendar.current.date(from: comps) ?? .now
        return formatter.string(from: date).lowercased()
    }
}

// MARK: - Widget

struct OptimalHoursWidget: Widget {
    let kind = "com.neuroskill.skill.widget.optimal-hours"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: OptimalHoursProvider()) { entry in
            OptimalHoursWidgetView(entry: entry)
        }
        .configurationDisplayName(L("widget.hours.name"))
        .description(L("widget.hours.desc"))
        .supportedFamilies([.systemSmall])
    }
}

// MARK: - Previews

#Preview("Optimal Hours", as: .systemSmall) {
    OptimalHoursWidget()
} timeline: {
    OptimalHoursEntry.placeholder
}

#Preview("Optimal Hours — Offline", as: .systemSmall) {
    OptimalHoursWidget()
} timeline: {
    OptimalHoursEntry.offline(date: .now)
}
