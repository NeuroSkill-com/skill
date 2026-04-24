// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import SwiftUI
import WidgetKit

// MARK: - Timeline Provider

struct CalendarMindProvider: TimelineProvider {
    func placeholder(in context: Context) -> CalendarMindEntry { .placeholder }

    func getSnapshot(in context: Context, completion: @escaping (CalendarMindEntry) -> Void) {
        if context.isPreview { completion(.placeholder); return }
        Task { completion(await fetchEntry()) }
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<CalendarMindEntry>) -> Void) {
        Task {
            let entry = await fetchEntry()
            let next = Calendar.current.date(byAdding: .minute, value: 15, to: entry.date)!
            completion(Timeline(entries: [entry], policy: .after(next)))
        }
    }

    private func fetchEntry() async -> CalendarMindEntry {
        let snap = await DaemonClient.shared.fetchSnapshot()
        guard snap.daemonOnline else {
            return .offline(date: .now, error: snap.error ?? .daemonOffline)
        }

        // Fetch calendar events, meeting recovery, and optimal hours in parallel
        async let calEvents = DaemonClient.shared.fetchCalendarEvents()
        async let recovery = DaemonClient.shared.fetchMeetingRecovery()
        async let hours = DaemonClient.shared.fetchOptimalHours()

        let events = (try? await calEvents) ?? []
        let meetingRecovery = try? await recovery
        let optimalHours = try? await hours

        // Build calendar slots with mind state annotations
        let slots = buildSlots(
            events: events,
            recovery: meetingRecovery,
            optimalHours: optimalHours,
            flow: snap.flow
        )

        // Detect anticipation patterns
        let anticipations = detectAnticipations(
            recovery: meetingRecovery,
            optimalHours: optimalHours
        )

        return CalendarMindEntry(
            date: .now,
            slots: slots,
            currentHour: UInt8(Calendar.current.component(.hour, from: .now)),
            avgRecoverySecs: meetingRecovery?.avgRecoverySecs ?? 0,
            anticipations: anticipations,
            overallFocus: snap.flow?.score ?? 0,
            inFlow: snap.flow?.inFlow ?? false,
            daemonOnline: true, error: nil
        )
    }

    /// Merge calendar events with focus/recovery data into annotated slots.
    private func buildSlots(
        events: [CalendarEvent],
        recovery: MeetingRecoveryResult?,
        optimalHours: OptimalHoursResult?,
        flow: FlowStateResult?
    ) -> [CalendarSlot] {
        let now = Int64(Date().timeIntervalSince1970)
        let hourScores = Dictionary(uniqueKeysWithValues:
            (optimalHours?.hours ?? []).map { (Int($0.hour), $0) }
        )
        let recoveryByTitle = Dictionary(uniqueKeysWithValues:
            (recovery?.meetings ?? []).map { ($0.title.lowercased(), $0) }
        )

        // Filter to non-all-day events, sorted by start time
        let filtered = events
            .filter { !$0.allDay && $0.endUtc > now - 3600 }
            .sorted { $0.startUtc < $1.startUtc }

        var slots: [CalendarSlot] = []
        var previousEnd: Int64 = now - 1800 // show from 30 min ago

        for event in filtered.prefix(8) {
            // Gap before event
            if event.startUtc > previousEnd + 300 {
                let gapHour = Int(previousEnd / 3600) % 24
                let gapScore = hourScores[gapHour]
                slots.append(CalendarSlot(
                    startUtc: previousEnd, endUtc: event.startUtc,
                    title: L("cal.deepWork"),
                    isEvent: false,
                    avgFocus: gapScore?.avgFocus,
                    wasInFlow: false, recoverySecs: nil,
                    anticipationDrop: nil, platform: nil
                ))
            }

            // Event slot
            let eventHour = Int(event.startUtc / 3600) % 24
            let hourScore = hourScores[eventHour]
            let rec = recoveryByTitle[event.title.lowercased()]

            // Anticipation: check if focus is typically lower in the hour before this event
            let preHour = (eventHour - 1 + 24) % 24
            let preScore = hourScores[preHour]
            var anticipation: Float? = nil
            if let pre = preScore?.avgFocus, let during = hourScore?.avgFocus, pre < during - 5 {
                anticipation = during - pre
            }

            slots.append(CalendarSlot(
                startUtc: event.startUtc, endUtc: event.endUtc,
                title: event.title,
                isEvent: true,
                avgFocus: hourScore?.avgFocus,
                wasInFlow: false,
                recoverySecs: rec?.recoverySecs,
                anticipationDrop: anticipation,
                platform: rec?.platform
            ))

            previousEnd = event.endUtc
        }

        return slots
    }

    /// Detect statistically significant anticipation patterns.
    private func detectAnticipations(
        recovery: MeetingRecoveryResult?,
        optimalHours: OptimalHoursResult?
    ) -> [AnticipationInsight] {
        guard let meetings = recovery?.meetings, meetings.count >= 3 else { return [] }

        var insights: [AnticipationInsight] = []

        // Find meetings with consistently high recovery times (>15 min)
        let highRecovery = meetings.filter { ($0.recoverySecs ?? 0) > 900 }
        if highRecovery.count >= 2 {
            let avgRec = highRecovery.compactMap(\.recoverySecs).reduce(0, +) / UInt64(highRecovery.count)
            insights.append(AnticipationInsight(
                icon: "arrow.clockwise",
                text: String(format: L("cal.slowRecovery"), avgRec / 60)
            ))
        }

        // Worst hours correlation
        if let worst = optimalHours?.worstHours.first {
            insights.append(AnticipationInsight(
                icon: "exclamationmark.triangle",
                text: String(format: L("cal.avoidHour"), formatHour12(worst))
            ))
        }

        return Array(insights.prefix(2))
    }
}

