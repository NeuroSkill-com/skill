// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import SwiftUI
import WidgetKit

// MARK: - Timeline Provider

struct SleepProvider: TimelineProvider {
    func placeholder(in context: Context) -> SleepEntry { .placeholder }

    func getSnapshot(in context: Context, completion: @escaping (SleepEntry) -> Void) {
        if context.isPreview { completion(.placeholder); return }
        Task { completion(await fetchEntry()) }
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<SleepEntry>) -> Void) {
        Task {
            let entry = await fetchEntry()
            // Refresh hourly — sleep data changes once a day
            let next = Calendar.current.date(byAdding: .hour, value: 1, to: entry.date)!
            completion(Timeline(entries: [entry], policy: .after(next)))
        }
    }

    private func fetchEntry() async -> SleepEntry {
        guard let sleep = try? await DaemonClient.shared.fetchSleep() else {
            return .offline(date: .now, error: DaemonClient.shared.detectError())
        }
        let s = sleep.summary
        let totalMins = Double(s.totalEpochs) * s.epochSecs / 60
        let wakeMins = Double(s.wakeEpochs) * s.epochSecs / 60
        let n1Mins = Double(s.n1Epochs) * s.epochSecs / 60
        let n2Mins = Double(s.n2Epochs) * s.epochSecs / 60
        let n3Mins = Double(s.n3Epochs) * s.epochSecs / 60
        let remMins = Double(s.remEpochs) * s.epochSecs / 60
        let sleepMins = totalMins - wakeMins

        // Quality score: weighted by deep sleep and REM proportion
        let quality: Float = totalMins > 0
            ? Float(min(((n3Mins / totalMins) * 200 + (remMins / totalMins) * 150 + (sleepMins / totalMins) * 50), 100))
            : 0

        return SleepEntry(
            date: .now,
            totalSleepMins: Int(sleepMins),
            deepMins: Int(n3Mins), lightMins: Int(n1Mins + n2Mins),
            remMins: Int(remMins), wakeMins: Int(wakeMins),
            quality: quality,
            daemonOnline: true, error: nil
        )
    }
}

// MARK: - Entry

struct SleepEntry: TimelineEntry {
    let date: Date
    let totalSleepMins: Int
    let deepMins: Int
    let lightMins: Int
    let remMins: Int
    let wakeMins: Int
    let quality: Float
    let daemonOnline: Bool
    let error: WidgetError?

    var hasData: Bool { totalSleepMins > 0 }

    var qualityColor: Color {
        if quality >= 70 { return SkillColors.flowGreen }
        if quality >= 40 { return SkillColors.warmOrange }
        return SkillColors.alertRed
    }

    var durationText: String {
        let h = totalSleepMins / 60
        let m = totalSleepMins % 60
        return m > 0 ? "\(h)h \(m)m" : "\(h)h"
    }

    /// Proportions for the stage bar (deep, light, REM, wake).
    var stageBars: [(String, Int, Color)] {
        [
            (L("sleep.deep"), deepMins, Color(red: 0.20, green: 0.25, blue: 0.65)),
            (L("sleep.light"), lightMins, Color(red: 0.40, green: 0.55, blue: 0.85)),
            (L("sleep.rem"), remMins, Color(red: 0.60, green: 0.35, blue: 0.80)),
            (L("sleep.wake"), wakeMins, Color(red: 0.75, green: 0.75, blue: 0.75)),
        ]
    }

    static let placeholder = SleepEntry(
        date: .now, totalSleepMins: 432, deepMins: 85, lightMins: 220,
        remMins: 105, wakeMins: 22, quality: 78,
        daemonOnline: true, error: nil
    )

    static func offline(date: Date, error: WidgetError = .daemonOffline) -> SleepEntry {
        SleepEntry(date: date, totalSleepMins: 0, deepMins: 0, lightMins: 0,
                   remMins: 0, wakeMins: 0, quality: 0,
                   daemonOnline: false, error: error)
    }
}

// MARK: - Small View

struct SleepSmallView: View {
    let entry: SleepEntry

