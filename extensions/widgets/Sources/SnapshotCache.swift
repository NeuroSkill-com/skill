// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// File-based snapshot cache so widgets show stale-but-useful data
// when the daemon is offline, instead of a wall of error states.
// Writes to ~/.config/skill/daemon/widget-cache.json.

import Foundation

/// Cached data for all widget types.
struct CachedSnapshot: Codable {
    let updatedAt: TimeInterval      // Date().timeIntervalSince1970
    let flow: FlowStateResult?
    let fatigue: FatigueAlert?
    let streak: DeepWorkStreak?
    let status: DaemonStatus?
}

final class SnapshotCache: Sendable {
    static let shared = SnapshotCache()

    private let cacheURL: URL = {
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return appSupport
            .appendingPathComponent("skill")
            .appendingPathComponent("daemon")
            .appendingPathComponent("widget-cache.json")
    }()

    // MARK: - Write

    func save(_ snapshot: WidgetSnapshot) {
        guard snapshot.daemonOnline else { return }
        let cached = CachedSnapshot(
            updatedAt: Date().timeIntervalSince1970,
            flow: snapshot.flow,
            fatigue: snapshot.fatigue,
            streak: snapshot.streak,
            status: snapshot.status
        )
        do {
            let encoder = JSONEncoder()
            encoder.keyEncodingStrategy = .convertToSnakeCase
            let data = try encoder.encode(cached)
            try data.write(to: cacheURL, options: .atomic)
        } catch {
            // Non-fatal — cache write failure doesn't affect live data.
        }
    }

    // MARK: - Read

    func load() -> (snapshot: WidgetSnapshot, age: TimeInterval)? {
        guard let data = try? Data(contentsOf: cacheURL) else { return nil }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let cached = try? decoder.decode(CachedSnapshot.self, from: data) else { return nil }

        let age = Date().timeIntervalSince1970 - cached.updatedAt
        let snapshot = WidgetSnapshot(
            flow: cached.flow,
            fatigue: cached.fatigue,
            streak: cached.streak,
            status: cached.status,
            daemonOnline: false  // mark as offline — this is stale data
        )
        return (snapshot, age)
    }

    /// Formatted "Updated Xm ago" string. Returns nil if no cache.
    func staleLabelIfNeeded() -> String? {
        guard let (_, age) = load() else { return nil }
        if age < 60 { return nil } // fresh enough, no label
        let mins = Int(age / 60)
        if mins < 60 { return "\(mins)m ago" }
        let hours = mins / 60
        if hours < 24 { return "\(hours)h ago" }
        return "\(hours / 24)d ago"
    }
}
