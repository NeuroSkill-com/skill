// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import SwiftUI
import WidgetKit

// MARK: - Localization Helper

/// Shorthand for `String(localized:)`.
func L(_ key: String.LocalizationValue) -> String {
    String(localized: key)
}

// MARK: - Deep Links

/// URL scheme for widget → app navigation.
enum WidgetDeepLink {
    static let scheme = "neuroskill"

    static let dashboard   = URL(string: "\(scheme)://dashboard")!
    static let devices     = URL(string: "\(scheme)://devices")!
    static let settings    = URL(string: "\(scheme)://settings")!
    static let activity    = URL(string: "\(scheme)://activity")!
    static let session     = URL(string: "\(scheme)://session")!
    static let heartRate   = URL(string: "\(scheme)://heart-rate")!
}

// MARK: - Error States

/// Granular error type for widget offline views.
enum WidgetError {
    case daemonOffline      // Daemon process not reachable
    case tokenMissing       // Auth token file not found
    case apiError(String)   // Daemon returned an error

    var icon: String {
        switch self {
        case .daemonOffline: return "power.circle"
        case .tokenMissing:  return "key.slash"
        case .apiError:      return "exclamationmark.icloud"
        }
    }

    var title: String {
        switch self {
        case .daemonOffline: return L("error.daemonOffline")
        case .tokenMissing:  return L("error.tokenMissing")
        case .apiError:      return L("error.apiError")
        }
    }

    var hint: String {
        switch self {
        case .daemonOffline: return L("error.startApp")
        case .tokenMissing:  return L("error.reinstall")
        case .apiError(let msg): return msg
        }
    }
}

// MARK: - Brand Colors (adaptive light / dark)

enum SkillColors {
    // Primary brand — teal/cyan
    static let brand = Color("AccentColor")
    static let brandFallback = Color(red: 0.28, green: 0.83, blue: 0.98)

    // Semantic colors
    static let flowGreen = Color(red: 0.20, green: 0.84, blue: 0.58)
    static let warmOrange = Color(red: 1.0, green: 0.62, blue: 0.22)
    static let alertRed = Color(red: 0.94, green: 0.30, blue: 0.32)
    static let calmBlue = Color(red: 0.36, green: 0.53, blue: 0.96)

    // Text — adapts to color scheme
    static let textPrimary = Color.primary
    static let textSecondary = Color.secondary
    static let textTertiary = Color(light: Color(white: 0.45), dark: Color(white: 0.55))

    // Surfaces
    static let cardStroke = Color(light: Color.black.opacity(0.06), dark: Color.white.opacity(0.06))
    static let trackFill = Color(light: Color.black.opacity(0.06), dark: Color.white.opacity(0.10))

    /// Gradient for gauge arcs.
    static func scoreGradient(score: Float) -> LinearGradient {
        let c = scoreColor(score)
        return LinearGradient(colors: [c.opacity(0.65), c], startPoint: .leading, endPoint: .trailing)
    }

    static func scoreColor(_ score: Float) -> Color {
        switch score {
        case 70...: return flowGreen
        case 40..<70: return warmOrange
        default: return alertRed
        }
    }
}

// MARK: - Adaptive Color Helper

extension Color {
    /// Creates a color that adapts to light/dark appearance.
    init(light: Color, dark: Color) {
        self.init(nsColor: NSColor(name: nil) { appearance in
            let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            return NSColor(isDark ? dark : light)
        })
    }
}

// MARK: - Widget Background

struct AdaptiveWidgetBackground: View {
    @Environment(\.colorScheme) var colorScheme

