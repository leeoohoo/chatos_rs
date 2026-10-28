import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSMediaWorkerClient
import Foundation

struct PerformanceAcceptanceLoadConfiguration: Equatable, Sendable {
    let ownerUserID: String
    let runtimeRoot: URL
    let ledgerURL: URL
    let intervalMilliseconds: Int

    static func resolve(
        environment: [String: String],
        authentication: BuildSmokeAuthenticationConfiguration
    ) -> Self? {
        guard environment["CHATOS_PERFORMANCE_ACCEPTANCE_LOAD"] == "1",
              let rawLedgerPath = environment["CHATOS_PERFORMANCE_ACCEPTANCE_LEDGER_PATH"] else {
            return nil
        }
        let homeURL = URL(fileURLWithPath: authentication.homeDirectory, isDirectory: true)
            .standardizedFileURL
        let ledgerURL = URL(fileURLWithPath: rawLedgerPath, isDirectory: false)
            .standardizedFileURL
        guard ledgerURL.path.hasPrefix("/"),
              ledgerURL.path != homeURL.path,
              !ledgerURL.path.hasPrefix(homeURL.path + "/") else {
            return nil
        }
        let interval = environment["CHATOS_PERFORMANCE_ACCEPTANCE_INTERVAL_MS"]
            .flatMap(Int.init) ?? 1_000
        guard (100...60_000).contains(interval) else { return nil }
        return .init(
            ownerUserID: authentication.ownerUserID,
            runtimeRoot: homeURL
                .appendingPathComponent("Library/Caches", isDirectory: true)
                .appendingPathComponent("ChatOSPerformanceAcceptance", isDirectory: true),
            ledgerURL: ledgerURL,
            intervalMilliseconds: interval
        )
    }
}

final class PerformanceAcceptanceLoadDriver {
    private let task: Task<Void, Never>

    init(configuration: PerformanceAcceptanceLoadConfiguration) {
        task = Task.detached(priority: .utility) {
            await Self.run(configuration: configuration)
        }
    }

    deinit {
        task.cancel()
    }

    func stop() {
        task.cancel()
    }

    private static func run(configuration: PerformanceAcceptanceLoadConfiguration) async {
        let ledger = PerformanceAcceptanceLedger(url: configuration.ledgerURL)
        do {
            try FileManager.default.createDirectory(
                at: configuration.runtimeRoot,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            try await ledger.start()
        } catch {
            return
        }

        await withTaskGroup(of: Void.self) { group in
            group.addTask { await runAgentLoop(configuration: configuration, ledger: ledger) }
            group.addTask { await runMediaLoop(configuration: configuration, ledger: ledger) }
            await group.waitForAll()
        }
        try? await ledger.stop()
    }

    private static func runAgentLoop(
        configuration: PerformanceAcceptanceLoadConfiguration,
        ledger: PerformanceAcceptanceLedger
    ) async {
        let client = AgentWorkerProcessClient(shutdownGraceSeconds: 5)
        while !Task.isCancelled {
            do {
                let scope = "performance-acceptance-\(UUID().uuidString.lowercased())"
                var checkpoint = AgentRunCheckpoint(
                    scope: scope,
                    messages: [.init(role: .user, content: "Complete the acceptance probe")]
                )
                checkpoint.id = UUID()
                var policy = AgentRunPolicy()
                policy.maximumModelCalls = 2
                policy.maximumRequestRetries = 0
                policy.maximumNoProgressRounds = 2
                policy.requestTimeoutSeconds = 10
                policy.runTimeoutSeconds = 30
                let result = try await client.executeRun(
                    checkpoint: checkpoint,
                    scope: scope,
                    policy: policy,
                    model: PerformanceAcceptanceModel(),
                    tools: [.init(
                        name: "finish",
                        description: "Complete the isolated acceptance probe",
                        schema: Data(
                            #"{"type":"object","properties":{},"additionalProperties":false}"#.utf8
                        ),
                        effect: .terminal
                    )],
                    execute: { call in
                        guard call.name == "finish" else {
                            throw PerformanceAcceptanceLoadError.unexpectedTool
                        }
                        return .init("acceptance-worker-completed")
                    },
                    completionCheck: { nil },
                    contextProvider: nil,
                    record: { _, _ in }
                )
                guard result.status == .completed,
                      result.result == "acceptance-worker-completed" else {
                    throw PerformanceAcceptanceLoadError.invalidAgentResult
                }
                try await ledger.recordSuccess(kind: .agent)
            } catch is CancellationError {
                break
            } catch {
                try? await ledger.recordFailure(kind: .agent, error: error)
            }
            guard await sleep(configuration.intervalMilliseconds) else { break }
        }
    }

    private static func runMediaLoop(
        configuration: PerformanceAcceptanceLoadConfiguration,
        ledger: PerformanceAcceptanceLedger
    ) async {
        let sourceURL = configuration.runtimeRoot.appendingPathComponent(
            "media-source.png",
            isDirectory: false
        )
        do {
            guard let source = Data(base64Encoded:
                "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
            ) else {
                throw PerformanceAcceptanceLoadError.invalidMediaFixture
            }
            try source.write(to: sourceURL, options: [.atomic])
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: sourceURL.path
            )
        } catch {
            try? await ledger.recordFailure(kind: .media, error: error)
            return
        }

        let client = MediaWorkerProcessClient.shared
        while !Task.isCancelled {
            let destinationURL = configuration.runtimeRoot.appendingPathComponent(
                "media-output-\(UUID().uuidString.lowercased()).png",
                isDirectory: false
            )
            do {
                guard FileManager.default.createFile(
                    atPath: destinationURL.path,
                    contents: nil,
                    attributes: [.posixPermissions: 0o600]
                ) else {
                    throw PerformanceAcceptanceLoadError.cannotCreateMediaOutput
                }
                defer { try? FileManager.default.removeItem(at: destinationURL) }
                let result = try await client.normalizePNG(
                    sourceURL: sourceURL,
                    sourceIdentity: nil,
                    destinationURL: destinationURL,
                    ownerUserID: configuration.ownerUserID,
                    maximumSourceBytes: 1 * 1_024 * 1_024,
                    maximumSourcePixelCount: 4 * 1_024 * 1_024,
                    maximumOutputPixelSize: 512,
                    maximumOutputBytes: 1 * 1_024 * 1_024
                )
                guard result.path == destinationURL.path,
                      result.pixelWidth == 1,
                      result.pixelHeight == 1 else {
                    throw PerformanceAcceptanceLoadError.invalidMediaResult
                }
                try await ledger.recordSuccess(kind: .media)
            } catch is CancellationError {
                try? FileManager.default.removeItem(at: destinationURL)
                break
            } catch {
                try? FileManager.default.removeItem(at: destinationURL)
                try? await ledger.recordFailure(kind: .media, error: error)
            }
            guard await sleep(configuration.intervalMilliseconds) else { break }
        }
    }

