// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import SwiftUI
import WidgetKit

// MARK: - Timeline Provider

struct BandPowerProvider: TimelineProvider {
    func placeholder(in context: Context) -> BandPowerEntry { .placeholder }

    func getSnapshot(in context: Context, completion: @escaping (BandPowerEntry) -> Void) {
        if context.isPreview { completion(.placeholder); return }
        Task { completion(await fetchEntry()) }
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<BandPowerEntry>) -> Void) {
        Task {
            let entry = await fetchEntry()
            let next = Calendar.current.date(byAdding: .minute, value: 5, to: entry.date)!
            completion(Timeline(entries: [entry], policy: .after(next)))
        }
    }

    private func fetchEntry() async -> BandPowerEntry {
        guard let metrics = try? await DaemonClient.shared.fetchRecentMetrics(windowSecs: 300) else {
            return .offline(date: .now, error: DaemonClient.shared.detectError())
        }
        return BandPowerEntry(
            date: .now,
            delta: metrics.relDelta ?? 0,
            theta: metrics.relTheta ?? 0,
            alpha: metrics.relAlpha ?? 0,
            beta: metrics.relBeta ?? 0,
            gamma: metrics.relGamma ?? 0,
            relaxation: metrics.relaxation ?? 0,
            engagement: metrics.engagement ?? 0,
            daemonOnline: true, error: nil
        )
    }
}

// MARK: - Entry

struct BandPowerEntry: TimelineEntry {
    let date: Date
    let delta: Double
    let theta: Double
    let alpha: Double
    let beta: Double
    let gamma: Double
    let relaxation: Double
    let engagement: Double
    let daemonOnline: Bool
    let error: WidgetError?

    var hasData: Bool { delta + theta + alpha + beta + gamma > 0 }
    var bands: [(String, Double, Color)] {
        [
            ("δ", delta, Color(red: 0.55, green: 0.35, blue: 0.85)),  // purple
            ("θ", theta, Color(red: 0.30, green: 0.65, blue: 0.90)),  // blue
            ("α", alpha, SkillColors.flowGreen),                       // green
            ("β", beta,  SkillColors.warmOrange),                      // orange
            ("γ", gamma, SkillColors.alertRed),                        // red
        ]
    }

    static let placeholder = BandPowerEntry(
        date: .now, delta: 0.25, theta: 0.18, alpha: 0.30, beta: 0.20, gamma: 0.07,
        relaxation: 68, engagement: 55, daemonOnline: true, error: nil
    )

    static func offline(date: Date, error: WidgetError = .daemonOffline) -> BandPowerEntry {
        BandPowerEntry(date: date, delta: 0, theta: 0, alpha: 0, beta: 0, gamma: 0,
                       relaxation: 0, engagement: 0, daemonOnline: false, error: error)
    }
}

// MARK: - Small View

struct BandPowerSmallView: View {
    let entry: BandPowerEntry

    var body: some View {
        if !entry.daemonOnline || !entry.hasData {
            OfflineView(compact: true, error: entry.error ?? .daemonOffline)
                .widgetURL(WidgetDeepLink.dashboard)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        } else {
            content
                .widgetURL(WidgetDeepLink.dashboard)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        }
    }

    private var content: some View {
        VStack(spacing: 5) {
            HStack(spacing: 4) {
                Image(systemName: "brain.filled.head.profile")
                    .font(.system(size: 8, weight: .medium))
                    .foregroundStyle(SkillColors.brand)
                Text(L("eeg.title"))
                    .font(.system(size: 8, weight: .bold, design: .rounded))
                    .foregroundStyle(SkillColors.brand)
                    .kerning(1.0).textCase(.uppercase)
                Spacer()
            }

            // Band power bars
            HStack(alignment: .bottom, spacing: 6) {
                ForEach(entry.bands, id: \.0) { label, value, color in
                    bandBar(label: label, value: value, color: color)
                }
            }
            .frame(height: 70)
            .padding(.horizontal, 4)

            // Relaxation / Engagement
            HStack(spacing: 12) {
                miniScore(label: L("eeg.relax"), value: entry.relaxation, color: SkillColors.flowGreen)
                miniScore(label: L("eeg.engage"), value: entry.engagement, color: SkillColors.calmBlue)
            }
        }
        .padding(.top, 2)
    }

