// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import SwiftUI
import WidgetKit

// MARK: - Timeline Provider

struct FocusProvider: TimelineProvider {
    func placeholder(in context: Context) -> FocusEntry { .placeholder }

    func getSnapshot(in context: Context, completion: @escaping (FocusEntry) -> Void) {
        if context.isPreview { completion(.placeholder); return }
        Task { completion(await fetchEntry()) }
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<FocusEntry>) -> Void) {
        Task {
            let entry = await fetchEntry()
            let next = Calendar.current.date(byAdding: .minute, value: 5, to: entry.date)!
            completion(Timeline(entries: [entry], policy: .after(next)))
        }
    }

    private func fetchEntry() async -> FocusEntry {
        let snap = await DaemonClient.shared.fetchSnapshot()
        guard snap.daemonOnline else {
            return .offline(date: .now, error: snap.error ?? .daemonOffline)
        }
        return FocusEntry(
            date: .now,
            score: snap.flow?.score ?? 0,
            inFlow: snap.flow?.inFlow ?? false,
            flowDurationSecs: snap.flow?.durationSecs ?? 0,
            connectivityState: snap.connectivityState,
            deviceName: snap.status?.deviceName,
            battery: snap.status?.battery,
            daemonOnline: true, error: nil
        )
    }
}

// MARK: - Entry

struct FocusEntry: TimelineEntry {
    let date: Date
    let score: Float
    let inFlow: Bool
    let flowDurationSecs: UInt64
    let connectivityState: String
    let deviceName: String?
    let battery: Float?
    let daemonOnline: Bool
    let error: WidgetError?

    static let placeholder = FocusEntry(
        date: .now, score: 72, inFlow: true, flowDurationSecs: 1800,
        connectivityState: "recording", deviceName: "Muse S", battery: 85,
        daemonOnline: true, error: nil
    )

    static func offline(date: Date, error: WidgetError = .daemonOffline) -> FocusEntry {
        FocusEntry(
            date: date, score: 0, inFlow: false, flowDurationSecs: 0,
            connectivityState: "offline", deviceName: nil, battery: nil,
            daemonOnline: false, error: error
        )
    }
}

// MARK: - Widget View

struct FocusWidgetView: View {
    @Environment(\.widgetFamily) var family
    let entry: FocusEntry

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
            HStack(spacing: 4) {
                Image(systemName: "brain.filled.head.profile")
                    .font(.system(size: 8, weight: .medium))
                    .foregroundStyle(SkillColors.brand)
                Text("FOCUS")
                    .font(.system(size: 8, weight: .bold, design: .rounded))
                    .foregroundStyle(SkillColors.brand)
                    .kerning(1.2)
                Spacer()
            }

            ArcGauge(score: entry.score, size: 82, lineWidth: 7, label: L("focus.label"))

            if entry.inFlow {
                StatusPill(
                    icon: "bolt.fill",
                    text: "\(L("focus.inFlow")) \(formatDuration(secs: entry.flowDurationSecs))",
                    color: SkillColors.flowGreen, filled: true
                )
            } else {
                StatusPill(icon: "bolt.slash", text: L("focus.notInFlow"), color: SkillColors.textTertiary)
            }

            ConnectivityBadge(state: entry.connectivityState, deviceName: entry.deviceName, battery: entry.battery)
        }
        .padding(.top, 2)
    }

    // MARK: - Lock Screen

    #if os(iOS)
    private var lockScreenCircular: some View {
        AccessoryGauge(score: entry.daemonOnline ? entry.score : 0, label: L("focus.label"))
    }

    private var lockScreenRectangular: some View {
        AccessoryRectView(
            icon: entry.inFlow ? "bolt.fill" : "brain.head.profile",
            title: L("focus.label"),
            value: entry.daemonOnline ? "\(Int(entry.score))" : "--",
            subtitle: entry.inFlow ? "\(L("focus.inFlow")) \(formatDuration(secs: entry.flowDurationSecs))" : L("focus.notInFlow")
        )
    }
    #endif
}

// MARK: - Widget

struct FocusWidget: Widget {
    let kind = "com.neuroskill.skill.widget.focus"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: FocusProvider()) { entry in
            FocusWidgetView(entry: entry)
        }
        .configurationDisplayName(L("widget.focus.name"))
        .description(L("widget.focus.desc"))
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

#Preview("Focus — In Flow", as: .systemSmall) {
    FocusWidget()
} timeline: {
    FocusEntry.placeholder
}

#Preview("Focus — Offline", as: .systemSmall) {
    FocusWidget()
} timeline: {
    FocusEntry.offline(date: .now)
}

#if os(iOS)
#Preview("Focus — Lock Screen", as: .accessoryCircular) {
    FocusWidget()
} timeline: {
    FocusEntry.placeholder
}
#endif
