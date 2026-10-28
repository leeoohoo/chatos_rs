import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSMediaWorkerClient
import Foundation

struct PerformanceAcceptanceLoadConfiguration: Equatable, Sendable {
    let ownerUserID: String
    let runtimeRoot: URL
    let ledgerURL: URL
    let intervalMilliseconds: Int
    let agentModelDelayMilliseconds: Int

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
        let agentModelDelay = environment["CHATOS_PERFORMANCE_ACCEPTANCE_AGENT_MODEL_DELAY_MS"]
            .flatMap(Int.init) ?? 750
        guard (100...60_000).contains(interval),
              (0...10_000).contains(agentModelDelay) else { return nil }
        return .init(
            ownerUserID: authentication.ownerUserID,
            runtimeRoot: homeURL
                .appendingPathComponent("Library/Caches", isDirectory: true)
                .appendingPathComponent("ChatOSPerformanceAcceptance", isDirectory: true),
            ledgerURL: ledgerURL,
            intervalMilliseconds: interval,
            agentModelDelayMilliseconds: agentModelDelay
        )
    }
}

final class PerformanceAcceptanceLoadDriver {
    private let plugin: NativePluginPerformanceAcceptanceProbe
    private let task: Task<Void, Never>

    init(configuration: PerformanceAcceptanceLoadConfiguration) {
        let plugin = NativePluginPerformanceAcceptanceProbe(
            rootURL: configuration.runtimeRoot.appendingPathComponent(
                "PluginApplication",
                isDirectory: true
            )
        )
        self.plugin = plugin
        task = Task.detached(priority: .utility) {
            await Self.run(configuration: configuration, plugin: plugin)
        }
    }

    deinit {
        plugin.terminateAllSynchronously()
        task.cancel()
    }

    func stop() {
        plugin.terminateAllSynchronously()
        task.cancel()
    }

    private static func run(
        configuration: PerformanceAcceptanceLoadConfiguration,
        plugin: NativePluginPerformanceAcceptanceProbe
    ) async {
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

        do {
            try await plugin.start(ownerUserID: configuration.ownerUserID)
        } catch {
            try? await ledger.recordFailure(kind: .plugin, error: error)
        }
        await withTaskGroup(of: Void.self) { group in
            group.addTask { await runAgentLoop(configuration: configuration, ledger: ledger) }
            group.addTask { await runMediaLoop(configuration: configuration, ledger: ledger) }
            group.addTask {
                await runPluginLoop(
                    configuration: configuration,
                    ledger: ledger,
                    plugin: plugin
                )
            }
            await group.waitForAll()
        }
        await plugin.stop()
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
                    model: PerformanceAcceptanceModel(
                        delayMilliseconds: configuration.agentModelDelayMilliseconds
                    ),
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
            "media-source.bmp",
            isDirectory: false
        )
        do {
            let source = try makeBitmapFixture(pixelSize: 2_048)
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
                    maximumSourceBytes: 20 * 1_024 * 1_024,
                    maximumSourcePixelCount: 4 * 1_024 * 1_024,
                    maximumOutputPixelSize: 2_048,
                    maximumOutputBytes: 20 * 1_024 * 1_024
                )
                guard result.path == destinationURL.path,
                      result.pixelWidth == 2_048,
                      result.pixelHeight == 2_048 else {
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

    private static func runPluginLoop(
        configuration: PerformanceAcceptanceLoadConfiguration,
        ledger: PerformanceAcceptanceLedger,
        plugin: NativePluginPerformanceAcceptanceProbe
    ) async {
        while !Task.isCancelled {
            do {
                try await plugin.exercise()
                try await ledger.recordSuccess(kind: .plugin)
            } catch is CancellationError {
                break
            } catch {
                try? await ledger.recordFailure(kind: .plugin, error: error)
            }
            guard await sleep(configuration.intervalMilliseconds) else { break }
        }
    }

    private static func makeBitmapFixture(pixelSize: Int) throws -> Data {
        guard pixelSize > 0 else {
            throw PerformanceAcceptanceLoadError.invalidMediaFixture
        }
        let bytesPerRow = pixelSize * 4
        let pixelBytes = bytesPerRow * pixelSize
        let fileBytes = 54 + pixelBytes
        var data = Data(capacity: fileBytes)
        func appendUInt16(_ value: UInt16) {
            var littleEndian = value.littleEndian
            withUnsafeBytes(of: &littleEndian) { data.append(contentsOf: $0) }
        }
        func appendUInt32(_ value: UInt32) {
            var littleEndian = value.littleEndian
            withUnsafeBytes(of: &littleEndian) { data.append(contentsOf: $0) }
        }
        data.append(contentsOf: [0x42, 0x4D])
        appendUInt32(UInt32(fileBytes))
        appendUInt32(0)
        appendUInt32(54)
        appendUInt32(40)
        appendUInt32(UInt32(pixelSize))
        appendUInt32(UInt32(pixelSize))
        appendUInt16(1)
        appendUInt16(32)
        appendUInt32(0)
        appendUInt32(UInt32(pixelBytes))
        appendUInt32(2_835)
        appendUInt32(2_835)
        appendUInt32(0)
        appendUInt32(0)
        data.append(Data(repeating: 0x7F, count: pixelBytes))
        guard data.count == fileBytes else {
            throw PerformanceAcceptanceLoadError.invalidMediaFixture
        }
        return data
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
    let delayMilliseconds: Int

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        guard tools.contains(where: { $0.name == "finish" }) else {
            throw PerformanceAcceptanceLoadError.unexpectedTool
        }
        if delayMilliseconds > 0 {
            try await Task.sleep(for: .milliseconds(delayMilliseconds))
        }
        return .init(role: .assistant, toolCalls: [
            .init(id: "finish-call", name: "finish", arguments: "{}"),
        ])
    }
}

private enum PerformanceAcceptanceWorkKind: String, Codable, Sendable {
    case agent
    case media
    case plugin
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
        var pluginCompletions = 0
        var pluginFailures = 0
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
        case .plugin: snapshot.pluginCompletions += 1
        }
        try persist()
    }

    func recordFailure(kind: PerformanceAcceptanceWorkKind, error: Error) throws {
        switch kind {
        case .agent: snapshot.agentFailures += 1
        case .media: snapshot.mediaFailures += 1
        case .plugin: snapshot.pluginFailures += 1
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
