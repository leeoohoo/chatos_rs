import ChatOSProcessRuntime
import Darwin
import Foundation
import OSLog

/// Thread-safe process ownership ledger used by the actor runtime and the
/// synchronous NSApplication termination callback.
final class NativePluginApplicationProcessRegistry: @unchecked Sendable {
    struct Record: Codable, Equatable, Sendable {
        var pid: Int32
        var startSeconds: UInt64
        var startMicroseconds: UInt64
    }

    private let stateURL: URL
    private let pluginInstallationRootURL: URL
    private let lock = NSLock()
    private var records: [Int32: Record] = [:]
    private static let logger = Logger(
        subsystem: "com.chatos.swift-client",
        category: "PluginApplicationProcesses"
    )

    init(stateURL: URL, pluginInstallationRootURL: URL) {
        self.stateURL = stateURL
        self.pluginInstallationRootURL = pluginInstallationRootURL.standardizedFileURL
        records = Self.loadRecords(from: stateURL)
        terminateRecordedProcessesSynchronously(graceMilliseconds: 500)
        terminateLegacyOrphansSynchronously()
    }

    func register(pid: pid_t) throws {
        var seconds: UInt64 = 0
        var microseconds: UInt64 = 0
        let result = chatos_process_start_time(pid, &seconds, &microseconds)
        guard result == 0 else {
            throw POSIXError(POSIXErrorCode(rawValue: result) ?? .ESRCH)
        }
        lock.lock()
        records[pid] = Record(
            pid: pid,
            startSeconds: seconds,
            startMicroseconds: microseconds
        )
        persistLocked()
        lock.unlock()
    }

    func unregister(pid: pid_t) {
        lock.lock()
        if records.removeValue(forKey: pid) != nil {
            persistLocked()
        }
        lock.unlock()
    }

    func terminateAllSynchronously(graceMilliseconds: Int = 2_000) {
        terminateRecordedProcessesSynchronously(graceMilliseconds: graceMilliseconds)
    }

    private func terminateRecordedProcessesSynchronously(graceMilliseconds: Int) {
        let snapshot = recordSnapshot()
        guard !snapshot.isEmpty else { return }

        for record in snapshot where Self.matches(record) {
            _ = chatos_signal_process_group(record.pid, SIGTERM)
        }

        let deadline = Date().addingTimeInterval(Double(max(0, graceMilliseconds)) / 1_000)
        var remaining = snapshot.filter(Self.matches)
        while !remaining.isEmpty, Date() < deadline {
            usleep(50_000)
            remaining = remaining.filter(Self.matches)
        }
        for record in remaining {
            _ = chatos_signal_process_group(record.pid, SIGKILL)
        }
        if !remaining.isEmpty {
            usleep(50_000)
        }

        lock.lock()
        for record in snapshot where !Self.matches(record) {
            records.removeValue(forKey: record.pid)
        }
        persistLocked()
        lock.unlock()
    }

    /// Releases pre-fix Plugin Application servers that have already been
    /// adopted by launchd. The exact installation root plus PPID 1 prevents
    /// unrelated Node processes from being selected.
    private func terminateLegacyOrphansSynchronously() {
        let ps = Process()
        ps.executableURL = URL(fileURLWithPath: "/bin/ps")
        ps.arguments = ["-axo", "pid=,ppid=,pgid=,command="]
        let output = Pipe()
        ps.standardOutput = output
        ps.standardError = FileHandle.nullDevice
        do {
            try ps.run()
            let data = output.fileHandleForReading.readDataToEndOfFile()
            ps.waitUntilExit()
            guard ps.terminationStatus == 0 else { return }
            let prefix = pluginInstallationRootURL.path + "/"
            let candidates = String(decoding: data, as: UTF8.self)
                .split(separator: "\n")
                .compactMap { line -> pid_t? in
                    let parts = line.split(
                        maxSplits: 3,
                        omittingEmptySubsequences: true,
                        whereSeparator: \Character.isWhitespace
                    )
                    guard parts.count == 4,
                          let pid = pid_t(parts[0]),
                          let parent = pid_t(parts[1]),
                          let group = pid_t(parts[2]),
                          parent == 1,
                          pid > 1,
                          group == pid,
                          parts[3].contains(prefix) else { return nil }
                    return group
                }
            guard !candidates.isEmpty else { return }
            for group in candidates {
                _ = chatos_signal_process_group(group, SIGTERM)
            }
            usleep(250_000)
            for group in candidates where kill(-group, 0) == 0 {
                _ = chatos_signal_process_group(group, SIGKILL)
            }
            Self.logger.notice("Cleaned \(candidates.count, privacy: .public) legacy plugin application process groups")
        } catch {
            Self.logger.error("Legacy plugin process cleanup failed: \(error.localizedDescription, privacy: .public)")
        }
    }

    private func recordSnapshot() -> [Record] {
        lock.lock()
        defer { lock.unlock() }
        return Array(records.values)
    }

    private static func matches(_ record: Record) -> Bool {
        chatos_process_matches_start_time(
            record.pid,
            record.startSeconds,
            record.startMicroseconds
        ) == 1
    }

    private static func loadRecords(from url: URL) -> [Int32: Record] {
        guard let data = try? Data(contentsOf: url),
              let decoded = try? JSONDecoder().decode([Record].self, from: data) else {
            return [:]
        }
        return Dictionary(uniqueKeysWithValues: decoded.map { ($0.pid, $0) })
    }

    private func persistLocked() {
        do {
            try FileManager.default.createDirectory(
                at: stateURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            let data = try JSONEncoder().encode(
                records.values.sorted { $0.pid < $1.pid }
            )
            try data.write(to: stateURL, options: .atomic)
        } catch {
            Self.logger.error("Plugin process ledger write failed: \(error.localizedDescription, privacy: .public)")
        }
    }
}
