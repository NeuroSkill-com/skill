// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import SwiftUI
import WidgetKit

// MARK: - Timeline Provider

struct SessionStatusProvider: TimelineProvider {
    func placeholder(in context: Context) -> SessionStatusEntry { .placeholder }

    func getSnapshot(in context: Context, completion: @escaping (SessionStatusEntry) -> Void) {
        if context.isPreview { completion(.placeholder); return }
        Task { completion(await fetchEntry()) }
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<SessionStatusEntry>) -> Void) {
        Task {
            let entry = await fetchEntry()
            let next = Calendar.current.date(byAdding: .minute, value: 5, to: entry.date)!
            completion(Timeline(entries: [entry], policy: .after(next)))
        }
    }

    private func fetchEntry() async -> SessionStatusEntry {
        guard let status = try? await DaemonClient.shared.fetchStatus() else {
            return .offline(date: .now)
        }
        return SessionStatusEntry(
            date: .now,
            state: status.state,
            deviceName: status.deviceName,
            battery: status.battery,
            sampleCount: status.sampleCount,
            channelQuality: status.channelQuality ?? [],
            channelCount: status.eegChannelCount ?? 0,
            hasPpg: status.hasPpg ?? false,
            daemonOnline: true, error: nil
        )
    }
}

// MARK: - Entry

struct SessionStatusEntry: TimelineEntry {
    let date: Date
    let state: String
    let deviceName: String?
    let battery: Float
    let sampleCount: UInt64
    let channelQuality: [String]
    let channelCount: Int
    let hasPpg: Bool
    let daemonOnline: Bool
    let error: WidgetError?

    var isRecording: Bool { state == "recording" }
    var isConnected: Bool { state == "connected" || state == "recording" }

    var elapsedText: String {
        // Approximate elapsed from sample count (assuming ~256 Hz)
        guard sampleCount > 0 else { return "0:00" }
        let secs = sampleCount / 256
        let m = secs / 60
        let s = secs % 60
        if m >= 60 {
            return "\(m / 60)h \(m % 60)m"
        }
        return "\(m):\(String(format: "%02d", s))"
    }

    static let placeholder = SessionStatusEntry(
        date: .now, state: "recording", deviceName: "Muse S",
        battery: 72, sampleCount: 460800,
        channelQuality: ["good", "good", "fair", "good"],
        channelCount: 4, hasPpg: true, daemonOnline: true, error: nil
    )

    static func offline(date: Date, error: WidgetError = .daemonOffline) -> SessionStatusEntry {
        SessionStatusEntry(
            date: date, state: "idle", deviceName: nil,
            battery: 0, sampleCount: 0,
            channelQuality: [], channelCount: 0,
            hasPpg: false, daemonOnline: false, error: error
        )
    }
}

// MARK: - Widget View

struct SessionStatusWidgetView: View {
    @Environment(\.widgetFamily) var family
    let entry: SessionStatusEntry