    var body: some View {
        ContainerRelativeShape()
            .fill(
                LinearGradient(
                    colors: colorScheme == .dark
                        ? [Color(white: 0.06), Color(white: 0.10)]
                        : [Color(white: 0.97), Color(white: 0.93)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            )
    }
}

// MARK: - Reusable Components

/// Arc gauge for score display (270-degree sweep).
struct ArcGauge: View {
    let score: Float
    let size: CGFloat
    let lineWidth: CGFloat
    let label: String

    init(score: Float, size: CGFloat = 80, lineWidth: CGFloat = 7, label: String = "") {
        self.score = score
        self.size = size
        self.lineWidth = lineWidth
        self.label = label.isEmpty ? L("focus.label") : label
    }

    private var normalised: CGFloat { CGFloat(min(max(score, 0), 100)) / 100 }

    var body: some View {
        ZStack {
            Circle()
                .trim(from: 0.15, to: 0.85)
                .stroke(SkillColors.trackFill, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
                .rotationEffect(.degrees(90))

            Circle()
                .trim(from: 0.15, to: 0.15 + 0.7 * normalised)
                .stroke(
                    SkillColors.scoreGradient(score: score),
                    style: StrokeStyle(lineWidth: lineWidth, lineCap: .round)
                )
                .rotationEffect(.degrees(90))
                .shadow(color: SkillColors.scoreColor(score).opacity(0.3), radius: 4, x: 0, y: 2)

            VStack(spacing: 0) {
                Text("\(Int(score))")
                    .font(.system(size: size * 0.36, weight: .bold, design: .rounded))
                    .foregroundStyle(SkillColors.scoreColor(score))
                Text(label)
                    .font(.system(size: size * 0.11, weight: .medium))
                    .foregroundStyle(SkillColors.textTertiary)
                    .textCase(.uppercase)
                    .kerning(0.8)
            }
        }
        .frame(width: size, height: size)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(label) \(Int(score)) out of 100")
        .accessibilityValue(scoreAccessibilityLevel)
    }

    private var scoreAccessibilityLevel: String {
        switch score {
        case 70...: return L("a11y.good")
        case 40..<70: return L("a11y.moderate")
        default: return L("a11y.low")
        }
    }
}

/// Pill badge with icon and text.
struct StatusPill: View {
    let icon: String
    let text: String
    let color: Color
    let filled: Bool

    init(icon: String, text: String, color: Color, filled: Bool = false) {
        self.icon = icon
        self.text = text
        self.color = color
        self.filled = filled
    }

    var body: some View {
        HStack(spacing: 3) {
            Image(systemName: icon)
                .font(.system(size: 8, weight: .semibold))
            Text(text)
                .font(.system(size: 9, weight: .semibold, design: .rounded))
        }
        .padding(.horizontal, 7)
        .padding(.vertical, 3)
        .foregroundStyle(filled ? .white : color)
        .background(
            Capsule()
                .fill(filled ? color : color.opacity(0.12))
        )
        .accessibilityElement(children: .combine)
    }
}

/// Metric row with icon, label, and right-aligned value.
struct MetricRow: View {
    let icon: String
    let iconColor: Color
    let label: String
    let value: String
    let valueColor: Color

    init(icon: String, iconColor: Color, label: String, value: String, valueColor: Color = .primary) {
        self.icon = icon
        self.iconColor = iconColor
        self.label = label
        self.value = value
        self.valueColor = valueColor
    }

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: icon)
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(iconColor)
                .frame(width: 14, alignment: .center)

            Text(label)
                .font(.system(size: 10.5))
                .foregroundStyle(SkillColors.textSecondary)

            Spacer(minLength: 2)

            Text(value)
                .font(.system(size: 10.5, weight: .semibold, design: .rounded))
                .foregroundStyle(valueColor)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(label): \(value)")
    }
}

/// Slim progress bar with labels.
struct SlimProgressBar: View {
    let progress: Double
    let fillColor: Color
    let leftLabel: String
    let rightLabel: String

    var body: some View {
        VStack(spacing: 3) {
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule()
                        .fill(SkillColors.trackFill)
                    Capsule()
                        .fill(
                            LinearGradient(
                                colors: [fillColor.opacity(0.55), fillColor],
                                startPoint: .leading,
                                endPoint: .trailing
                            )
                        )
                        .frame(width: max(geo.size.width * progress, 4))
                        .shadow(color: fillColor.opacity(0.2), radius: 2, x: 0, y: 1)
                }
            }
            .frame(height: 5)
            .clipShape(Capsule())

            HStack {
                Text(leftLabel)
                    .font(.system(size: 8, weight: .medium))
                    .foregroundStyle(SkillColors.textTertiary)
                Spacer()
                Text(rightLabel)
                    .font(.system(size: 8, weight: .semibold, design: .rounded))
                    .foregroundStyle(SkillColors.textSecondary)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(leftLabel): \(rightLabel)")
        .accessibilityValue("\(Int(progress * 100)) percent")
    }
}

/// Branded header row.
struct BrandHeader: View {
    let subtitle: String

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: "brain.filled.head.profile")
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(SkillColors.brand)
            Text(L("brand.name"))
                .font(.system(size: 9, weight: .bold, design: .rounded))
                .foregroundStyle(SkillColors.brand)

            Spacer()

            Text(subtitle)
                .font(.system(size: 8, weight: .medium))
                .foregroundStyle(SkillColors.textTertiary)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("NeuroSkill, \(subtitle)")
    }
}

/// Device connectivity indicator (small dot + label).
struct ConnectivityBadge: View {
    let state: String
    let deviceName: String?
    let battery: Float?

    private var dotColor: Color {
        switch state {
        case "recording": return SkillColors.flowGreen
        case "connected": return SkillColors.calmBlue
        case "scanning":  return SkillColors.warmOrange
        default:          return SkillColors.textTertiary
        }
    }

