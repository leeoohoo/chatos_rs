import ChatOSCore
import CryptoKit
import Darwin
import Foundation

public extension Notification.Name {
    static let nativeLocalAgentHostDidExit = Notification.Name(
        "com.chatos.swift.local-agent-host-did-exit"
    )
    static let nativeLocalAgentHostRecoveryRequested = Notification.Name(
        "com.chatos.swift.local-agent-host-recovery-requested"
    )
}

enum NativeLocalAgentHostProtocol {
    static let version = 45
}

public enum NativeLocalAgentConcurrencyPolicy {
    public static let minimum = 1
    public static let maximum = 16

    public static var adaptiveDefault: Int {
        NativeClientResourceBudgetPolicy.adaptiveDefault.localAgentHostWorkItems
    }

    public static func validated(_ value: Int) -> Int? {
        (minimum...maximum).contains(value) ? value : nil
    }
}

/// A process-wide logical-job budget shared by the two client runtimes that can
/// currently create model/tool work. Media remains outside this first governor
/// slice and must not be reported as governed yet.
public struct NativeClientResourceBudget: Sendable, Equatable {
    public let totalLogicalJobs: Int
    public let localAgentHostWorkItems: Int
    public let swiftAgentRuns: Int

    public init(totalLogicalJobs: Int, localAgentHostWorkItems: Int) {
        precondition(totalLogicalJobs >= 2)
        precondition((1..<totalLogicalJobs).contains(localAgentHostWorkItems))
        self.totalLogicalJobs = totalLogicalJobs
        self.localAgentHostWorkItems = localAgentHostWorkItems
        self.swiftAgentRuns = totalLogicalJobs - localAgentHostWorkItems
    }
}

public enum NativeClientResourceBudgetPolicy {
    public static let minimumTotalLogicalJobs = 2
    public static let maximumTotalLogicalJobs = 16

    public static var adaptiveDefault: NativeClientResourceBudget {
        recommended(logicalProcessorCount: ProcessInfo.processInfo.activeProcessorCount)
    }

    public static func recommended(
        logicalProcessorCount: Int,
        configuredTotalLogicalJobs: Int? = nil,
        configuredHostWorkItems: Int? = nil
    ) -> NativeClientResourceBudget {
        // Start with the architecture plan's conservative process-group limit. Very small
        // machines use two jobs; larger machines stay at three until a fixed Release baseline
        // justifies raising it. Processor count alone is not evidence that memory/I/O scale too.
        let conservativeDefault = logicalProcessorCount <= 2 ? 2 : 3
        let total = configuredTotalLogicalJobs.flatMap(validatedTotal) ?? conservativeDefault
        // Keep one slot for the Swift Agent runtime. A legacy Host override remains useful,
        // but it no longer permits the two runtimes to multiply their independent defaults.
        // The default also preserves two Swift lanes where possible so a manager reply is not
        // blocked behind one long executor Run.
        let recommendedHost = max(1, total - 2)
        let requestedHost = configuredHostWorkItems
            .flatMap(NativeLocalAgentConcurrencyPolicy.validated) ?? recommendedHost
        return .init(
            totalLogicalJobs: total,
            localAgentHostWorkItems: min(total - 1, requestedHost)
        )
    }

    public static func validatedTotal(_ value: Int) -> Int? {
        (minimumTotalLogicalJobs...maximumTotalLogicalJobs).contains(value) ? value : nil
    }
}

public struct NativeLocalAgentHostConfiguration: Sendable, Equatable {
    public let executableURL: URL
    public let databaseURL: URL
    public let startupTimeout: Duration
    public let requestTimeoutMilliseconds: Int
    public let readOnlyToolNames: [String]
    public let approvalExemptToolNames: [String]
    public let memoryBaseURL: URL?
    public let memorySourceID: String?
    public let memoryTimeoutMilliseconds: Int
    public let maximumConcurrentWorkItems: Int

    public init(
        executableURL: URL,
        databaseURL: URL,
        startupTimeout: Duration = .seconds(10),
        requestTimeoutMilliseconds: Int = 75_000,
        readOnlyToolNames: [String] = NativeLocalAgentPlatformToolCatalog.readOnlyToolNames,
        approvalExemptToolNames: [String] =
            NativeLocalAgentPlatformToolCatalog.approvalExemptToolNames,
        memoryBaseURL: URL? = nil,
        memorySourceID: String? = nil,
        memoryTimeoutMilliseconds: Int = 30_000,
        maximumConcurrentWorkItems: Int = NativeLocalAgentConcurrencyPolicy.adaptiveDefault
    ) {
        self.executableURL = executableURL
        self.databaseURL = databaseURL
        self.startupTimeout = startupTimeout
        self.requestTimeoutMilliseconds = requestTimeoutMilliseconds
        self.readOnlyToolNames = readOnlyToolNames
        self.approvalExemptToolNames = approvalExemptToolNames
        self.memoryBaseURL = memoryBaseURL
        self.memorySourceID = memorySourceID
        self.memoryTimeoutMilliseconds = memoryTimeoutMilliseconds
        self.maximumConcurrentWorkItems = maximumConcurrentWorkItems
    }
}

