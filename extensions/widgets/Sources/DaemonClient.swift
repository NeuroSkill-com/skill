// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import Foundation

/// HTTP client for the skill-daemon REST API.
/// Reads the auth token from ~/.config/skill/daemon/auth.token
/// and connects to the daemon on localhost:18444.
final class DaemonClient: Sendable {
    static let shared = DaemonClient()

    private let baseURL: URL
    private let session: URLSession

    private init() {
        let port = ProcessInfo.processInfo.environment["SKILL_DAEMON_PORT"] ?? "18444"
        baseURL = URL(string: "http://127.0.0.1:\(port)")!
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 5
        config.timeoutIntervalForResource = 8
        session = URLSession(configuration: config)
    }

    // MARK: - Token

    private func loadToken() throws -> String {
        // dirs::config_dir() in Rust → ~/Library/Application Support on macOS
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let tokenPath = appSupport
            .appendingPathComponent("skill")
            .appendingPathComponent("daemon")
            .appendingPathComponent("auth.token")
        let raw = try String(contentsOf: tokenPath, encoding: .utf8)
        let token = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !token.isEmpty else { throw DaemonError.emptyToken }
        return token
    }

    // MARK: - Requests

    private func get<T: Decodable>(_ path: String) async throws -> T {
        let token = try loadToken()
        var request = URLRequest(url: baseURL.appendingPathComponent(path))
        request.httpMethod = "GET"
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            throw DaemonError.badStatus
        }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(T.self, from: data)
    }

    private func post<T: Decodable>(_ path: String, body: [String: Any] = [:]) async throws -> T {
        let token = try loadToken()
        var request = URLRequest(url: baseURL.appendingPathComponent(path))
        request.httpMethod = "POST"
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        if !body.isEmpty {
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
        } else {
            request.httpBody = Data("{}".utf8)
        }
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            throw DaemonError.badStatus
        }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(T.self, from: data)
    }

    // MARK: - Public API

    func fetchFlowState(windowSecs: Int = 300) async throws -> FlowStateResult {
        try await post("/v1/brain/flow-state", body: ["window_secs": windowSecs])
    }

    func fetchFatigue() async throws -> FatigueAlert {
        try await get("/v1/brain/fatigue")
    }

    func fetchStreak(minDeepWorkMins: Int = 60) async throws -> DeepWorkStreak {
        try await post("/v1/brain/streak", body: ["min_deep_work_mins": minDeepWorkMins])
    }

    func fetchStatus() async throws -> DaemonStatus {
        try await get("/v1/api/status")
    }

    func fetchOptimalHours(days: Int = 14) async throws -> OptimalHoursResult {
        try await post("/v1/brain/optimal-hours", body: ["days": days])
    }

    func fetchBreakTiming() async throws -> BreakTimingResult {
        try await post("/v1/brain/break-timing")
    }

    func fetchDailyReport() async throws -> DailyBrainReport {
        let cal = Calendar.current
        let dayStart = Int(cal.startOfDay(for: Date()).timeIntervalSince1970)
        return try await post("/v1/brain/daily-report", body: ["dayStart": dayStart])
    }

    func fetchCognitiveLoad() async throws -> [CognitiveLoadRow] {
        try await post("/v1/brain/cognitive-load")
    }

    func fetchRecentMetrics(windowSecs: Int = 300) async throws -> SessionMetrics {
        let now = Int(Date().timeIntervalSince1970)
        return try await post("/v1/analysis/metrics", body: [
            "start_utc": now - windowSecs,
            "end_utc": now
        ])
    }

    func fetchMeetingRecovery(sinceSecs: Int = 86400) async throws -> MeetingRecoveryResult {
        let since = Int(Date().timeIntervalSince1970) - sinceSecs
        return try await post("/v1/brain/meeting-recovery", body: ["since_utc": since])
    }

    func fetchSleep() async throws -> SleepStages {
        let cal = Calendar.current
        var sixAM = cal.startOfDay(for: Date())
        sixAM = cal.date(bySettingHour: 6, minute: 0, second: 0, of: sixAM) ?? sixAM
        let sixAMUnix = Int(sixAM.timeIntervalSince1970)
        return try await post("/v1/analysis/sleep", body: [
            "start_utc": sixAMUnix - 43200, // 6pm yesterday
            "end_utc": sixAMUnix             // 6am today
        ])
    }

    func fetchCalendarEvents() async throws -> [CalendarEvent] {
        let cal = Calendar.current
        let startOfDay = Int(cal.startOfDay(for: Date()).timeIntervalSince1970)
        return try await post("/v1/api/calendar-events", body: [
            "start_utc": startOfDay,
            "end_utc": startOfDay + 86400
        ])
    }

    /// Fetch all data needed for widget display in one batch.
    /// On success, caches the result. On failure, returns cached data if available.
    func fetchSnapshot() async -> WidgetSnapshot {
        // Check daemon connectivity first via status
        guard let status = try? await fetchStatus() else {
            // Daemon unreachable — try cached data
            if let (cached, _) = SnapshotCache.shared.load() {
                return cached
            }
            let error = detectError()
            return .offline(error: error)
        }

        async let flow = fetchFlowState()
        async let fatigue = fetchFatigue()
        async let streak = fetchStreak()

        let snapshot = WidgetSnapshot(
            flow: try? await flow,
            fatigue: try? await fatigue,
            streak: try? await streak,
            status: status,
            daemonOnline: true
        )

        // Cache for offline fallback
        SnapshotCache.shared.save(snapshot)

        return snapshot
    }

    /// Detect the specific reason the daemon is unreachable.
    func detectError() -> WidgetError {
        do {
            _ = try loadToken()
            return .daemonOffline
        } catch DaemonError.emptyToken {
            return .tokenMissing
        } catch {
            return .tokenMissing
        }
    }
}

enum DaemonError: Error {
    case emptyToken
    case badStatus
}