    private var stateText: String {
        switch state {
        case "recording":  return L("status.recording")
        case "connected":  return deviceName ?? L("status.connected")
        case "scanning":   return L("status.scanning")
        case "idle":       return L("status.idle")
        default:           return L("status.noDevice")
        }
    }

    var body: some View {
        HStack(spacing: 4) {
            Circle()
                .fill(dotColor)
                .frame(width: 5, height: 5)
                .overlay(
                    Circle()
                        .fill(dotColor.opacity(0.3))
                        .frame(width: 9, height: 9)
                )

            Text(stateText)
                .font(.system(size: 8, weight: .medium))
                .foregroundStyle(SkillColors.textTertiary)
                .lineLimit(1)

            if let battery, battery > 0 {
                Text("·")
                    .font(.system(size: 8))
                    .foregroundStyle(SkillColors.textTertiary)
                HStack(spacing: 1) {
                    Image(systemName: batteryIcon(battery))
                        .font(.system(size: 7))
                    Text("\(Int(battery))%")
                        .font(.system(size: 7, weight: .medium, design: .rounded))
                }
                .foregroundStyle(battery < 20 ? SkillColors.alertRed : SkillColors.textTertiary)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilityText)
    }

    private var accessibilityText: String {
        var parts = [stateText]
        if let battery, battery > 0 {
            parts.append("\(L("status.battery")) \(Int(battery))%")
        }
        return parts.joined(separator: ", ")
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

/// Offline/error placeholder view with granular error type.
struct OfflineView: View {
    @Environment(\.colorScheme) var colorScheme
    let compact: Bool
    let error: WidgetError

    init(compact: Bool = false, error: WidgetError = .daemonOffline) {
        self.compact = compact
        self.error = error
    }

    var body: some View {
        VStack(spacing: compact ? 6 : 10) {
            Image(systemName: error.icon)
                .font(.system(size: compact ? 20 : 26))
                .foregroundStyle(
                    LinearGradient(
                        colors: [
                            SkillColors.brand.opacity(colorScheme == .dark ? 0.4 : 0.5),
                            SkillColors.brand.opacity(colorScheme == .dark ? 0.15 : 0.25)
                        ],
                        startPoint: .top, endPoint: .bottom
                    )
                )

            VStack(spacing: 2) {
                Text(error.title)
                    .font(.system(size: compact ? 10 : 12, weight: .semibold, design: .rounded))
                    .foregroundStyle(SkillColors.textSecondary)
                Text(error.hint)
                    .font(.system(size: compact ? 8 : 9))
                    .foregroundStyle(SkillColors.textTertiary)
                    .multilineTextAlignment(.center)
                    .lineLimit(2)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(error.title). \(error.hint)")
    }
}

/// Small "Updated Xm ago" label for stale cached data.
struct StaleLabel: View {
    let staleText: String?

    var body: some View {
        if let text = staleText {
            HStack(spacing: 2) {
                Image(systemName: "clock.arrow.circlepath")
                    .font(.system(size: 6))
                Text(text)
                    .font(.system(size: 7, weight: .medium))
            }
            .foregroundStyle(SkillColors.warmOrange.opacity(0.7))
            .accessibilityLabel("Last updated \(text)")
        }
    }
}

// MARK: - Lock Screen Components (accessory families)

#if os(iOS)
/// Compact circular gauge for lock screen / menu bar.
struct AccessoryGauge: View {
    let score: Float
    let label: String

    var body: some View {
        Gauge(value: Double(min(max(score, 0), 100)), in: 0...100) {
            Text(label)
        } currentValueLabel: {
            Text("\(Int(score))")
                .font(.system(size: 12, weight: .bold, design: .rounded))
        }
        .gaugeStyle(.accessoryCircular)
        .accessibilityLabel("\(label) \(Int(score)) out of 100")
    }
}

/// Compact rectangular view for lock screen.
struct AccessoryRectView: View {
    let icon: String
    let title: String
    let value: String
    let subtitle: String

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: icon)
                .font(.system(size: 12))

            VStack(alignment: .leading, spacing: 1) {
                Text(value)
                    .font(.system(size: 14, weight: .bold, design: .rounded))
                Text(subtitle)
                    .font(.system(size: 9))
                    .foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(title): \(value), \(subtitle)")
    }
}
#endif

// MARK: - Duration Formatting

func formatDuration(secs: UInt64) -> String {
    let mins = secs / 60
    if mins >= 60 {
        let h = mins / 60
        let m = mins % 60
        return m > 0 ? "\(h)h \(m)m" : "\(h)h"
    }
    return "\(mins)m"
}
