// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import SwiftUI
import WidgetKit

// MARK: - Timeline Provider

struct HeartRateProvider: TimelineProvider {
    func placeholder(in context: Context) -> HeartRateEntry { .placeholder }

    func getSnapshot(in context: Context, completion: @escaping (HeartRateEntry) -> Void) {
        if context.isPreview { completion(.placeholder); return }
        Task { completion(await fetchEntry()) }
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<HeartRateEntry>) -> Void) {
        Task {
            let entry = await fetchEntry()
            let next = Calendar.current.date(byAdding: .minute, value: 5, to: entry.date)!
            completion(Timeline(entries: [entry], policy: .after(next)))
        }
    }

    private func fetchEntry() async -> HeartRateEntry {
        guard let metrics = try? await DaemonClient.shared.fetchRecentMetrics(windowSecs: 300) else {
            return .offline(date: .now)
        }
        let status = try? await DaemonClient.shared.fetchStatus()
        return HeartRateEntry(
            date: .now,
            hr: metrics.hr,
            rmssd: metrics.rmssd,
            stressIndex: metrics.stressIndex,
            respiratoryRate: metrics.respiratoryRate,
            spo2: metrics.spo2Estimate,
            hasPpg: status?.hasPpg ?? false,
            deviceName: status?.deviceName,
            daemonOnline: true, error: nil
        )
    }
}

// MARK: - Entry

struct HeartRateEntry: TimelineEntry {
    let date: Date
    let hr: Double?
    let rmssd: Double?
    let stressIndex: Double?
    let respiratoryRate: Double?
    let spo2: Double?
    let hasPpg: Bool
    let deviceName: String?
    let daemonOnline: Bool
    let error: WidgetError?

    var hasData: Bool { hr != nil && hr! > 0 }

    var stressLevel: String {
        guard let si = stressIndex, si > 0 else { return L("hr.unknown") }
        if si > 150 { return L("hr.high") }
        if si > 50 { return L("hr.moderate") }
        return L("hr.low")
    }

    var stressColor: Color {
        guard let si = stressIndex, si > 0 else { return SkillColors.textTertiary }
        if si > 150 { return SkillColors.alertRed }
        if si > 50 { return SkillColors.warmOrange }
        return SkillColors.flowGreen
    }

    static let placeholder = HeartRateEntry(
        date: .now, hr: 68, rmssd: 42, stressIndex: 65,
        respiratoryRate: 14, spo2: 98,
        hasPpg: true, deviceName: "Muse S", daemonOnline: true, error: nil
    )

    static func offline(date: Date, error: WidgetError = .daemonOffline) -> HeartRateEntry {
        HeartRateEntry(
            date: date, hr: nil, rmssd: nil, stressIndex: nil,
            respiratoryRate: nil, spo2: nil,
            hasPpg: false, deviceName: nil, daemonOnline: false, error: error
        )
    }
}

// MARK: - Widget View

