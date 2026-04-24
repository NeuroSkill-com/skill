// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import SwiftUI
import WidgetKit

// MARK: - Timeline Provider

struct BreakTimerProvider: TimelineProvider {
    func placeholder(in context: Context) -> BreakTimerEntry { .placeholder }

    func getSnapshot(in context: Context, completion: @escaping (BreakTimerEntry) -> Void) {
        if context.isPreview { completion(.placeholder); return }
        Task { completion(await fetchEntry()) }
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<BreakTimerEntry>) -> Void) {
        Task {
            let entry = await fetchEntry()
            // Refresh frequently — break timing is time-sensitive
            let next = Calendar.current.date(byAdding: .minute, value: 5, to: entry.date)!
            completion(Timeline(entries: [entry], policy: .after(next)))
        }
    }

    private func fetchEntry() async -> BreakTimerEntry {
        guard let status = try? await DaemonClient.shared.fetchStatus() else {
            return .offline(date: .now)
        }
        guard let fatigue = try? await DaemonClient.shared.fetchFatigue() else {
            return BreakTimerEntry(
                date: .now, continuousWorkMins: 0, suggestedIntervalMins: 52,
                naturalCycleMins: nil, confidence: 0, fatigued: false,
                connectivityState: status.state, daemonOnline: true, error: nil
            )
        }
        let breakTiming = try? await DaemonClient.shared.fetchBreakTiming()

        return BreakTimerEntry(
            date: .now,
            continuousWorkMins: UInt32(fatigue.continuousWorkMins),
            suggestedIntervalMins: breakTiming?.suggestedBreakIntervalMins ?? 52,
            naturalCycleMins: breakTiming?.naturalCycleMins,
            confidence: breakTiming?.confidence ?? 0,
            fatigued: fatigue.fatigued,
            connectivityState: status.state,
            daemonOnline: true, error: nil
        )
    }
}

// MARK: - Entry

struct BreakTimerEntry: TimelineEntry {
    let date: Date
    let continuousWorkMins: UInt32
    let suggestedIntervalMins: UInt32
    let naturalCycleMins: UInt32?
    let confidence: Float
    let fatigued: Bool
    let connectivityState: String
    let daemonOnline: Bool
    let error: WidgetError?

    var remainingMins: Int {
        max(0, Int(suggestedIntervalMins) - Int(continuousWorkMins))
    }

    var progress: Double {
        guard suggestedIntervalMins > 0 else { return 0 }
        return min(Double(continuousWorkMins) / Double(suggestedIntervalMins), 1.0)
    }

    var isOverdue: Bool { continuousWorkMins >= suggestedIntervalMins }

    static let placeholder = BreakTimerEntry(
        date: .now, continuousWorkMins: 38, suggestedIntervalMins: 52,
        naturalCycleMins: 48, confidence: 0.8, fatigued: false,
        connectivityState: "recording", daemonOnline: true, error: nil
    )

    static func offline(date: Date, error: WidgetError = .daemonOffline) -> BreakTimerEntry {
        BreakTimerEntry(
            date: date, continuousWorkMins: 0, suggestedIntervalMins: 52,
            naturalCycleMins: nil, confidence: 0, fatigued: false,
            connectivityState: "offline", daemonOnline: false, error: error
        )
    }
}

// MARK: - Widget View

struct BreakTimerWidgetView: View {
    @Environment(\.widgetFamily) var family
    let entry: BreakTimerEntry

    private var ringColor: Color {
        if entry.isOverdue { return SkillColors.alertRed }
        if entry.progress > 0.75 { return SkillColors.warmOrange }
        return SkillColors.flowGreen
    }