// MARK: - Supporting Types

struct AnticipationInsight: Identifiable {
    let id = UUID()
    let icon: String
    let text: String
}

// MARK: - Entry

struct CalendarMindEntry: TimelineEntry {
    let date: Date
    let slots: [CalendarSlot]
    let currentHour: UInt8
    let avgRecoverySecs: UInt64
    let anticipations: [AnticipationInsight]
    let overallFocus: Float
    let inFlow: Bool
    let daemonOnline: Bool
    let error: WidgetError?

    static let placeholder: CalendarMindEntry = {
        let now = Int64(Date().timeIntervalSince1970)
        return CalendarMindEntry(
            date: .now,
            slots: [
                CalendarSlot(startUtc: now - 1800, endUtc: now - 600, title: "Deep Work",
                             isEvent: false, avgFocus: 78, wasInFlow: true, recoverySecs: nil,
                             anticipationDrop: nil, platform: nil),
                CalendarSlot(startUtc: now - 600, endUtc: now + 1200, title: "Sprint Planning",
                             isEvent: true, avgFocus: 52, wasInFlow: false, recoverySecs: 480,
                             anticipationDrop: 12, platform: "zoom"),
                CalendarSlot(startUtc: now + 1200, endUtc: now + 3600, title: "Deep Work",
                             isEvent: false, avgFocus: 72, wasInFlow: false, recoverySecs: nil,
                             anticipationDrop: nil, platform: nil),
                CalendarSlot(startUtc: now + 3600, endUtc: now + 5400, title: "1:1 with Alex",
                             isEvent: true, avgFocus: 65, wasInFlow: false, recoverySecs: 300,
                             anticipationDrop: 8, platform: "teams"),
                CalendarSlot(startUtc: now + 5400, endUtc: now + 9000, title: "Deep Work",
                             isEvent: false, avgFocus: 82, wasInFlow: false, recoverySecs: nil,
                             anticipationDrop: nil, platform: nil),
            ],
            currentHour: UInt8(Calendar.current.component(.hour, from: .now)),
            avgRecoverySecs: 420,
            anticipations: [
                AnticipationInsight(icon: "arrow.clockwise", text: "Avg 7m recovery after meetings"),
                AnticipationInsight(icon: "exclamationmark.triangle", text: "Avoid scheduling deep work at 3pm"),
            ],
            overallFocus: 68, inFlow: false,
            daemonOnline: true, error: nil
        )
    }()

    static func offline(date: Date, error: WidgetError = .daemonOffline) -> CalendarMindEntry {
        CalendarMindEntry(date: date, slots: [], currentHour: 0, avgRecoverySecs: 0,
                          anticipations: [], overallFocus: 0, inFlow: false,
                          daemonOnline: false, error: error)
    }
}