struct HeartRateWidgetView: View {
    @Environment(\.widgetFamily) var family
    let entry: HeartRateEntry

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
                    .widgetURL(WidgetDeepLink.heartRate)
                    .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
            } else if !entry.hasData {
                noPpgView
                    .widgetURL(WidgetDeepLink.heartRate)
                    .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
            } else {
                content
                    .widgetURL(WidgetDeepLink.heartRate)
                    .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
            }
        }
    }

    private var content: some View {
        VStack(spacing: 5) {
            // Header
            HStack(spacing: 4) {
                Image(systemName: "brain.filled.head.profile")
                    .font(.system(size: 8, weight: .medium))
                    .foregroundStyle(SkillColors.brand)
                Text(L("hr.title"))
                    .font(.system(size: 8, weight: .bold, design: .rounded))
                    .foregroundStyle(SkillColors.brand)
                    .kerning(1.0)
                    .textCase(.uppercase)
                Spacer()
            }

            Spacer(minLength: 0)

            // Heart rate
            HStack(spacing: 4) {
                Image(systemName: "heart.fill")
                    .font(.system(size: 16))
                    .foregroundStyle(SkillColors.alertRed)
                    .shadow(color: SkillColors.alertRed.opacity(0.3), radius: 4, x: 0, y: 2)
                Text("\(Int(entry.hr ?? 0))")
                    .font(.system(size: 32, weight: .heavy, design: .rounded))
                    .foregroundStyle(SkillColors.textPrimary)
                Text("bpm")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(SkillColors.textTertiary)
                    .offset(y: 6)
            }

            // Stress level pill
            StatusPill(
                icon: "waveform.path.ecg",
                text: stressLabel,
                color: entry.stressColor,
                filled: (entry.stressIndex ?? 0) > 150
            )

            Spacer(minLength: 0)

            // HRV + respiratory row
            HStack(spacing: 10) {
                if let rmssd = entry.rmssd, rmssd > 0 {
                    miniMetric(label: "HRV", value: "\(Int(rmssd))ms")
                }
                if let rr = entry.respiratoryRate, rr > 0 {
                    miniMetric(label: L("hr.resp"), value: "\(Int(rr))")
                }
                if let spo2 = entry.spo2, spo2 > 0 {
                    miniMetric(label: "SpO\u{2082}", value: "\(Int(spo2))%")
                }
            }
        }
        .padding(.top, 2)
    }

    private var stressLabel: String {
        "\(L("hr.stress")): \(entry.stressLevel)"
    }

    private func miniMetric(label: String, value: String) -> some View {
        VStack(spacing: 1) {
            Text(value)
                .font(.system(size: 10, weight: .bold, design: .rounded))
                .foregroundStyle(SkillColors.textPrimary)
            Text(label)
                .font(.system(size: 7, weight: .medium))
                .foregroundStyle(SkillColors.textTertiary)
        }
    }

    private var noPpgView: some View {
        VStack(spacing: 8) {
            Image(systemName: "heart.slash")
                .font(.system(size: 24))
                .foregroundStyle(SkillColors.textTertiary)
            Text(L("hr.noPpg"))
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(SkillColors.textSecondary)
                .multilineTextAlignment(.center)
        }
    }

    // MARK: - Lock Screen

    #if os(iOS)
    private var lockScreenCircular: some View {
        Gauge(value: Double(min(entry.hr ?? 0, 200)), in: 40...200) {
            Image(systemName: "heart.fill")
        } currentValueLabel: {
            Text("\(Int(entry.hr ?? 0))")
                .font(.system(size: 12, weight: .bold, design: .rounded))
        }
        .gaugeStyle(.accessoryCircular)
        .accessibilityLabel("Heart rate \(Int(entry.hr ?? 0)) bpm")
    }

    private var lockScreenRectangular: some View {
        AccessoryRectView(
            icon: "heart.fill",
            title: L("hr.title"),
            value: entry.hasData ? "\(Int(entry.hr ?? 0)) bpm" : "--",
            subtitle: entry.rmssd.map { "HRV \(Int($0))ms" } ?? L("hr.noPpg")
        )
    }
    #endif
}

// MARK: - Widget

struct HeartRateWidget: Widget {
    let kind = "com.neuroskill.skill.widget.heart-rate"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: HeartRateProvider()) { entry in
            HeartRateWidgetView(entry: entry)
        }
        .configurationDisplayName(L("widget.hr.name"))
        .description(L("widget.hr.desc"))
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

#Preview("Heart Rate", as: .systemSmall) {
    HeartRateWidget()
} timeline: {
    HeartRateEntry.placeholder
}

#Preview("Heart Rate — Offline", as: .systemSmall) {
    HeartRateWidget()
} timeline: {
    HeartRateEntry.offline(date: .now)
}

#if os(iOS)
#Preview("Heart Rate — Lock Screen", as: .accessoryCircular) {
    HeartRateWidget()
} timeline: {
    HeartRateEntry.placeholder
}
#endif