    var body: some View {
        #if os(iOS)
        switch family {
        case .accessoryCircular:
            lockScreenCircular
        case .accessoryRectangular:
            lockScreenRectangular
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
                    .widgetURL(WidgetDeepLink.dashboard)
                    .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
            } else {
                content
                    .widgetURL(WidgetDeepLink.dashboard)
                    .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
            }
        }
    }

    private var content: some View {
        VStack(spacing: 6) {
            // Header
            HStack(spacing: 4) {
                Image(systemName: "brain.filled.head.profile")
                    .font(.system(size: 8, weight: .medium))
                    .foregroundStyle(SkillColors.brand)
                Text(L("break.title"))
                    .font(.system(size: 8, weight: .bold, design: .rounded))
                    .foregroundStyle(SkillColors.brand)
                    .kerning(1.0)
                    .textCase(.uppercase)
                Spacer()
            }

            // Countdown ring
            ZStack {
                Circle()
                    .stroke(SkillColors.trackFill, style: StrokeStyle(lineWidth: 7, lineCap: .round))

                Circle()
                    .trim(from: 0, to: entry.progress)
                    .stroke(
                        ringColor,
                        style: StrokeStyle(lineWidth: 7, lineCap: .round)
                    )
                    .rotationEffect(.degrees(-90))
                    .shadow(color: ringColor.opacity(0.3), radius: 4, x: 0, y: 2)

                VStack(spacing: 1) {
                    if entry.isOverdue {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .font(.system(size: 14))
                            .foregroundStyle(SkillColors.alertRed)
                        Text(L("break.overdue"))
                            .font(.system(size: 8, weight: .bold))
                            .foregroundStyle(SkillColors.alertRed)
                    } else {
                        Text("\(entry.remainingMins)")
                            .font(.system(size: 26, weight: .bold, design: .rounded))
                            .foregroundStyle(ringColor)
                        Text(L("break.minsLeft"))
                            .font(.system(size: 8, weight: .medium))
                            .foregroundStyle(SkillColors.textTertiary)
                            .textCase(.uppercase)
                    }
                }
            }
            .frame(width: 78, height: 78)

            // Work duration
            HStack(spacing: 3) {
                Image(systemName: "deskclock")
                    .font(.system(size: 8))
                Text("\(entry.continuousWorkMins)m \(L("break.working"))")
                    .font(.system(size: 9, weight: .medium, design: .rounded))
            }
            .foregroundStyle(entry.fatigued ? SkillColors.warmOrange : SkillColors.textSecondary)
        }
        .padding(.top, 2)
    }

    // MARK: - Lock Screen

    #if os(iOS)
    private var lockScreenCircular: some View {
        Gauge(value: entry.progress) {
            Image(systemName: "cup.and.saucer.fill")
        } currentValueLabel: {
            Text("\(entry.remainingMins)")
                .font(.system(size: 12, weight: .bold, design: .rounded))
        }
        .gaugeStyle(.accessoryCircular)
        .accessibilityLabel("\(entry.remainingMins) minutes until break")
    }

    private var lockScreenRectangular: some View {
        AccessoryRectView(
            icon: entry.isOverdue ? "exclamationmark.triangle.fill" : "cup.and.saucer.fill",
            title: L("break.title"),
            value: entry.isOverdue ? L("break.overdue") : "\(entry.remainingMins)m",
            subtitle: "\(entry.continuousWorkMins)m \(L("break.working"))"
        )
    }
    #endif
}

// MARK: - Widget

struct BreakTimerWidget: Widget {
    let kind = "com.neuroskill.skill.widget.break-timer"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: BreakTimerProvider()) { entry in
            BreakTimerWidgetView(entry: entry)
        }
        .configurationDisplayName(L("widget.break.name"))
        .description(L("widget.break.desc"))
        .supportedFamilies({
            var families: [WidgetFamily] = [.systemSmall]
            #if os(iOS)
            families += [.accessoryCircular, .accessoryRectangular]
            #endif
            return families
        }())
    }
}

// MARK: - Previews

#Preview("Break Timer", as: .systemSmall) {
    BreakTimerWidget()
} timeline: {
    BreakTimerEntry.placeholder
}

#Preview("Break Timer — Offline", as: .systemSmall) {
    BreakTimerWidget()
} timeline: {
    BreakTimerEntry.offline(date: .now)
}

#if os(iOS)
#Preview("Break Timer — Lock Screen", as: .accessoryCircular) {
    BreakTimerWidget()
} timeline: {
    BreakTimerEntry.placeholder
}
#endif