    private static func sleep(_ milliseconds: Int) async -> Bool {
        do {
            try await Task.sleep(for: .milliseconds(milliseconds))
            return !Task.isCancelled
        } catch {
            return false
        }
    }
}

private struct PerformanceAcceptanceModel: AgentModelClient {
    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        guard tools.contains(where: { $0.name == "finish" }) else {
            throw PerformanceAcceptanceLoadError.unexpectedTool
        }
        return .init(role: .assistant, toolCalls: [
            .init(id: "finish-call", name: "finish", arguments: "{}"),
        ])
    }
}

private enum PerformanceAcceptanceWorkKind: String, Codable, Sendable {
    case agent
    case media
}

private enum PerformanceAcceptanceLoadError: Error {
    case unexpectedTool
    case invalidAgentResult
    case invalidMediaFixture
    case cannotCreateMediaOutput
    case invalidMediaResult
}

private actor PerformanceAcceptanceLedger {
    private struct Snapshot: Codable {
        var formatVersion = 1
        var status = "starting"
        var startedUnixMilliseconds = Int64(Date().timeIntervalSince1970 * 1_000)
        var updatedUnixMilliseconds = Int64(Date().timeIntervalSince1970 * 1_000)
        var agentCompletions = 0
        var mediaCompletions = 0
        var agentFailures = 0
        var mediaFailures = 0
        var recentFailures: [String] = []
    }

    private let url: URL
    private var snapshot = Snapshot()

    init(url: URL) {
        self.url = url
    }

    func start() throws {
        snapshot.status = "running"
        try persist()
    }

    func recordSuccess(kind: PerformanceAcceptanceWorkKind) throws {
        switch kind {
        case .agent: snapshot.agentCompletions += 1
        case .media: snapshot.mediaCompletions += 1
        }
        try persist()
    }

    func recordFailure(kind: PerformanceAcceptanceWorkKind, error: Error) throws {
        switch kind {
        case .agent: snapshot.agentFailures += 1
        case .media: snapshot.mediaFailures += 1
        }
        snapshot.recentFailures.append("\(kind.rawValue): \(String(describing: error))")
        if snapshot.recentFailures.count > 20 {
            snapshot.recentFailures.removeFirst(snapshot.recentFailures.count - 20)
        }
        try persist()
    }

    func stop() throws {
        snapshot.status = "stopped"
        try persist()
    }

    private func persist() throws {
        snapshot.updatedUnixMilliseconds = Int64(Date().timeIntervalSince1970 * 1_000)
        let data = try JSONEncoder().encode(snapshot)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        try data.write(to: url, options: [.atomic])
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600],
            ofItemAtPath: url.path
        )
    }
}
