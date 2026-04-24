// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import SwiftUI
import WidgetKit

// MARK: - Timeline Provider

struct StreakProvider: TimelineProvider {
    func placeholder(in context: Context) -> StreakEntry { .placeholder }

    func getSnapshot(in context: Context, completion: @escaping (StreakEntry) -> Void) {
        if context.isPreview { completion(.placeholder); return }
        Task { completion(await fetchEntry()) }
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<StreakEntry>) -> Void) {
        Task {
            let entry = await fetchEntry()
            let next = Calendar.current.date(byAdding: .minute, value: 10, to: entry.date)!
            completion(Timeline(entries: [entry], policy: .after(next)))
        }
    }

    private func fetchEntry() async -> StreakEntry {
        let snap = await DaemonClient.shared.fetchSnapshot()
        guard snap.daemonOnline, let streak = snap.streak else {
            return .offline(date: .now, error: snap.error ?? .daemonOffline)
        }
        return StreakEntry(
            date: .now, streakDays: streak.currentStreakDays, longestStreak: streak.longestStreakDays,
            todayMins: streak.todayDeepMins, thresholdMins: streak.thresholdMins,
            qualifies: streak.todayQualifies, weeklyAvg: streak.weeklyAvgDeepMins,
            connectivityState: snap.connectivityState, deviceName: snap.status?.deviceName,
            daemonOnline: true, error: nil
        )
    }
}

// MARK: - Entry

struct StreakEntry: TimelineEntry {
    let date: Date
    let streakDays: UInt32
    let longestStreak: UInt32
    let todayMins: UInt32
    let thresholdMins: UInt32
    let qualifies: Bool
    let weeklyAvg: Float
    let connectivityState: String
    let deviceName: String?
    let daemonOnline: Bool
    let error: WidgetError?

    static let placeholder = StreakEntry(
        date: .now, streakDays: 12, longestStreak: 21,
        todayMins: 45, thresholdMins: 60, qualifies: false, weeklyAvg: 52,
        connectivityState: "recording", deviceName: "Muse S",
        daemonOnline: true, error: nil
    )

    static func offline(date: Date, error: WidgetError = .daemonOffline) -> StreakEntry {
        StreakEntry(date: date, streakDays: 0, longestStreak: 0,
                    todayMins: 0, thresholdMins: 60, qualifies: false, weeklyAvg: 0,
                    connectivityState: "offline", deviceName: nil,
                    daemonOnline: false, error: error)
    }
}

// MARK: - Widget View

struct StreakWidgetView: View {
    @Environment(\.widgetFamily) var family
    let entry: StreakEntry

    private var progress: Double {
        guard entry.thresholdMins > 0 else { return 0 }
        return min(Double(entry.todayMins) / Double(entry.thresholdMins), 1.0)
    }

    private var flameColor: Color {
        if entry.streakDays >= 14 { return SkillColors.alertRed }
        if entry.streakDays >= 7 { return SkillColors.warmOrange }
        if entry.streakDays >= 3 { return .yellow }
        return SkillColors.textTertiary
    }

    var body: some View {
        #if os(iOS)
        switch family {
        case .accessoryCircular:
            Gauge(value: progress) {
                Image(systemName: "flame.fill")
            } currentValueLabel: {
                Text("\(entry.streakDays)")
                    .font(.system(size: 12, weight: .bold, design: .rounded))
            }
            .gaugeStyle(.accessoryCircular)
            .accessibilityLabel("\(entry.streakDays) day streak")
        case .accessoryRectangular:
            AccessoryRectView(
                icon: "flame.fill", title: L("streak.label"),
                value: "\(entry.streakDays)d",
                subtitle: "\(entry.todayMins)/\(entry.thresholdMins)m \(L("streak.today").lowercased())"
            )
        default:
            systemSmallView
        }
        #else
        systemSmallView
        #endif
    }

    private var systemSmallView: some View {
        Group {
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
    }

    private var content: some View {
        VStack(spacing: 4) {
            HStack(spacing: 4) {
                Image(systemName: "brain.filled.head.profile")
                    .font(.system(size: 8, weight: .medium))
                    .foregroundStyle(SkillColors.brand)
                Text("STREAK")
                    .font(.system(size: 8, weight: .bold, design: .rounded))
                    .foregroundStyle(SkillColors.brand)
                    .kerning(1.2)
                Spacer()
            }

            Spacer(minLength: 0)

            ZStack {
                Circle().fill(flameColor.opacity(0.10)).frame(width: 56, height: 56)
                VStack(spacing: -2) {
                    Image(systemName: "flame.fill")
                        .font(.system(size: 18))
                        .foregroundStyle(LinearGradient(colors: [flameColor.opacity(0.75), flameColor], startPoint: .top, endPoint: .bottom))
                        .shadow(color: flameColor.opacity(0.35), radius: 4, x: 0, y: 2)
                    Text("\(entry.streakDays)")
                        .font(.system(size: 30, weight: .heavy, design: .rounded))
                        .foregroundStyle(SkillColors.textPrimary)
                }
            }
            .accessibilityLabel("\(entry.streakDays) day streak")

            Text(entry.streakDays == 1 ? L("streak.day") : L("streak.days"))
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(SkillColors.textTertiary)
                .textCase(.uppercase).kerning(0.6)

            Spacer(minLength: 0)

            SlimProgressBar(progress: progress,
                            fillColor: entry.qualifies ? SkillColors.flowGreen : SkillColors.calmBlue,
                            leftLabel: L("streak.today"),
                            rightLabel: "\(entry.todayMins)/\(entry.thresholdMins)m")

            ConnectivityBadge(state: entry.connectivityState, deviceName: entry.deviceName, battery: nil)
        }
        .padding(.top, 2)
    }
}

// MARK: - Widget

struct StreakWidget: Widget {
    let kind = "com.neuroskill.skill.widget.streak"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: StreakProvider()) { entry in
            StreakWidgetView(entry: entry)
        }
        .configurationDisplayName(L("widget.streak.name"))
        .description(L("widget.streak.desc"))
        .supportedFamilies({
            var families: [WidgetFamily] = [.systemSmall]
            #if os(iOS)
            families += [.accessoryCircular, .accessoryRectangular]
            #endif
            return families
        }())
    }
}

#Preview("Streak", as: .systemSmall) { StreakWidget() } timeline: { StreakEntry.placeholder }
#if os(iOS)
#Preview("Streak — Lock", as: .accessoryCircular) { StreakWidget() } timeline: { StreakEntry.placeholder }
#endif
