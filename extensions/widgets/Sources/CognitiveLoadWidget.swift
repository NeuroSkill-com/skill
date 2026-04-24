// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import SwiftUI
import WidgetKit

// MARK: - Timeline Provider

struct CognitiveLoadProvider: TimelineProvider {
    func placeholder(in context: Context) -> CognitiveLoadEntry { .placeholder }

    func getSnapshot(in context: Context, completion: @escaping (CognitiveLoadEntry) -> Void) {
        if context.isPreview { completion(.placeholder); return }
        Task { completion(await fetchEntry()) }
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<CognitiveLoadEntry>) -> Void) {
        Task {
            let entry = await fetchEntry()
            let next = Calendar.current.date(byAdding: .minute, value: 10, to: entry.date)!
            completion(Timeline(entries: [entry], policy: .after(next)))
        }
    }

    private func fetchEntry() async -> CognitiveLoadEntry {
        guard let rows = try? await DaemonClient.shared.fetchCognitiveLoad() else {
            return .offline(date: .now)
        }
        // Sort by load_score descending, take top 3
        let sorted = rows.sorted { $0.loadScore > $1.loadScore }
        let top = Array(sorted.prefix(3))
        let avgLoad = sorted.isEmpty ? Float(0) : sorted.map(\.loadScore).reduce(0, +) / Float(sorted.count)

        return CognitiveLoadEntry(
            date: .now,
            overallLoad: avgLoad,
            topItems: top,
            daemonOnline: true, error: nil
        )
    }
}

// MARK: - Entry

struct CognitiveLoadEntry: TimelineEntry {
    let date: Date
    let overallLoad: Float
    let topItems: [CognitiveLoadRow]
    let daemonOnline: Bool
    let error: WidgetError?

    var loadLevel: String {
        if overallLoad > 70 { return L("cog.heavy") }
        if overallLoad > 40 { return L("cog.moderate") }
        return L("cog.light")
    }

    var loadColor: Color {
        if overallLoad > 70 { return SkillColors.alertRed }
        if overallLoad > 40 { return SkillColors.warmOrange }
        return SkillColors.flowGreen
    }

    static let placeholder = CognitiveLoadEntry(
        date: .now, overallLoad: 58,
        topItems: [
            CognitiveLoadRow(key: "Rust", avgFocus: 55, avgUndos: 3.2, interactions: 240, totalSecs: 3600, loadScore: 72),
            CognitiveLoadRow(key: "TypeScript", avgFocus: 68, avgUndos: 1.8, interactions: 180, totalSecs: 2400, loadScore: 48),
            CognitiveLoadRow(key: "Swift", avgFocus: 45, avgUndos: 4.5, interactions: 90, totalSecs: 1200, loadScore: 82),
        ],
        daemonOnline: true, error: nil
    )

    static func offline(date: Date, error: WidgetError = .daemonOffline) -> CognitiveLoadEntry {
        CognitiveLoadEntry(date: date, overallLoad: 0, topItems: [],
                           daemonOnline: false, error: error)
    }
}

// MARK: - Widget View

struct CognitiveLoadWidgetView: View {
    @Environment(\.widgetFamily) var family
    let entry: CognitiveLoadEntry

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
        VStack(spacing: 5) {
            // Header
            HStack(spacing: 4) {
                Image(systemName: "brain.filled.head.profile")
                    .font(.system(size: 8, weight: .medium))
                    .foregroundStyle(SkillColors.brand)
                Text(L("cog.title"))
                    .font(.system(size: 8, weight: .bold, design: .rounded))
                    .foregroundStyle(SkillColors.brand)
                    .kerning(1.0)
                    .textCase(.uppercase)
                Spacer()
            }

            // Overall load gauge
            ZStack {
                Circle()
                    .trim(from: 0.15, to: 0.85)
                    .stroke(SkillColors.trackFill, style: StrokeStyle(lineWidth: 6, lineCap: .round))
                    .rotationEffect(.degrees(90))

                Circle()
                    .trim(from: 0.15, to: 0.15 + 0.7 * CGFloat(min(entry.overallLoad, 100)) / 100)
                    .stroke(
                        LinearGradient(
                            colors: [entry.loadColor.opacity(0.6), entry.loadColor],
                            startPoint: .leading, endPoint: .trailing
                        ),
                        style: StrokeStyle(lineWidth: 6, lineCap: .round)
                    )
                    .rotationEffect(.degrees(90))
                    .shadow(color: entry.loadColor.opacity(0.3), radius: 3, x: 0, y: 2)

                VStack(spacing: 0) {
                    Text("\(Int(entry.overallLoad))")
                        .font(.system(size: 20, weight: .bold, design: .rounded))
                        .foregroundStyle(entry.loadColor)
                    Text(entry.loadLevel)
                        .font(.system(size: 7, weight: .medium))
                        .foregroundStyle(SkillColors.textTertiary)
                        .textCase(.uppercase)
                }
            }
            .frame(width: 64, height: 64)

            // Top struggle items
            VStack(spacing: 3) {
                ForEach(Array(entry.topItems.prefix(2).enumerated()), id: \.offset) { _, item in
                    HStack(spacing: 4) {
                        Circle()
                            .fill(SkillColors.scoreColor(100 - item.loadScore))
                            .frame(width: 5, height: 5)
                        Text(shortKey(item.key))
                            .font(.system(size: 9, weight: .medium))
                            .foregroundStyle(SkillColors.textSecondary)
                            .lineLimit(1)
                        Spacer()
                        Text("\(Int(item.loadScore))")
                            .font(.system(size: 9, weight: .bold, design: .rounded))
                            .foregroundStyle(SkillColors.scoreColor(100 - item.loadScore))
                    }
                }
            }
        }
        .padding(.top, 2)
    }

    // MARK: - Lock Screen

    #if os(iOS)
    private var lockScreenCircular: some View {
        AccessoryGauge(score: entry.daemonOnline ? entry.overallLoad : 0, label: L("cog.title"))
    }
    #endif

    /// Shorten file paths or show just the language name.
    private func shortKey(_ key: String) -> String {
        if key.contains("/") {
            return String(key.split(separator: "/").last ?? Substring(key))
        }
        return key
    }
}

// MARK: - Widget

struct CognitiveLoadWidget: Widget {
    let kind = "com.neuroskill.skill.widget.cognitive-load"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: CognitiveLoadProvider()) { entry in
            CognitiveLoadWidgetView(entry: entry)
        }
        .configurationDisplayName(L("widget.cog.name"))
        .description(L("widget.cog.desc"))
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

#Preview("Cognitive Load", as: .systemSmall) {
    CognitiveLoadWidget()
} timeline: {
    CognitiveLoadEntry.placeholder
}

#Preview("Cognitive Load — Offline", as: .systemSmall) {
    CognitiveLoadWidget()
} timeline: {
    CognitiveLoadEntry.offline(date: .now)
}

#if os(iOS)
#Preview("Cognitive Load — Lock Screen", as: .accessoryCircular) {
    CognitiveLoadWidget()
} timeline: {
    CognitiveLoadEntry.placeholder
}
#endif