// MARK: - Widget View

struct CalendarMindWidgetView: View {
    let entry: CalendarMindEntry

    var body: some View {
        if !entry.daemonOnline {
            OfflineView(error: entry.error ?? .daemonOffline)
                .widgetURL(WidgetDeepLink.activity)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        } else if entry.slots.isEmpty {
            emptyView
                .widgetURL(WidgetDeepLink.activity)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        } else {
            calendarView
                .widgetURL(WidgetDeepLink.activity)
                .containerBackground(for: .widget) { AdaptiveWidgetBackground() }
        }
    }

    // MARK: - Calendar Timeline

    private var calendarView: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                BrandHeader(subtitle: entry.inFlow ? L("focus.inFlow") : L("cal.subtitle"))
            }
            .padding(.bottom, 4)

            // Timeline slots
            VStack(spacing: 2) {
                ForEach(entry.slots.prefix(5)) { slot in
                    slotRow(slot)
                }
            }

            Spacer(minLength: 2)

            // Anticipation insights
            if !entry.anticipations.isEmpty {
                Divider().padding(.vertical, 2).opacity(0.3)
                VStack(spacing: 3) {
                    ForEach(entry.anticipations.prefix(2)) { insight in
                        insightRow(insight)
                    }
                }
            }
        }
    }

    // MARK: - Slot Row

    private func slotRow(_ slot: CalendarSlot) -> some View {
        HStack(spacing: 6) {
            // Time column
            VStack(spacing: 0) {
                Text(formatTime(slot.startUtc))
                    .font(.system(size: 8, weight: .medium, design: .rounded))
                    .foregroundStyle(SkillColors.textTertiary)
                    .frame(width: 32, alignment: .trailing)
            }

            // Focus bar
            focusBar(slot: slot)

            // Title + annotations
            VStack(alignment: .leading, spacing: 1) {
                HStack(spacing: 3) {
                    if slot.isEvent {
                        platformIcon(slot.platform)
                    }
                    Text(slot.title)
                        .font(.system(size: 9.5, weight: slot.isEvent ? .semibold : .regular))
                        .foregroundStyle(slot.isEvent ? SkillColors.textPrimary : SkillColors.textSecondary)
                        .lineLimit(1)
                }

                HStack(spacing: 6) {
                    // Focus score
                    if let focus = slot.avgFocus {
                        HStack(spacing: 2) {
                            Circle()
                                .fill(SkillColors.scoreColor(focus))
                                .frame(width: 4, height: 4)
                            Text("\(Int(focus))")
                                .font(.system(size: 7.5, weight: .bold, design: .rounded))
                                .foregroundStyle(SkillColors.scoreColor(focus))
                        }
                        .accessibilityLabel("Focus \(Int(focus))")
                    }

                    // Recovery time
                    if let rec = slot.recoverySecs, rec > 0 {
                        HStack(spacing: 1) {
                            Image(systemName: "arrow.clockwise")
                                .font(.system(size: 6))
                            Text("\(rec / 60)m")
                                .font(.system(size: 7, weight: .medium, design: .rounded))
                        }
                        .foregroundStyle(rec > 600 ? SkillColors.warmOrange : SkillColors.textTertiary)
                        .accessibilityLabel("Recovery \(rec / 60) minutes")
                    }

                    // Anticipation marker
                    if let drop = slot.anticipationDrop, drop > 5 {
                        HStack(spacing: 1) {
                            Image(systemName: "arrow.down.right")
                                .font(.system(size: 6))
                            Text("-\(Int(drop))")
                                .font(.system(size: 7, weight: .bold, design: .rounded))
                        }
                        .foregroundStyle(SkillColors.alertRed.opacity(0.8))
                        .accessibilityLabel("Focus drops \(Int(drop)) points before this event")
                    }
                }
            }

            Spacer(minLength: 0)
        }
        .padding(.vertical, 2)
    }

    // MARK: - Focus Bar

    private func focusBar(slot: CalendarSlot) -> some View {
        let focus = slot.avgFocus ?? 0
        let barHeight: CGFloat = slot.isEvent ? 24 : 18

        return RoundedRectangle(cornerRadius: 2)
            .fill(
                slot.isEvent
                    ? LinearGradient(colors: [SkillColors.calmBlue.opacity(0.3), SkillColors.calmBlue.opacity(0.5)],
                                     startPoint: .top, endPoint: .bottom)
                    : LinearGradient(colors: [SkillColors.scoreColor(focus).opacity(0.15), SkillColors.scoreColor(focus).opacity(0.25)],
                                     startPoint: .top, endPoint: .bottom)
            )
            .frame(width: 4, height: barHeight)
            .overlay(alignment: .bottom) {
                if slot.isEvent, let drop = slot.anticipationDrop, drop > 5 {
                    // Red tick mark for anticipation drop
                    Rectangle()
                        .fill(SkillColors.alertRed.opacity(0.6))
                        .frame(width: 4, height: 3)
                }
            }
    }

    // MARK: - Platform Icon

    private func platformIcon(_ platform: String?) -> some View {
        let icon: String
        switch platform?.lowercased() {
        case "zoom":        icon = "video.fill"
        case "teams":       icon = "person.2.fill"
        case "slack":       icon = "bubble.left.fill"
        case "google_meet": icon = "video.fill"
        case "facetime":    icon = "phone.fill"
        case "discord":     icon = "headphones"
        case "webex":       icon = "video.fill"
        default:            icon = "calendar"
        }
        return Image(systemName: icon)
            .font(.system(size: 7))
            .foregroundStyle(SkillColors.textTertiary)
    }

    // MARK: - Insight Row

    private func insightRow(_ insight: AnticipationInsight) -> some View {
        HStack(spacing: 4) {
            Image(systemName: insight.icon)
                .font(.system(size: 7, weight: .medium))
                .foregroundStyle(SkillColors.warmOrange)
            Text(insight.text)
                .font(.system(size: 8, weight: .medium))
                .foregroundStyle(SkillColors.textSecondary)
                .lineLimit(1)
        }
    }

    // MARK: - Empty State

    private var emptyView: some View {
        VStack(spacing: 8) {
            BrandHeader(subtitle: L("cal.subtitle"))
            Spacer()
            Image(systemName: "calendar.badge.checkmark")
                .font(.system(size: 28))
                .foregroundStyle(SkillColors.brand.opacity(0.4))
            Text(L("cal.noEvents"))
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(SkillColors.textSecondary)
            Text(L("cal.freeDay"))
                .font(.system(size: 9))
                .foregroundStyle(SkillColors.textTertiary)
            Spacer()
        }
    }

    // MARK: - Helpers

    private func formatTime(_ utc: Int64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(utc))
        let fmt = DateFormatter()
        fmt.dateFormat = "h:mm"
        return fmt.string(from: date)
    }
}

