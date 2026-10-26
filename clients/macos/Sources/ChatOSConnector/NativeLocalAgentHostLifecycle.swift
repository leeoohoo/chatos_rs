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
    static let version = 47
}

public struct NativeLocalAgentHostResourceSnapshot: Sendable, Equatable {
    public let maximumConcurrentWorkItems: Int
    public let activeWorkItems: Int
    public let idleWorkers: Int
    public let queuedReadyWorkItems: Int
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

/// The machine-level logical-job budget split between the Rust Host reservation and the App-owned
/// pool. Swift Agent Runs consume the App share directly; Media and Code Read workers borrow its
/// otherwise idle weighted units through `ClientMachineResourceGovernor`.
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
    public static let reservedApplicationMemoryBytes: UInt64 = 2 * 1_024 * 1_024 * 1_024
    public static let memoryBytesPerLogicalJob: UInt64 = 2 * 1_024 * 1_024 * 1_024

    public static var adaptiveDefault: NativeClientResourceBudget {
        recommended(
            logicalProcessorCount: ProcessInfo.processInfo.activeProcessorCount,
            physicalMemoryBytes: ProcessInfo.processInfo.physicalMemory
        )
    }

    public static func recommended(
        logicalProcessorCount: Int,
        physicalMemoryBytes: UInt64 = ProcessInfo.processInfo.physicalMemory,
        configuredTotalLogicalJobs: Int? = nil,
        configuredHostWorkItems: Int? = nil
    ) -> NativeClientResourceBudget {
        // Reserve two logical cores for UI/OS work and 2 GiB for the App and shared services.
        // Each remaining logical job receives a conservative 2 GiB memory share, which is above
        // the Agent Worker hard RSS limit. Both bounds must permit a job before the default grows.
        let processorCapacity = max(
            minimumTotalLogicalJobs,
            logicalProcessorCount - 2
        )
        let memoryAvailable = physicalMemoryBytes > reservedApplicationMemoryBytes
            ? physicalMemoryBytes - reservedApplicationMemoryBytes
            : 0
        let memoryCapacity = max(
            minimumTotalLogicalJobs,
            Int(min(
                UInt64(maximumTotalLogicalJobs),
                memoryAvailable / memoryBytesPerLogicalJob
            ))
        )
        let adaptiveTotal = max(
            minimumTotalLogicalJobs,
            min(maximumTotalLogicalJobs, min(processorCapacity, memoryCapacity))
        )
        let total = configuredTotalLogicalJobs.flatMap(validatedTotal) ?? adaptiveTotal
        // Keep one slot for the Swift Agent runtime. A legacy Host override remains useful,
        // but it no longer permits the two runtimes to multiply their independent defaults.
        // Give Host roughly one quarter of the budget; Swift keeps the remaining independent Run
        // lanes so long model/network waits do not strand otherwise idle process capacity.
        let recommendedHost = max(1, total / 4)
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
        // Startup includes integrity checks and the transactionally consistent pre-migration
        // snapshot. A multi-gigabyte database can legitimately spend minutes in this phase;
        // killing it after the ordinary request timeout leaves an incomplete recovery file.
        startupTimeout: Duration = .seconds(10 * 60),
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
    private struct LaunchIntent: Sendable, Equatable {
        let ownerUserID: String
        let credentialFingerprint: Data
        let workersEnabled: Bool

        func satisfies(_ requested: LaunchIntent) -> Bool {
            guard ownerUserID == requested.ownerUserID else { return false }
            // A credentialled worker can serve every bootstrap request for the same account.
            guard requested.workersEnabled else { return true }
            return workersEnabled
                && credentialFingerprint == requested.credentialFingerprint
        }
    }

    private struct HostTransition: Sendable {
        let id: UUID
        let target: LaunchIntent?
        let task: Task<ManagedLocalAgentHostProcess?, Error>
    }

    private let configuration: NativeLocalAgentHostConfiguration
    private var managedProcess: ManagedLocalAgentHostProcess?
    private var transition: HostTransition?
    private var credentialFingerprint: Data?
    private var workersEnabled = false
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

    /// Returns a point-in-time Host scheduler view without launching or recovering the Host.
    /// Observation failures remain unknown to callers and never take down healthy task traffic.
    public func resourceSnapshot() async -> NativeLocalAgentHostResourceSnapshot? {
        guard let process = managedProcess, process.process.isRunning else { return nil }
        let identity = process.identity
        let snapshot = try? await process.resourceSnapshot(timeoutMilliseconds: 1_000)
        guard managedProcess?.identity == identity,
              managedProcess?.process.isRunning == true else { return nil }
        return snapshot
    }

    public func start(ownerUserID: String) async throws {
        try Self.validate(ownerUserID: ownerUserID)
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
        let intent = LaunchIntent(
            ownerUserID: ownerUserID,
            credentialFingerprint: Self.fingerprint(credentialEnvironment),
            workersEnabled: workersEnabled
        )

        while true {
            if runningProcessSatisfies(intent) { return }

            if let transition {
                if transition.target?.satisfies(intent) == true {
                    try await settle(transition)
                    continue
                }
                let successor = chainedLaunch(
                    after: transition,
                    intent: intent,
                    credentialEnvironment: credentialEnvironment
                )
                self.transition = successor
                try await settle(successor)
                continue
            }

            let predecessor = detachManagedProcess()
            let configuration = configuration
            let operation = HostTransition(
                id: UUID(),
                target: intent,
                task: Task.detached(priority: .userInitiated) {
                    predecessor?.terminateAndWait()
                    return try ManagedLocalAgentHostProcess.launch(
                        configuration: configuration,
                        ownerUserID: intent.ownerUserID,
                        credentialEnvironment: credentialEnvironment,
                        workersEnabled: intent.workersEnabled
                    )
                }
            )
            transition = operation
            try await settle(operation)
        }
    }

    public func stop() async {
        let predecessor = transition
        let managed = detachManagedProcess()
        let operation = HostTransition(
            id: UUID(),
            target: nil,
            task: Task.detached(priority: .userInitiated) {
                let launched: ManagedLocalAgentHostProcess?
                if let predecessor {
                    do {
                        launched = try await predecessor.task.value
                    } catch {
                        launched = nil
                    }
                } else {
                    launched = nil
                }
                launched?.terminateAndWait()
                managed?.terminateAndWait()
                return nil
            }
        )
        transition = operation
        try? await settle(operation)
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

    private func runningProcessSatisfies(_ intent: LaunchIntent) -> Bool {
        guard managedProcess?.process.isRunning == true,
              activeOwnerUserID == intent.ownerUserID else { return false }
        guard intent.workersEnabled else { return true }
        return workersEnabled && credentialFingerprint == intent.credentialFingerprint
    }

    private func chainedLaunch(
        after predecessor: HostTransition,
        intent: LaunchIntent,
        credentialEnvironment: [String: String]
    ) -> HostTransition {
        let configuration = configuration
        return HostTransition(
            id: UUID(),
            target: intent,
            task: Task.detached(priority: .userInitiated) {
                let previous: ManagedLocalAgentHostProcess?
                do {
                    previous = try await predecessor.task.value
                } catch {
                    previous = nil
                }
                previous?.terminateAndWait()
                return try ManagedLocalAgentHostProcess.launch(
                    configuration: configuration,
                    ownerUserID: intent.ownerUserID,
                    credentialEnvironment: credentialEnvironment,
                    workersEnabled: intent.workersEnabled
                )
            }
        )
    }

    private func settle(_ observed: HostTransition) async throws {
        let result = await observed.task.result
        guard transition?.id == observed.id else { return }
        transition = nil
        switch result {
        case let .success(managed):
            guard let managed else { return }
            managedProcess = managed
            guard let intent = observed.target else {
                managed.terminateAndWait()
                managedProcess = nil
                return
            }
            activeOwnerUserID = intent.ownerUserID
            workersEnabled = intent.workersEnabled
            credentialFingerprint = intent.credentialFingerprint
            managed.installTerminationHandler { [weak self] identity, processIdentifier in
                Task {
                    await self?.processDidTerminate(
                        identity: identity,
                        processIdentifier: processIdentifier
                    )
                }
            }
        case let .failure(error):
            _ = detachManagedProcess()
            throw error
        }
    }

    @discardableResult
    private func detachManagedProcess() -> ManagedLocalAgentHostProcess? {
        let managed = managedProcess
        credentialFingerprint = nil
        workersEnabled = false
        activeOwnerUserID = nil
        managedProcess = nil
        return managed
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