public actor NativeLocalAgentHostLifecycle: LocalAgentHostClientServicing {
    private let configuration: NativeLocalAgentHostConfiguration
    private var managedProcess: ManagedLocalAgentHostProcess?
    private var credentialFingerprint: Data?
    private var workersEnabled = false
    private var launchGeneration: UInt64 = 0
    public private(set) var activeOwnerUserID: String?

    public init(configuration: NativeLocalAgentHostConfiguration) {
        self.configuration = configuration
    }

    public var isRunning: Bool {
        managedProcess?.process.isRunning == true
    }

    public var processIdentifier: Int32? {
        guard managedProcess?.process.isRunning == true else { return nil }
        return managedProcess?.process.processIdentifier
    }

    public func start(ownerUserID: String) async throws {
        try Self.validate(ownerUserID: ownerUserID)
        if managedProcess?.process.isRunning == true, activeOwnerUserID == ownerUserID {
            return
        }
        try await launch(
            ownerUserID: ownerUserID,
            credentialEnvironment: [:],
            workersEnabled: true
        )
    }

    /// Starts an IPC-only Host so persisted control-plane state can be read before model
    /// credentials are restored. The bootstrap process must not claim durable work because
    /// `restart` replaces it as soon as Keychain-backed credentials have been loaded.
    public func startForBootstrap(ownerUserID: String) async throws {
        try Self.validate(ownerUserID: ownerUserID)
        try await launch(
            ownerUserID: ownerUserID,
            credentialEnvironment: [:],
            workersEnabled: false
        )
    }

    /// Restarts the Host with model credentials visible only to the child
    /// process. Callers must source these values from Keychain and discard the
    /// dictionary after this method returns.
    public func restart(
        ownerUserID: String,
        credentialEnvironment: [String: String]
    ) async throws {
        try Self.validate(ownerUserID: ownerUserID)
        try Self.validate(credentialEnvironment: credentialEnvironment)
        try await launch(
            ownerUserID: ownerUserID,
            credentialEnvironment: credentialEnvironment,
            workersEnabled: true
        )
    }

    /// A catalog refresh must not terminate durable work restored at startup. Identical
    /// credentials reuse the worker. Changed credentials take effect on the next normal
    /// startup, rather than guessing whether a concurrently claiming worker is idle.
    public func refreshCredentials(
        ownerUserID: String,
        credentialEnvironment: [String: String]
    ) async throws {
        try Self.validate(ownerUserID: ownerUserID)
        try Self.validate(credentialEnvironment: credentialEnvironment)
        if managedProcess?.process.isRunning == true, workersEnabled {
            guard activeOwnerUserID == ownerUserID else {
                throw NativeLocalAgentHostError.ownerMismatch
            }
            guard credentialFingerprint == Self.fingerprint(credentialEnvironment) else {
                throw NativeLocalAgentCredentialRefreshDeferred()
            }
            return
        }
        try await launch(
            ownerUserID: ownerUserID,
            credentialEnvironment: credentialEnvironment,
            workersEnabled: true
        )
    }

    private static func fingerprint(_ environment: [String: String]) -> Data {
        // Length prefixes prevent ambiguous key/value concatenation. Only a digest is
        // retained by the lifecycle; credentials are neither persisted nor logged here.
        var hash = SHA256()
        for name in environment.keys.sorted() {
            for value in [name, environment[name]!] {
                var length = UInt64(value.utf8.count).bigEndian
                withUnsafeBytes(of: &length) { hash.update(data: Data($0)) }
                hash.update(data: Data(value.utf8))
            }
        }
        return Data(hash.finalize())
    }

    private func launch(
        ownerUserID: String,
        credentialEnvironment: [String: String],
        workersEnabled: Bool
    ) async throws {
        stopLocked()
        let generation = launchGeneration
        do {
            let configuration = configuration
            let managed = try await Task.detached(priority: .userInitiated) {
                try ManagedLocalAgentHostProcess.launch(
                    configuration: configuration,
                    ownerUserID: ownerUserID,
                    credentialEnvironment: credentialEnvironment,
                    workersEnabled: workersEnabled
                )
            }.value
            guard generation == launchGeneration, !Task.isCancelled else {
                managed.terminate()
                throw CancellationError()
            }
            managedProcess = managed
            activeOwnerUserID = ownerUserID
            self.workersEnabled = workersEnabled
            credentialFingerprint = Self.fingerprint(credentialEnvironment)
            managed.installTerminationHandler { [weak self] identity, processIdentifier in
                Task {
                    await self?.processDidTerminate(
                        identity: identity,
                        processIdentifier: processIdentifier
                    )
                }
            }
        } catch {
            if generation == launchGeneration { stopLocked() }
            throw error
        }
    }

    public func stop() async {
        stopLocked()
    }

    public func request(command: Data) async throws -> Data {
        let commandValue = try JSONSerialization.jsonObject(with: command)
        guard let commandObject = commandValue as? [String: Any],
              commandObject["type"] is String else {
            throw NativeLocalAgentHostError.invalidCommand
        }
        let commandOwner = commandObject["owner_user_id"] as? String
        let commandID = "native-command-\(UUID().uuidString.lowercased())"
        let envelope: [String: Any] = [
            "protocol_version": NativeLocalAgentHostProtocol.version,
            "command_id": commandID,
            "command": commandObject,
        ]
        let request = try JSONSerialization.data(withJSONObject: envelope)
        let process = try await waitForRunningProcess(commandOwner: commandOwner)
        let responseData: Data
        do {
            responseData = try await process.roundTripAsync(request)
        } catch is CancellationError {
            throw CancellationError()
        } catch let error as NativeLocalAgentHostError where error == .requestTimedOut {
            // One long-running command timing out does not mean the multiplexed Host transport
            // is broken. In particular, an event long poll can outlive its client deadline while
            // the Host remains able to serve task-detail requests. Probe the same process before
            // invalidating it so an isolated timeout cannot take every concurrent request down.
            if await process.respondsToHealthCheck(timeoutMilliseconds: 1_000) {
                throw error
            }
            invalidateAfterTransportFailure(process)
            throw error
        } catch let error as NativeLocalAgentHostError where error == .notRunning {
            invalidateAfterTransportFailure(process)
            let recovered = try await waitForRunningProcess(commandOwner: commandOwner)
            do {
                responseData = try await recovered.roundTripAsync(request)
            } catch {
                invalidateAfterTransportFailure(recovered)
                throw error
            }
        } catch {
            invalidateAfterTransportFailure(process)
            throw error
        }
        guard let response = try JSONSerialization.jsonObject(with: responseData)
            as? [String: Any],
              response["protocol_version"] as? Int == NativeLocalAgentHostProtocol.version,
              response["command_id"] as? String == commandID,
              let ok = response["ok"] as? Bool else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        guard ok else {
            let error = response["error"] as? [String: Any]
            throw NativeLocalAgentHostError.hostError(
                code: error?["code"] as? String ?? "host_error",
                message: error?["message"] as? String ?? "Local Agent Host request failed.",
                retryable: error?["retryable"] as? Bool ?? false
            )
        }
        guard let result = response["result"],
              JSONSerialization.isValidJSONObject(result) else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return try JSONSerialization.data(withJSONObject: result)
    }

    private func waitForRunningProcess(
        commandOwner: String?
    ) async throws -> ManagedLocalAgentHostProcess {
        if let process = try runningProcess(commandOwner: commandOwner) {
            return process
        }
        NotificationCenter.default.post(
            name: .nativeLocalAgentHostRecoveryRequested,
            object: nil
        )
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: configuration.startupTimeout)
        while clock.now < deadline {
            try Task.checkCancellation()
            try await Task.sleep(for: .milliseconds(50))
            if let process = try runningProcess(commandOwner: commandOwner) {
                return process
            }
        }
        throw NativeLocalAgentHostError.notRunning
    }

    private func runningProcess(
        commandOwner: String?
    ) throws -> ManagedLocalAgentHostProcess? {
        guard let managedProcess, managedProcess.process.isRunning else { return nil }
        guard commandOwner == nil || commandOwner == activeOwnerUserID else {
            throw NativeLocalAgentHostError.ownerMismatch
        }
        return managedProcess
    }

    private func stopLocked() {
        launchGeneration &+= 1
        credentialFingerprint = nil
        workersEnabled = false
        activeOwnerUserID = nil
        managedProcess?.terminate()
        managedProcess = nil
    }

    private func processDidTerminate(identity: UUID, processIdentifier: Int32) {
        guard let managedProcess, managedProcess.identity == identity else { return }
        activeOwnerUserID = nil
        credentialFingerprint = nil
        workersEnabled = false
        self.managedProcess = nil
        managedProcess.closeAfterExit()
        NotificationCenter.default.post(
            name: .nativeLocalAgentHostDidExit,
            object: nil,
            userInfo: ["process_identifier": processIdentifier]
        )
    }

    private func invalidateAfterTransportFailure(_ managedProcess: ManagedLocalAgentHostProcess) {
        guard self.managedProcess?.identity == managedProcess.identity else { return }
        let processIdentifier = managedProcess.process.processIdentifier
        activeOwnerUserID = nil
        credentialFingerprint = nil
        workersEnabled = false
        self.managedProcess = nil
        managedProcess.terminate()
        NotificationCenter.default.post(
            name: .nativeLocalAgentHostDidExit,
            object: nil,
            userInfo: ["process_identifier": processIdentifier]
        )
    }

    private static func validate(ownerUserID: String) throws {
        guard !ownerUserID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              ownerUserID.count <= 256,
              !ownerUserID.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
        else {
            throw NativeLocalAgentHostError.invalidOwner
        }
    }

    private static func validate(credentialEnvironment: [String: String]) throws {
        guard credentialEnvironment.count <= 64 else {
            throw NativeLocalAgentHostError.invalidCredentialEnvironment
        }
        for (name, value) in credentialEnvironment {
            guard (name == "CHATOS_MEMORY_ACCESS_TOKEN"
                    || name.hasPrefix("CHATOS_LOCAL_AGENT_MODEL_")),
                  name.count <= 128,
                  name.unicodeScalars.allSatisfy({ scalar in
                      CharacterSet.uppercaseLetters.contains(scalar)
                          || CharacterSet.decimalDigits.contains(scalar)
                          || scalar == "_"
                  }),
                  !value.isEmpty,
                  value.lengthOfBytes(using: .utf8) <= 64 * 1_024,
                  !value.contains("\0") else {
                throw NativeLocalAgentHostError.invalidCredentialEnvironment
            }
        }
    }
}

