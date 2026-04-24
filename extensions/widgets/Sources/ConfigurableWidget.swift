// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import AppIntents
import SwiftUI
import WidgetKit

// MARK: - Metric Selection Intent

enum WidgetMetric: String, AppEnum {
    case focus        = "focus"
    case streak       = "streak"
    case breakTimer   = "break"
    case heartRate    = "heartRate"
    case cognitiveLoad = "cognitiveLoad"

    static var typeDisplayRepresentation = TypeDisplayRepresentation(name: "Metric")

    static var caseDisplayRepresentations: [WidgetMetric: DisplayRepresentation] = [
        .focus:         .init(title: "Focus Score",      image: .init(systemName: "brain.head.profile")),
        .streak:        .init(title: "Deep Work Streak", image: .init(systemName: "flame.fill")),
        .breakTimer:    .init(title: "Break Timer",      image: .init(systemName: "cup.and.saucer.fill")),
        .heartRate:     .init(title: "Heart Rate",       image: .init(systemName: "heart.fill")),
        .cognitiveLoad: .init(title: "Cognitive Load",   image: .init(systemName: "brain")),
    ]
}

struct SelectMetricIntent: WidgetConfigurationIntent {
    static var title: LocalizedStringResource = "Select Metric"
    static var description = IntentDescription("Choose which brain metric to display.")

    @Parameter(title: "Metric", default: .focus)
    var metric: WidgetMetric
}

// MARK: - Timeline Provider

struct ConfigurableProvider: AppIntentTimelineProvider {
    func placeholder(in context: Context) -> ConfigurableEntry {
        ConfigurableEntry(date: .now, metric: .focus, snapshot: nil, daemonOnline: true, error: nil)
    }

    func snapshot(for configuration: SelectMetricIntent, in context: Context) async -> ConfigurableEntry {
        if context.isPreview {
            return ConfigurableEntry(date: .now, metric: configuration.metric, snapshot: nil, daemonOnline: true, error: nil)
        }
        return await fetchEntry(metric: configuration.metric)
    }

    func timeline(for configuration: SelectMetricIntent, in context: Context) async -> Timeline<ConfigurableEntry> {
        let entry = await fetchEntry(metric: configuration.metric)
        let next = Calendar.current.date(byAdding: .minute, value: 5, to: entry.date)!
        return Timeline(entries: [entry], policy: .after(next))
    }

    private func fetchEntry(metric: WidgetMetric) async -> ConfigurableEntry {
        let snap = await DaemonClient.shared.fetchSnapshot()
        guard snap.daemonOnline else {
            return ConfigurableEntry(date: .now, metric: metric, snapshot: nil,
                                     daemonOnline: false, error: snap.error ?? .daemonOffline)
        }
        return ConfigurableEntry(date: .now, metric: metric, snapshot: snap,
                                 daemonOnline: true, error: nil)
    }
}

// MARK: - Entry

struct ConfigurableEntry: TimelineEntry {
    let date: Date
    let metric: WidgetMetric
    let snapshot: WidgetSnapshot?
    let daemonOnline: Bool
    let error: WidgetError?
}

// MARK: - Widget View

struct ConfigurableWidgetView: View {
    let entry: ConfigurableEntry

    var body: some View {
        if !entry.daemonOnline {
            OfflineView(compact: true, error: entry.error ?? .daemonOffline)
                .widgetURL(WidgetDeepLink.dashboard)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        } else {
            metricView
                .widgetURL(deepLink)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        }
    }

    private var deepLink: URL {
        switch entry.metric {
        case .focus, .cognitiveLoad: return WidgetDeepLink.dashboard
        case .streak:       return WidgetDeepLink.activity
        case .breakTimer:   return WidgetDeepLink.dashboard
        case .heartRate:    return WidgetDeepLink.heartRate
        }
    }

    @ViewBuilder
    private var metricView: some View {
        switch entry.metric {
        case .focus:
            focusView
        case .streak:
            streakView
        case .breakTimer:
            breakView
        case .heartRate:
            heartRateView
        case .cognitiveLoad:
            cogLoadView
        }
    }

    // MARK: - Metric Views

    private var focusView: some View {
        let flow = entry.snapshot?.flow
        return VStack(spacing: 6) {
            header("FOCUS")
            ArcGauge(score: flow?.score ?? 0, size: 82, lineWidth: 7, label: L("focus.label"))
            if flow?.inFlow == true {
                StatusPill(icon: "bolt.fill",
                           text: "\(L("focus.inFlow")) \(formatDuration(secs: flow?.durationSecs ?? 0))",
                           color: SkillColors.flowGreen, filled: true)
            } else {
                StatusPill(icon: "bolt.slash", text: L("focus.notInFlow"), color: SkillColors.textTertiary)
            }
        }.padding(.top, 2)
    }