    var body: some View {
        if !entry.daemonOnline || !entry.hasData {
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
        VStack(spacing: 6) {
            HStack(spacing: 4) {
                Image(systemName: "brain.filled.head.profile")
                    .font(.system(size: 8, weight: .medium))
                    .foregroundStyle(SkillColors.brand)
                Text(L("sleep.title"))
                    .font(.system(size: 8, weight: .bold, design: .rounded))
                    .foregroundStyle(SkillColors.brand)
                    .kerning(1.0).textCase(.uppercase)
                Spacer()
            }

            // Duration
            HStack(spacing: 4) {
                Image(systemName: "moon.fill")
                    .font(.system(size: 14))
                    .foregroundStyle(Color(red: 0.40, green: 0.35, blue: 0.80))
                Text(entry.durationText)
                    .font(.system(size: 26, weight: .bold, design: .rounded))
                    .foregroundStyle(SkillColors.textPrimary)
            }

            // Quality pill
            StatusPill(
                icon: "star.fill",
                text: "\(L("sleep.quality")) \(Int(entry.quality))%",
                color: entry.qualityColor,
                filled: entry.quality >= 70
            )

            // Stage breakdown bar
            stageBar
        }
        .padding(.top, 2)
    }

    private var stageBar: some View {
        let total = max(entry.totalSleepMins + entry.wakeMins, 1)
        return VStack(spacing: 3) {
            GeometryReader { geo in
                HStack(spacing: 1) {
                    ForEach(entry.stageBars, id: \.0) { _, mins, color in
                        let width = geo.size.width * CGFloat(mins) / CGFloat(total)
                        RoundedRectangle(cornerRadius: 1.5)
                            .fill(color)
                            .frame(width: max(width, 2))
                    }
                }
            }
            .frame(height: 6)
            .clipShape(RoundedRectangle(cornerRadius: 3))

            HStack(spacing: 0) {
                ForEach(entry.stageBars.prefix(3), id: \.0) { label, mins, color in
                    HStack(spacing: 2) {
                        Circle().fill(color).frame(width: 4, height: 4)
                        Text("\(mins / 60)h")
                            .font(.system(size: 7, weight: .medium, design: .rounded))
                            .foregroundStyle(SkillColors.textTertiary)
                    }
                    if label != L("sleep.rem") { Spacer(minLength: 0) }
                }
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Deep \(entry.deepMins) minutes, Light \(entry.lightMins) minutes, REM \(entry.remMins) minutes")
    }
}

// MARK: - Medium View

struct SleepMediumView: View {
    let entry: SleepEntry

    var body: some View {
        if !entry.daemonOnline || !entry.hasData {
            OfflineView(error: entry.error ?? .daemonOffline)
                .widgetURL(WidgetDeepLink.activity)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        } else {
            content
                .widgetURL(WidgetDeepLink.activity)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        }
    }

    private var content: some View {
        VStack(spacing: 0) {
            BrandHeader(subtitle: L("sleep.lastNight"))
                .padding(.bottom, 6)

            HStack(alignment: .center, spacing: 14) {
                // Left: Duration + quality
                VStack(spacing: 6) {
                    HStack(spacing: 4) {
                        Image(systemName: "moon.fill")
                            .font(.system(size: 14))
                            .foregroundStyle(Color(red: 0.40, green: 0.35, blue: 0.80))
                        Text(entry.durationText)
                            .font(.system(size: 24, weight: .bold, design: .rounded))
                            .foregroundStyle(SkillColors.textPrimary)
                    }

                    ArcGauge(score: entry.quality, size: 50, lineWidth: 5, label: L("sleep.quality"))
                }
                .frame(width: 80)

                Rectangle().fill(SkillColors.cardStroke).frame(width: 1).padding(.vertical, 4)

                // Right: Stage breakdown
                VStack(alignment: .leading, spacing: 5) {
                    ForEach(entry.stageBars, id: \.0) { label, mins, color in
                        HStack(spacing: 6) {
                            Circle().fill(color).frame(width: 6, height: 6)
                            Text(label)
                                .font(.system(size: 10, weight: .medium))
                                .foregroundStyle(SkillColors.textSecondary)
                                .frame(width: 36, alignment: .leading)
                            Spacer(minLength: 2)
                            Text(formatSleepDuration(mins))
                                .font(.system(size: 10, weight: .bold, design: .rounded))
                                .foregroundStyle(SkillColors.textPrimary)
                        }
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel("\(label) \(formatSleepDuration(mins))")
                    }
                }
            }
        }
    }

    private func formatSleepDuration(_ mins: Int) -> String {
        let h = mins / 60
        let m = mins % 60
        if h > 0 { return "\(h)h \(m)m" }
        return "\(m)m"
    }
}

// MARK: - Unified View

struct SleepWidgetView: View {
    @Environment(\.widgetFamily) var family
    let entry: SleepEntry

    var body: some View {
        switch family {
        case .systemMedium:
            SleepMediumView(entry: entry)
        default:
            SleepSmallView(entry: entry)
        }
    }
}

// MARK: - Widget

struct SleepWidget: Widget {
    let kind = "com.neuroskill.skill.widget.sleep"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: SleepProvider()) { entry in
            SleepWidgetView(entry: entry)
        }
        .configurationDisplayName(L("widget.sleep.name"))
        .description(L("widget.sleep.desc"))
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}

// MARK: - Previews

#Preview("Sleep — Small", as: .systemSmall) { SleepWidget() } timeline: { SleepEntry.placeholder }
#Preview("Sleep — Medium", as: .systemMedium) { SleepWidget() } timeline: { SleepEntry.placeholder }
#Preview("Sleep — Offline", as: .systemSmall) { SleepWidget() } timeline: { SleepEntry.offline(date: .now) }