    private func bandBar(label: String, value: Double, color: Color) -> some View {
        VStack(spacing: 2) {
            RoundedRectangle(cornerRadius: 3)
                .fill(LinearGradient(colors: [color.opacity(0.4), color], startPoint: .bottom, endPoint: .top))
                .frame(width: 16, height: max(CGFloat(value) * 180, 4))
                .shadow(color: color.opacity(0.25), radius: 2, x: 0, y: 1)

            Text(label)
                .font(.system(size: 9, weight: .bold))
                .foregroundStyle(color)

            Text("\(Int(value * 100))%")
                .font(.system(size: 7, weight: .medium, design: .rounded))
                .foregroundStyle(SkillColors.textTertiary)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(label) \(Int(value * 100)) percent")
    }

    private func miniScore(label: String, value: Double, color: Color) -> some View {
        HStack(spacing: 3) {
            Circle().fill(color).frame(width: 4, height: 4)
            Text(label).font(.system(size: 8, weight: .medium)).foregroundStyle(SkillColors.textTertiary)
            Text("\(Int(value))").font(.system(size: 9, weight: .bold, design: .rounded)).foregroundStyle(color)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(label) \(Int(value))")
    }
}

// MARK: - Medium View

struct BandPowerMediumView: View {
    let entry: BandPowerEntry

    var body: some View {
        if !entry.daemonOnline || !entry.hasData {
            OfflineView(error: entry.error ?? .daemonOffline)
                .widgetURL(WidgetDeepLink.dashboard)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        } else {
            content
                .widgetURL(WidgetDeepLink.dashboard)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        }
    }

    private var content: some View {
        VStack(spacing: 0) {
            BrandHeader(subtitle: L("eeg.spectrum"))
                .padding(.bottom, 6)

            HStack(alignment: .center, spacing: 14) {
                // Left: Band bars
                HStack(alignment: .bottom, spacing: 8) {
                    ForEach(entry.bands, id: \.0) { label, value, color in
                        VStack(spacing: 2) {
                            RoundedRectangle(cornerRadius: 3)
                                .fill(LinearGradient(colors: [color.opacity(0.4), color], startPoint: .bottom, endPoint: .top))
                                .frame(width: 20, height: max(CGFloat(value) * 140, 4))
                                .shadow(color: color.opacity(0.2), radius: 2, x: 0, y: 1)
                            Text(label).font(.system(size: 10, weight: .bold)).foregroundStyle(color)
                            Text("\(Int(value * 100))%")
                                .font(.system(size: 7.5, weight: .medium, design: .rounded))
                                .foregroundStyle(SkillColors.textTertiary)
                        }
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel("\(label) \(Int(value * 100)) percent")
                    }
                }
                .frame(maxHeight: 80)

                Rectangle().fill(SkillColors.cardStroke).frame(width: 1).padding(.vertical, 4)

                // Right: Scores
                VStack(alignment: .leading, spacing: 8) {
                    MetricRow(icon: "leaf.fill", iconColor: SkillColors.flowGreen,
                              label: L("eeg.relax"), value: "\(Int(entry.relaxation))",
                              valueColor: SkillColors.flowGreen)
                    MetricRow(icon: "bolt.fill", iconColor: SkillColors.calmBlue,
                              label: L("eeg.engage"), value: "\(Int(entry.engagement))",
                              valueColor: SkillColors.calmBlue)

                    // Dominant band
                    let dominant = entry.bands.max(by: { $0.1 < $1.1 })
                    if let dom = dominant {
                        HStack(spacing: 4) {
                            Image(systemName: "waveform.path")
                                .font(.system(size: 9))
                                .foregroundStyle(dom.2)
                            Text("\(L("eeg.dominant")): \(dom.0)")
                                .font(.system(size: 9.5, weight: .semibold))
                                .foregroundStyle(SkillColors.textSecondary)
                        }
                    }
                }
            }
        }
    }
}

// MARK: - Widget

struct BandPowerWidgetView: View {
    @Environment(\.widgetFamily) var family
    let entry: BandPowerEntry

    var body: some View {
        switch family {
        case .systemMedium:
            BandPowerMediumView(entry: entry)
        default:
            BandPowerSmallView(entry: entry)
        }
    }
}

struct BandPowerWidget: Widget {
    let kind = "com.neuroskill.skill.widget.band-power"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: BandPowerProvider()) { entry in
            BandPowerWidgetView(entry: entry)
        }
        .configurationDisplayName(L("widget.eeg.name"))
        .description(L("widget.eeg.desc"))
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}

// MARK: - Previews

#Preview("EEG Bands — Small", as: .systemSmall) { BandPowerWidget() } timeline: { BandPowerEntry.placeholder }
#Preview("EEG Bands — Medium", as: .systemMedium) { BandPowerWidget() } timeline: { BandPowerEntry.placeholder }
#Preview("EEG Bands — Offline", as: .systemSmall) { BandPowerWidget() } timeline: { BandPowerEntry.offline(date: .now) }