    private var streakView: some View {
        let streak = entry.snapshot?.streak
        let days = streak?.currentStreakDays ?? 0
        let flameColor: Color = days >= 14 ? SkillColors.alertRed : days >= 7 ? SkillColors.warmOrange : days >= 3 ? .yellow : SkillColors.textTertiary
        let progress = (streak?.thresholdMins ?? 60) > 0
            ? min(Double(streak?.todayDeepMins ?? 0) / Double(streak?.thresholdMins ?? 60), 1.0)
            : 0.0

        return VStack(spacing: 4) {
            header("STREAK")
            Spacer(minLength: 0)
            ZStack {
                Circle().fill(flameColor.opacity(0.10)).frame(width: 56, height: 56)
                VStack(spacing: -2) {
                    Image(systemName: "flame.fill").font(.system(size: 18))
                        .foregroundStyle(flameColor)
                    Text("\(days)")
                        .font(.system(size: 30, weight: .heavy, design: .rounded))
                        .foregroundStyle(SkillColors.textPrimary)
                }
            }
            Text(L("streak.days")).font(.system(size: 10, weight: .medium))
                .foregroundStyle(SkillColors.textTertiary).textCase(.uppercase)
            Spacer(minLength: 0)
            SlimProgressBar(progress: progress,
                            fillColor: (streak?.todayQualifies ?? false) ? SkillColors.flowGreen : SkillColors.calmBlue,
                            leftLabel: L("streak.today"),
                            rightLabel: "\(streak?.todayDeepMins ?? 0)/\(streak?.thresholdMins ?? 60)m")
        }.padding(.top, 2)
    }

    private var breakView: some View {
        let fatigue = entry.snapshot?.fatigue
        let workMins = UInt32(fatigue?.continuousWorkMins ?? 0)
        let interval: UInt32 = 52
        let remaining = max(0, Int(interval) - Int(workMins))
        let progress = interval > 0 ? min(Double(workMins) / Double(interval), 1.0) : 0
        let isOverdue = workMins >= interval
        let ringColor: Color = isOverdue ? SkillColors.alertRed : progress > 0.75 ? SkillColors.warmOrange : SkillColors.flowGreen

        return VStack(spacing: 6) {
            header("BREAK")
            ZStack {
                Circle().stroke(SkillColors.trackFill, style: StrokeStyle(lineWidth: 7, lineCap: .round))
                Circle().trim(from: 0, to: progress)
                    .stroke(ringColor, style: StrokeStyle(lineWidth: 7, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                VStack(spacing: 1) {
                    if isOverdue {
                        Image(systemName: "exclamationmark.triangle.fill").font(.system(size: 14)).foregroundStyle(SkillColors.alertRed)
                        Text(L("break.overdue")).font(.system(size: 8, weight: .bold)).foregroundStyle(SkillColors.alertRed)
                    } else {
                        Text("\(remaining)").font(.system(size: 26, weight: .bold, design: .rounded)).foregroundStyle(ringColor)
                        Text(L("break.minsLeft")).font(.system(size: 8, weight: .medium)).foregroundStyle(SkillColors.textTertiary).textCase(.uppercase)
                    }
                }
            }.frame(width: 78, height: 78)
            HStack(spacing: 3) {
                Image(systemName: "deskclock").font(.system(size: 8))
                Text("\(workMins)m \(L("break.working"))").font(.system(size: 9, weight: .medium, design: .rounded))
            }.foregroundStyle((fatigue?.fatigued ?? false) ? SkillColors.warmOrange : SkillColors.textSecondary)
        }.padding(.top, 2)
    }

    private var heartRateView: some View {
        VStack(spacing: 5) {
            header("HEART RATE")
            Spacer(minLength: 0)
            HStack(spacing: 4) {
                Image(systemName: "heart.fill").font(.system(size: 16)).foregroundStyle(SkillColors.alertRed)
                Text("--").font(.system(size: 32, weight: .heavy, design: .rounded)).foregroundStyle(SkillColors.textPrimary)
                Text("bpm").font(.system(size: 10, weight: .medium)).foregroundStyle(SkillColors.textTertiary).offset(y: 6)
            }
            Text(L("hr.noPpg")).font(.system(size: 9)).foregroundStyle(SkillColors.textTertiary).multilineTextAlignment(.center)
            Spacer(minLength: 0)
        }.padding(.top, 2)
    }

    private var cogLoadView: some View {
        VStack(spacing: 5) {
            header("COG LOAD")
            ArcGauge(score: 0, size: 64, lineWidth: 6, label: L("cog.title"))
            Text(L("cog.light")).font(.system(size: 9, weight: .medium))
                .foregroundStyle(SkillColors.textTertiary).textCase(.uppercase)
        }.padding(.top, 2)
    }

    private func header(_ title: String) -> some View {
        HStack(spacing: 4) {
            Image(systemName: "brain.filled.head.profile")
                .font(.system(size: 8, weight: .medium))
                .foregroundStyle(SkillColors.brand)
            Text(title)
                .font(.system(size: 8, weight: .bold, design: .rounded))
                .foregroundStyle(SkillColors.brand)
                .kerning(1.0)
            Spacer()
        }
    }
}

// MARK: - Widget

struct ConfigurableMetricWidget: Widget {
    let kind = "com.neuroskill.skill.widget.configurable"

    var body: some WidgetConfiguration {
        AppIntentConfiguration(kind: kind, intent: SelectMetricIntent.self, provider: ConfigurableProvider()) { entry in
            ConfigurableWidgetView(entry: entry)
        }
        .configurationDisplayName("NeuroSkill Metric")
        .description("Choose any brain metric to display.")
        .supportedFamilies([.systemSmall])
    }
}

// MARK: - Previews

#Preview("Configurable — Focus", as: .systemSmall) {
    ConfigurableMetricWidget()
} timeline: {
    ConfigurableEntry(date: .now, metric: .focus, snapshot: nil, daemonOnline: true, error: nil)
}

#Preview("Configurable — Offline", as: .systemSmall) {
    ConfigurableMetricWidget()
} timeline: {
    ConfigurableEntry(date: .now, metric: .streak, snapshot: nil, daemonOnline: false, error: .daemonOffline)
}
