// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import SwiftUI
import WidgetKit

// MARK: - Timeline Provider

struct BrainDashProvider: TimelineProvider {
    func placeholder(in context: Context) -> BrainDashEntry { .placeholder }

    func getSnapshot(in context: Context, completion: @escaping (BrainDashEntry) -> Void) {
        if context.isPreview { completion(.placeholder); return }
        Task { completion(await fetchEntry()) }
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<BrainDashEntry>) -> Void) {
        Task {
            let entry = await fetchEntry()
            let next = Calendar.current.date(byAdding: .minute, value: 5, to: entry.date)!
            completion(Timeline(entries: [entry], policy: .after(next)))
        }
    }

    private func fetchEntry() async -> BrainDashEntry {
        let snap = await DaemonClient.shared.fetchSnapshot()
        guard snap.daemonOnline else { return .offline(date: .now, error: snap.error ?? .daemonOffline) }
        return BrainDashEntry(
            date: .now, focusScore: snap.flow?.score ?? 0,
            inFlow: snap.flow?.inFlow ?? false, flowDurationSecs: snap.flow?.durationSecs ?? 0,
            fatigued: snap.fatigue?.fatigued ?? false,
            continuousWorkMins: snap.fatigue?.continuousWorkMins ?? 0,
            breakSuggestion: snap.fatigue?.suggestion ?? "",
            streakDays: snap.streak?.currentStreakDays ?? 0,
            todayDeepMins: snap.streak?.todayDeepMins ?? 0,
            thresholdMins: snap.streak?.thresholdMins ?? 60,
            connectivityState: snap.connectivityState,
            deviceName: snap.status?.deviceName, battery: snap.status?.battery,
            daemonOnline: true, error: nil
        )
    }
}

// MARK: - Entry

struct BrainDashEntry: TimelineEntry {
    let date: Date
    let focusScore: Float
    let inFlow: Bool
    let flowDurationSecs: UInt64
    let fatigued: Bool
    let continuousWorkMins: UInt64
    let breakSuggestion: String
    let streakDays: UInt32
    let todayDeepMins: UInt32
    let thresholdMins: UInt32
    let connectivityState: String
    let deviceName: String?
    let battery: Float?
    let daemonOnline: Bool
    let error: WidgetError?

    static let placeholder = BrainDashEntry(
        date: .now, focusScore: 72, inFlow: true, flowDurationSecs: 1800,
        fatigued: false, continuousWorkMins: 45, breakSuggestion: "Take a 5-minute break",
        streakDays: 12, todayDeepMins: 48, thresholdMins: 60,
        connectivityState: "recording", deviceName: "Muse S", battery: 85,
        daemonOnline: true, error: nil
    )

    static func offline(date: Date, error: WidgetError = .daemonOffline) -> BrainDashEntry {
        BrainDashEntry(date: date, focusScore: 0, inFlow: false, flowDurationSecs: 0,
                       fatigued: false, continuousWorkMins: 0, breakSuggestion: "",
                       streakDays: 0, todayDeepMins: 0, thresholdMins: 60,
                       connectivityState: "offline", deviceName: nil, battery: nil,
                       daemonOnline: false, error: error)
    }
}

// MARK: - Widget View

struct BrainDashWidgetView: View {
    let entry: BrainDashEntry

    private var deepWorkProgress: Double {
        guard entry.thresholdMins > 0 else { return 0 }
        return min(Double(entry.todayDeepMins) / Double(entry.thresholdMins), 1.0)
    }

    var body: some View {
        if !entry.daemonOnline {
            OfflineView(error: entry.error ?? .daemonOffline)
                .widgetURL(WidgetDeepLink.dashboard)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        } else {
            dashContent
                .widgetURL(WidgetDeepLink.dashboard)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        }
    }

    private var dashContent: some View {
        VStack(spacing: 0) {
            BrandHeader(subtitle: entry.inFlow ? L("focus.inFlow") : L("dash.monitoring"))
                .padding(.bottom, 6)

            HStack(alignment: .center, spacing: 12) {
                VStack(spacing: 5) {
                    ArcGauge(score: entry.focusScore, size: 68, lineWidth: 6, label: L("focus.label"))
                    if entry.inFlow {
                        StatusPill(icon: "bolt.fill", text: formatDuration(secs: entry.flowDurationSecs),
                                   color: SkillColors.flowGreen, filled: true)
                    }
                }
                .frame(width: 78)

                Rectangle().fill(SkillColors.cardStroke).frame(width: 1).padding(.vertical, 6)

                VStack(alignment: .leading, spacing: 6) {
                    MetricRow(
                        icon: entry.fatigued ? "exclamationmark.triangle.fill" : "checkmark.seal.fill",
                        iconColor: entry.fatigued ? SkillColors.warmOrange : SkillColors.flowGreen,
                        label: entry.fatigued ? L("dash.fatigued") : L("dash.fresh"),
                        value: "\(entry.continuousWorkMins)m",
                        valueColor: entry.fatigued ? SkillColors.warmOrange : SkillColors.textSecondary
                    )

                    MetricRow(icon: "flame.fill",
                              iconColor: entry.streakDays >= 7 ? SkillColors.warmOrange : SkillColors.textTertiary,
                              label: L("streak.label"),
                              value: "\(entry.streakDays)\(L("streak.day").prefix(1))",
                              valueColor: entry.streakDays >= 7 ? SkillColors.warmOrange : SkillColors.textPrimary)

                    SlimProgressBar(progress: deepWorkProgress,
                                    fillColor: entry.todayDeepMins >= entry.thresholdMins ? SkillColors.flowGreen : SkillColors.calmBlue,
                                    leftLabel: L("dash.deepWork"),
                                    rightLabel: "\(entry.todayDeepMins)/\(entry.thresholdMins)m")

                    if entry.fatigued && !entry.breakSuggestion.isEmpty {
                        HStack(spacing: 3) {
                            Image(systemName: "cup.and.saucer.fill").font(.system(size: 7))
                            Text(entry.breakSuggestion).font(.system(size: 8))
                        }
                        .foregroundStyle(SkillColors.warmOrange.opacity(0.8))
                        .lineLimit(1)
                    }
                }
            }

            Spacer(minLength: 2)
            ConnectivityBadge(state: entry.connectivityState, deviceName: entry.deviceName, battery: entry.battery)
        }
    }
}

// MARK: - Widget

struct BrainDashWidget: Widget {
    let kind = "com.neuroskill.skill.widget.brain-dash"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: BrainDashProvider()) { entry in
            BrainDashWidgetView(entry: entry)
        }
        .configurationDisplayName(L("widget.dash.name"))
        .description(L("widget.dash.desc"))
        .supportedFamilies([.systemMedium])
    }
}

#Preview("Brain Dash", as: .systemMedium) { BrainDashWidget() } timeline: { BrainDashEntry.placeholder }
#Preview("Brain Dash — Offline", as: .systemMedium) { BrainDashWidget() } timeline: { BrainDashEntry.offline(date: .now, error: .tokenMissing) }