public struct NativeLocalAgentCredentialRefreshDeferred:
    LocalizedError,
    LocalAgentControlPlaneBootstrapNonRetryableError,
    Sendable
{
    public var errorDescription: String? {
        NSLocalizedString("连接凭据已更新。为避免中断当前执行，请在任务完成后重新打开应用以应用新配置。", comment: "Deferred model credential refresh")
    }
}

enum NativeLocalAgentHostError: LocalizedError, Equatable {
    case invalidConfiguration(String)
    case invalidOwner
    case launchFailed(String)
    case notRunning
    case invalidCommand
    case ownerMismatch
    case invalidCredentialEnvironment
    case invalidFrame
    case messageTooLarge
    case requestTimedOut
    case invalidResponse
    case hostError(code: String, message: String, retryable: Bool)

    var errorDescription: String? {
        switch self {
        case let .invalidConfiguration(message), let .launchFailed(message):
            message
        case let .hostError(_, message, _):
            message
        case .invalidOwner:
            "Local Agent owner must be 1...256 non-control characters."
        case .notRunning:
            "Local Agent Host is not running."
        case .invalidCommand:
            "Local Agent Host command must be a JSON object with a type."
        case .ownerMismatch:
            "Local Agent Host command owner does not match the active account."
        case .invalidCredentialEnvironment:
            "Local Agent Host credential environment is invalid."
        case .invalidFrame:
            "Local Agent Host returned an invalid frame."
        case .messageTooLarge:
            "Local Agent Host message exceeded the 64 MiB logical message limit; use an artifact reference for larger payloads."
        case .requestTimedOut:
            "Local Agent Host did not respond before the request deadline."
        case .invalidResponse:
            "Local Agent Host health response is invalid."
        }
    }
}

public extension LocalAgentHostClientServicing {
    func request<Command: Encodable & Sendable, Result: Decodable & Sendable>(
        _ command: Command,
        as resultType: Result.Type = Result.self
    ) async throws -> Result {
        let data = try JSONEncoder().encode(command)
        let response = try await request(command: data)
        return try JSONDecoder().decode(resultType, from: response)
    }
}

extension JSONEncoder {
    static var localAgent: JSONEncoder {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        return encoder
    }
}

extension JSONDecoder {
    static var localAgent: JSONDecoder {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }
}