func formatHour12(_ h: UInt8) -> String {
    let fmt = DateFormatter()
    fmt.dateFormat = "ha"
    var comps = DateComponents()
    comps.hour = Int(h)
    let date = Calendar.current.date(from: comps) ?? .now
    return fmt.string(from: date).lowercased()
}

// MARK: - Widget

struct CalendarMindWidget: Widget {
    let kind = "com.neuroskill.skill.widget.calendar-mind"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: CalendarMindProvider()) { entry in
            CalendarMindWidgetView(entry: entry)
        }
        .configurationDisplayName(L("widget.cal.name"))
        .description(L("widget.cal.desc"))
        .supportedFamilies([.systemLarge, .systemMedium])
    }
}

// MARK: - Previews

#Preview("Calendar Mind — Full", as: .systemLarge) {
    CalendarMindWidget()
} timeline: {
    CalendarMindEntry.placeholder
}

#Preview("Calendar Mind — Medium", as: .systemMedium) {
    CalendarMindWidget()
} timeline: {
    CalendarMindEntry.placeholder
}

#Preview("Calendar Mind — Offline", as: .systemLarge) {
    CalendarMindWidget()
} timeline: {
    CalendarMindEntry.offline(date: .now)
}

#Preview("Calendar Mind — Empty", as: .systemLarge) {
    CalendarMindWidget()
} timeline: {
    CalendarMindEntry(date: .now, slots: [], currentHour: 10, avgRecoverySecs: 0,
                      anticipations: [], overallFocus: 72, inFlow: true,
                      daemonOnline: true, error: nil)
}