    var body: some View {
        #if os(iOS)
        switch family {
        case .accessoryCircular:
            lockScreenCircular
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
                    .widgetURL(WidgetDeepLink.devices)
                    .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
            } else {
                content
                    .widgetURL(WidgetDeepLink.devices)
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
                Text(L("session.title"))
                    .font(.system(size: 8, weight: .bold, design: .rounded))
                    .foregroundStyle(SkillColors.brand)
                    .kerning(1.0)
                    .textCase(.uppercase)
                Spacer()
            }

            Spacer(minLength: 0)

            if entry.isConnected {
                connectedView
            } else {
                disconnectedView
            }

            Spacer(minLength: 0)
        }
        .padding(.top, 2)
    }

    private var connectedView: some View {
        VStack(spacing: 8) {
            // Device name + recording indicator
            HStack(spacing: 5) {
                // Pulsing dot for recording
                Circle()
                    .fill(entry.isRecording ? SkillColors.flowGreen : SkillColors.calmBlue)
                    .frame(width: 7, height: 7)
                    .overlay(
                        Circle()
                            .fill(entry.isRecording ? SkillColors.flowGreen.opacity(0.3) : .clear)
                            .frame(width: 13, height: 13)
                    )

                Text(entry.deviceName ?? L("status.connected"))
                    .font(.system(size: 13, weight: .semibold, design: .rounded))
                    .foregroundStyle(SkillColors.textPrimary)
                    .lineLimit(1)
            }

            // Elapsed time
            if entry.isRecording {
                HStack(spacing: 4) {
                    Image(systemName: "timer")
                        .font(.system(size: 9))
                    Text(entry.elapsedText)
                        .font(.system(size: 18, weight: .bold, design: .rounded))
                }
                .foregroundStyle(SkillColors.textPrimary)
            }

            // Signal quality dots
            if !entry.channelQuality.isEmpty {
                HStack(spacing: 4) {
                    Text(L("session.signal"))
                        .font(.system(size: 8, weight: .medium))
                        .foregroundStyle(SkillColors.textTertiary)

                    HStack(spacing: 3) {
                        ForEach(Array(entry.channelQuality.enumerated()), id: \.offset) { _, q in
                            Circle()
                                .fill(qualityColor(q))
                                .frame(width: 6, height: 6)
                        }
                    }
                }
            }

            // Battery + PPG indicator
            HStack(spacing: 8) {
                batteryView

                if entry.hasPpg {
                    HStack(spacing: 2) {
                        Image(systemName: "heart.fill")
                            .font(.system(size: 8))
                        Text("PPG")
                            .font(.system(size: 8, weight: .medium))
                    }
                    .foregroundStyle(SkillColors.alertRed.opacity(0.7))
                }
            }
        }
    }

    private var disconnectedView: some View {
        VStack(spacing: 8) {
            Image(systemName: "antenna.radiowaves.left.and.right.slash")
                .font(.system(size: 24))
                .foregroundStyle(SkillColors.textTertiary)

            Text(L("session.noDevice"))
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(SkillColors.textSecondary)

            Text(L("session.connectHint"))
                .font(.system(size: 9))
                .foregroundStyle(SkillColors.textTertiary)
                .multilineTextAlignment(.center)
        }
    }

    private var batteryView: some View {
        HStack(spacing: 2) {
            Image(systemName: batteryIcon(entry.battery))
                .font(.system(size: 9))
            Text("\(Int(entry.battery))%")
                .font(.system(size: 9, weight: .medium, design: .rounded))
        }
        .foregroundStyle(entry.battery < 20 ? SkillColors.alertRed : SkillColors.textTertiary)
    }

    // MARK: - Lock Screen

    #if os(iOS)
    private var lockScreenCircular: some View {
        Group {
            if entry.isConnected {
                Gauge(value: Double(min(entry.battery, 100)), in: 0...100) {
                    Image(systemName: "battery.100")
                } currentValueLabel: {
                    Text("\(Int(entry.battery))%")
                        .font(.system(size: 10, weight: .bold, design: .rounded))
                }
                .gaugeStyle(.accessoryCircular)
                .accessibilityLabel("\(entry.deviceName ?? "Device") battery \(Int(entry.battery)) percent")
            } else {
                ZStack {
                    AccessoryWidgetBackground()
                    Image(systemName: "antenna.radiowaves.left.and.right.slash")
                        .font(.system(size: 16))
                }
                .accessibilityLabel(L("session.noDevice"))
            }
        }
    }
    #endif

    // MARK: - Helpers

    private func qualityColor(_ q: String) -> Color {
        switch q.lowercased() {
        case "good": return SkillColors.flowGreen
        case "fair": return SkillColors.warmOrange
        default:     return SkillColors.alertRed
        }
    }

    private func batteryIcon(_ level: Float) -> String {
        switch level {
        case 75...: return "battery.100"
        case 50..<75: return "battery.75"
        case 25..<50: return "battery.50"
        default: return "battery.25"
        }
    }
}

// MARK: - Widget

struct SessionStatusWidget: Widget {
    let kind = "com.neuroskill.skill.widget.session"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: SessionStatusProvider()) { entry in
            SessionStatusWidgetView(entry: entry)
        }
        .configurationDisplayName(L("widget.session.name"))
        .description(L("widget.session.desc"))
        .supportedFamilies({
            var families: [WidgetFamily] = [.systemSmall]
            #if os(iOS)
            families += [.accessoryCircular]
            #endif
            return families
        }())
    }
}

// MARK: - Previews

#Preview("Session — Recording", as: .systemSmall) {
    SessionStatusWidget()
} timeline: {
    SessionStatusEntry.placeholder
}

#Preview("Session — Offline", as: .systemSmall) {
    SessionStatusWidget()
} timeline: {
    SessionStatusEntry.offline(date: .now)
}

#if os(iOS)
#Preview("Session — Lock Screen", as: .accessoryCircular) {
    SessionStatusWidget()
} timeline: {
    SessionStatusEntry.placeholder
}
#endif
