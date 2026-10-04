import ChatOSCore
import Darwin
import Foundation

public extension Notification.Name {
    static let nativeLocalAgentHostDidExit = Notification.Name(
        "com.chatos.swift.local-agent-host-did-exit"
    )
}

private enum NativeLocalAgentHostProtocol {
    static let version = 39
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
        memoryTimeoutMilliseconds: Int = 30_000
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
    }
}

public actor NativeLocalAgentHostLifecycle: LocalAgentHostClientServicing {
    private let configuration: NativeLocalAgentHostConfiguration
    private var managedProcess: ManagedLocalAgentHostProcess?
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
        try await launch(ownerUserID: ownerUserID, credentialEnvironment: [:])
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
            credentialEnvironment: credentialEnvironment
        )
    }

    private func launch(
        ownerUserID: String,
        credentialEnvironment: [String: String]
    ) async throws {
        stopLocked()
        do {
            let configuration = configuration
            let managed = try await Task.detached(priority: .userInitiated) {
                try ManagedLocalAgentHostProcess.launch(
                    configuration: configuration,
                    ownerUserID: ownerUserID,
                    credentialEnvironment: credentialEnvironment
                )
            }.value
            managedProcess = managed
            activeOwnerUserID = ownerUserID
            managed.installTerminationHandler { [weak self] identity, processIdentifier in
                Task {
                    await self?.processDidTerminate(
                        identity: identity,
                        processIdentifier: processIdentifier
                    )
                }
            }
        } catch {
            stopLocked()
            throw error
        }
    }

    public func stop() async {
        stopLocked()
    }

    public func request(command: Data) async throws -> Data {
        guard let managedProcess, managedProcess.process.isRunning else {
            throw NativeLocalAgentHostError.notRunning
        }
        let commandValue = try JSONSerialization.jsonObject(with: command)
        guard let commandObject = commandValue as? [String: Any],
              commandObject["type"] is String else {
            throw NativeLocalAgentHostError.invalidCommand
        }
        if let commandOwner = commandObject["owner_user_id"] as? String,
           commandOwner != activeOwnerUserID {
            throw NativeLocalAgentHostError.ownerMismatch
        }
        let commandID = "native-command-\(UUID().uuidString.lowercased())"
        let envelope: [String: Any] = [
            "protocol_version": NativeLocalAgentHostProtocol.version,
            "command_id": commandID,
            "command": commandObject,
        ]
        let request = try JSONSerialization.data(withJSONObject: envelope)
        let responseData: Data
        do {
            responseData = try await managedProcess.roundTripAsync(request)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            invalidateAfterTransportFailure(managedProcess)
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

    private func stopLocked() {
        activeOwnerUserID = nil
        managedProcess?.terminate()
        managedProcess = nil
    }

    private func processDidTerminate(identity: UUID, processIdentifier: Int32) {
        guard let managedProcess, managedProcess.identity == identity else { return }
        activeOwnerUserID = nil
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

enum NativeLocalAgentHostError: LocalizedError, Equatable {
    case invalidConfiguration(String)
    case invalidOwner
    case launchFailed(String)
    case notRunning
    case invalidCommand
    case ownerMismatch
    case invalidCredentialEnvironment
    case invalidFrame
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
        case .requestTimedOut:
            "Local Agent Host did not respond before the request deadline."
        case .invalidResponse:
            "Local Agent Host health response is invalid."
        }
    }
}

final class ManagedLocalAgentHostProcess: @unchecked Sendable {
    let identity = UUID()
    let process: Process
    private let input: FileHandle
    private let output: FileHandle
    private let errors: FileHandle
    private let startupTimeoutMilliseconds: Int
    private let requestTimeoutMilliseconds: Int
    private let writeQueue = DispatchQueue(
        label: "com.chatos.swift.local-agent-host-writer",
        qos: .userInitiated
    )
    private let readQueue = DispatchQueue(
        label: "com.chatos.swift.local-agent-host-reader",
        qos: .userInitiated
    )
    private let transportLock = NSLock()
    private var pendingResponses: [String: PendingResponse] = [:]
    private var transportClosed = false
    private var resourcesClosed = false

    private struct PendingResponse {
        let continuation: CheckedContinuation<Data, Error>
        let timeoutWorkItem: DispatchWorkItem
    }

    private init(
        process: Process,
        input: FileHandle,
        output: FileHandle,
        errors: FileHandle,
        startupTimeoutMilliseconds: Int,
        requestTimeoutMilliseconds: Int
    ) {
        self.process = process
        self.input = input
        self.output = output
        self.errors = errors
        self.startupTimeoutMilliseconds = startupTimeoutMilliseconds
        self.requestTimeoutMilliseconds = requestTimeoutMilliseconds
    }

    static func launch(
        configuration: NativeLocalAgentHostConfiguration,
        ownerUserID: String,
        credentialEnvironment: [String: String]
    ) throws -> ManagedLocalAgentHostProcess {
        let executable = configuration.executableURL.standardizedFileURL
        let values = try executable.resourceValues(forKeys: [
            .isRegularFileKey, .isSymbolicLinkKey, .isExecutableKey,
        ])
        guard executable.isFileURL,
              values.isRegularFile == true,
              values.isSymbolicLink != true,
              values.isExecutable == true else {
            throw NativeLocalAgentHostError.invalidConfiguration(
                "Local Agent Host executable is unavailable."
            )
        }
        let database = configuration.databaseURL.standardizedFileURL
        guard database.isFileURL else {
            throw NativeLocalAgentHostError.invalidConfiguration(
                "Local Agent Host database must be a local file."
            )
        }
        try FileManager.default.createDirectory(
            at: database.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )

        try validateMemoryConfiguration(configuration)
        guard (1...300_000).contains(configuration.requestTimeoutMilliseconds) else {
            throw NativeLocalAgentHostError.invalidConfiguration(
                "Local Agent Host request timeout must be between 1 and 300000 milliseconds."
            )
        }

        let process = Process()
        let inputPipe = Pipe()
        let outputPipe = Pipe()
        let errorPipe = Pipe()
        process.executableURL = executable
        process.currentDirectoryURL = executable.deletingLastPathComponent()
        process.arguments = arguments(
            configuration: configuration,
            database: database,
            ownerUserID: ownerUserID
        )
        process.environment = safeEnvironment(credentialEnvironment: credentialEnvironment)
        process.standardInput = inputPipe
        process.standardOutput = outputPipe
        process.standardError = errorPipe
        NativeProcessPipeReader.install(on: errorPipe.fileHandleForReading)
        do {
            try process.run()
        } catch {
            errorPipe.fileHandleForReading.readabilityHandler = nil
            throw NativeLocalAgentHostError.launchFailed(error.localizedDescription)
        }

        let managed = ManagedLocalAgentHostProcess(
            process: process,
            input: inputPipe.fileHandleForWriting,
            output: outputPipe.fileHandleForReading,
            errors: errorPipe.fileHandleForReading,
            startupTimeoutMilliseconds: Self.timeoutMilliseconds(
                configuration.startupTimeout
            ),
            requestTimeoutMilliseconds: configuration.requestTimeoutMilliseconds
        )
        do {
            try managed.prepareTransport()
        } catch {
            managed.terminate()
            throw error
        }
        let watchdog = Task.detached { [managed] in
            try? await Task.sleep(for: configuration.startupTimeout)
            guard !Task.isCancelled, managed.process.isRunning else { return }
            managed.process.terminate()
        }
        defer { watchdog.cancel() }
        do {
            try managed.verifyHealth()
            managed.startResponseReader()
            return managed
        } catch {
            managed.terminate()
            throw error
        }
    }

    func terminate() {
        process.terminationHandler = nil
        closeResources()
        if process.isRunning {
            process.terminate()
        }
    }

    func installTerminationHandler(
        _ handler: @escaping @Sendable (UUID, Int32) -> Void
    ) {
        let identity = identity
        let processIdentifier = process.processIdentifier
        process.terminationHandler = { _ in
            handler(identity, processIdentifier)
        }
    }

    func closeAfterExit() {
        process.terminationHandler = nil
        closeResources()
    }

    private func closeResources() {
        failTransport(with: NativeLocalAgentHostError.notRunning)
        transportLock.lock()
        guard !resourcesClosed else {
            transportLock.unlock()
            return
        }
        resourcesClosed = true
        transportLock.unlock()
        errors.readabilityHandler = nil
        try? input.close()
        try? output.close()
        try? errors.close()
    }

    private func prepareTransport() throws {
        try Self.setNonBlocking(input.fileDescriptor)
        try Self.setNonBlocking(output.fileDescriptor)
        guard fcntl(input.fileDescriptor, F_SETNOSIGPIPE, 1) != -1 else {
            throw NativeLocalAgentHostError.launchFailed(
                "Local Agent Host input pipe could not be configured."
            )
        }
    }

    private static func setNonBlocking(_ fileDescriptor: Int32) throws {
        let flags = fcntl(fileDescriptor, F_GETFL)
        guard flags != -1,
              fcntl(fileDescriptor, F_SETFL, flags | O_NONBLOCK) != -1 else {
            throw NativeLocalAgentHostError.launchFailed(
                "Local Agent Host pipe could not be configured."
            )
        }
    }

    private func verifyHealth() throws {
        let commandID = "native-health-\(UUID().uuidString.lowercased())"
        let request = HealthRequest(
            protocolVersion: NativeLocalAgentHostProtocol.version,
            commandId: commandID,
            command: .init(type: "health")
        )
        let payload = try roundTrip(
            try JSONEncoder.localAgent.encode(request),
            timeoutMilliseconds: startupTimeoutMilliseconds
        )
        let response = try JSONDecoder.localAgent.decode(
            HealthResponse.self,
            from: payload
        )
        guard response.protocolVersion == NativeLocalAgentHostProtocol.version,
              response.commandId == commandID else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        guard response.ok else {
            throw NativeLocalAgentHostError.hostError(
                code: response.error?.code ?? "health_failed",
                message: response.error?.message ?? "Local Agent Host health check failed.",
                retryable: response.error?.retryable ?? false
            )
        }
        guard response.result?.type == "health",
              response.result?.storageReady == true else {
            throw NativeLocalAgentHostError.invalidResponse
        }
    }

    func roundTrip(
        _ payload: Data,
        timeoutMilliseconds: Int? = nil
    ) throws -> Data {
        let deadline = LocalAgentHostFrameCodec.deadline(
            timeoutMilliseconds: timeoutMilliseconds ?? requestTimeoutMilliseconds
        )
        try LocalAgentHostFrameCodec.write(payload, to: input, deadline: deadline)
        return try LocalAgentHostFrameCodec.read(from: output, deadline: deadline)
    }

    func roundTripAsync(
        _ payload: Data,
        timeoutMilliseconds: Int? = nil
    ) async throws -> Data {
        let commandID = try Self.commandID(in: payload)
        let timeout = timeoutMilliseconds ?? requestTimeoutMilliseconds
        return try await withTaskCancellationHandler {
            try Task.checkCancellation()
            return try await withCheckedThrowingContinuation { continuation in
                register(
                    continuation: continuation,
                    commandID: commandID,
                    payload: payload,
                    timeoutMilliseconds: timeout
                )
                if Task.isCancelled {
                    cancelPendingResponse(commandID: commandID)
                }
            }
        } onCancel: { [self] in
            cancelPendingResponse(commandID: commandID)
        }
    }

    private static func commandID(in payload: Data) throws -> String {
        guard let envelope = try JSONSerialization.jsonObject(with: payload)
                as? [String: Any],
              let commandID = envelope["command_id"] as? String,
              !commandID.isEmpty else {
            throw NativeLocalAgentHostError.invalidCommand
        }
        return commandID
    }

    private func register(
        continuation: CheckedContinuation<Data, Error>,
        commandID: String,
        payload: Data,
        timeoutMilliseconds: Int
    ) {
        let timeoutWorkItem = DispatchWorkItem { [weak self] in
            self?.failPendingResponse(
                commandID: commandID,
                error: NativeLocalAgentHostError.requestTimedOut
            )
        }
        transportLock.lock()
        if transportClosed {
            transportLock.unlock()
            continuation.resume(throwing: NativeLocalAgentHostError.notRunning)
            return
        }
        guard pendingResponses[commandID] == nil else {
            transportLock.unlock()
            continuation.resume(throwing: NativeLocalAgentHostError.invalidCommand)
            return
        }
        pendingResponses[commandID] = PendingResponse(
            continuation: continuation,
            timeoutWorkItem: timeoutWorkItem
        )
        transportLock.unlock()

        DispatchQueue.global(qos: .utility).asyncAfter(
            deadline: .now() + .milliseconds(timeoutMilliseconds),
            execute: timeoutWorkItem
        )
        writeQueue.async { [weak self] in
            guard let self, self.isPending(commandID: commandID) else { return }
            do {
                try LocalAgentHostFrameCodec.write(
                    payload,
                    to: self.input,
                    deadline: LocalAgentHostFrameCodec.deadline(
                        timeoutMilliseconds: timeoutMilliseconds
                    )
                )
            } catch {
                self.failTransport(with: error)
            }
        }
    }

    private func startResponseReader() {
        readQueue.async { [weak self] in
            self?.readResponses()
        }
    }

    private func readResponses() {
        while !isTransportClosed {
            do {
                let response = try LocalAgentHostFrameCodec.read(
                    from: output,
                    deadline: UInt64.max,
                    shouldCancel: { [weak self] in
                        self?.isTransportClosed ?? true
                    }
                )
                let commandID = try Self.commandID(in: response)
                completePendingResponse(commandID: commandID, response: response)
            } catch {
                failTransport(with: error)
                return
            }
        }
    }

    private var isTransportClosed: Bool {
        transportLock.lock()
        defer { transportLock.unlock() }
        return transportClosed
    }

    private func isPending(commandID: String) -> Bool {
        transportLock.lock()
        defer { transportLock.unlock() }
        return pendingResponses[commandID] != nil && !transportClosed
    }

    private func completePendingResponse(commandID: String, response: Data) {
        transportLock.lock()
        let pending = pendingResponses.removeValue(forKey: commandID)
        transportLock.unlock()
        pending?.timeoutWorkItem.cancel()
        pending?.continuation.resume(returning: response)
    }

    private func cancelPendingResponse(commandID: String) {
        failPendingResponse(commandID: commandID, error: CancellationError())
    }

    private func failPendingResponse(commandID: String, error: Error) {
        transportLock.lock()
        let pending = pendingResponses.removeValue(forKey: commandID)
        transportLock.unlock()
        pending?.timeoutWorkItem.cancel()
        pending?.continuation.resume(throwing: error)
    }

    private func failTransport(with error: Error) {
        transportLock.lock()
        guard !transportClosed else {
            transportLock.unlock()
            return
        }
        transportClosed = true
        let pending = Array(pendingResponses.values)
        pendingResponses.removeAll(keepingCapacity: false)
        transportLock.unlock()
        for response in pending {
            response.timeoutWorkItem.cancel()
            response.continuation.resume(throwing: error)
        }
    }

    private static func timeoutMilliseconds(_ duration: Duration) -> Int {
        let components = duration.components
        guard components.seconds >= 0, components.attoseconds >= 0 else { return 1 }
        let maximumSeconds = Int64(Int.max / 1_000)
        guard components.seconds <= maximumSeconds else { return Int.max }
        let wholeMilliseconds = Int(components.seconds) * 1_000
        let attosecondsPerMillisecond: Int64 = 1_000_000_000_000_000
        let fractionalMilliseconds = Int(
            components.attoseconds / attosecondsPerMillisecond
        ) + (components.attoseconds % attosecondsPerMillisecond == 0 ? 0 : 1)
        return max(1, wholeMilliseconds + fractionalMilliseconds)
    }

    static func arguments(
        configuration: NativeLocalAgentHostConfiguration,
        database: URL,
        ownerUserID: String
    ) -> [String] {
        var arguments = [
            "--database", database.path,
            "--owner-user-id", ownerUserID,
        ]
        if let baseURL = configuration.memoryBaseURL,
           let sourceID = configuration.memorySourceID {
            arguments += [
                "--memory-base-url", baseURL.absoluteString,
                "--memory-source-id", sourceID,
                "--memory-timeout-ms", String(configuration.memoryTimeoutMilliseconds),
            ]
        }
        arguments += configuration.readOnlyToolNames.sorted().flatMap {
            ["--read-only-tool", $0]
        }
        arguments += configuration.approvalExemptToolNames.sorted().flatMap {
            ["--approval-exempt-tool", $0]
        }
        arguments.append("--stdio")
        return arguments
    }

    private static func validateMemoryConfiguration(
        _ configuration: NativeLocalAgentHostConfiguration
    ) throws {
        guard configuration.memoryBaseURL != nil || configuration.memorySourceID == nil else {
            throw NativeLocalAgentHostError.invalidConfiguration(
                "Local Agent Host Memory base URL and source ID must be configured together."
            )
        }
        guard configuration.memoryBaseURL == nil || configuration.memorySourceID != nil else {
            throw NativeLocalAgentHostError.invalidConfiguration(
                "Local Agent Host Memory base URL and source ID must be configured together."
            )
        }
        guard let baseURL = configuration.memoryBaseURL,
              let sourceID = configuration.memorySourceID else { return }
        guard ["http", "https"].contains(baseURL.scheme?.lowercased() ?? ""),
              baseURL.host != nil,
              baseURL.user == nil,
              baseURL.password == nil,
              baseURL.query == nil,
              baseURL.fragment == nil,
              !sourceID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              sourceID.count <= 128,
              !sourceID.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains),
              (1...300_000).contains(configuration.memoryTimeoutMilliseconds) else {
            throw NativeLocalAgentHostError.invalidConfiguration(
                "Local Agent Host Memory configuration is invalid."
            )
        }
    }

    static func safeEnvironment(
        credentialEnvironment: [String: String]
    ) -> [String: String] {
        let allowed = ["HOME", "LANG", "LC_ALL", "PATH", "SHELL", "TMPDIR", "USER"]
        let source = ProcessInfo.processInfo.environment
        var environment = Dictionary(uniqueKeysWithValues: allowed.compactMap { key in
            source[key].map { (key, $0) }
        })
        environment.merge(credentialEnvironment) { _, credential in credential }
        return environment
    }
}

enum LocalAgentHostFrameCodec {
    static let maximumFrameBytes = 4 * 1_024 * 1_024
    private static let maximumPollSliceMilliseconds: UInt64 = 250

    static func write(
        _ payload: Data,
        to handle: FileHandle,
        deadline: UInt64 = deadline(timeoutMilliseconds: 75_000)
    ) throws {
        guard !payload.isEmpty, payload.count <= maximumFrameBytes else {
            throw NativeLocalAgentHostError.invalidFrame
        }
        var length = UInt32(payload.count).bigEndian
        let header = withUnsafeBytes(of: &length) { Data($0) }
        try writeExactly(header, to: handle.fileDescriptor, deadline: deadline)
        try writeExactly(payload, to: handle.fileDescriptor, deadline: deadline)
    }

    static func read(
        from handle: FileHandle,
        deadline: UInt64 = deadline(timeoutMilliseconds: 75_000),
        shouldCancel: @Sendable () -> Bool = { false }
    ) throws -> Data {
        let header = try readExactly(
            4,
            from: handle.fileDescriptor,
            deadline: deadline,
            shouldCancel: shouldCancel
        )
        let length = header.withUnsafeBytes { rawBuffer in
            rawBuffer.loadUnaligned(as: UInt32.self).bigEndian
        }
        guard length > 0, length <= maximumFrameBytes else {
            throw NativeLocalAgentHostError.invalidFrame
        }
        return try readExactly(
            Int(length),
            from: handle.fileDescriptor,
            deadline: deadline,
            shouldCancel: shouldCancel
        )
    }

    static func deadline(timeoutMilliseconds: Int) -> UInt64 {
        let now = DispatchTime.now().uptimeNanoseconds
        let milliseconds = UInt64(max(0, timeoutMilliseconds))
        let (nanoseconds, overflow) = milliseconds.multipliedReportingOverflow(by: 1_000_000)
        guard !overflow, UInt64.max - now >= nanoseconds else { return UInt64.max }
        return now + nanoseconds
    }

    private static func writeExactly(
        _ data: Data,
        to fileDescriptor: Int32,
        deadline: UInt64
    ) throws {
        try data.withUnsafeBytes { buffer in
            guard let baseAddress = buffer.baseAddress else {
                throw NativeLocalAgentHostError.invalidFrame
            }
            var offset = 0
            while offset < buffer.count {
                try wait(
                    for: Int16(POLLOUT),
                    fileDescriptor: fileDescriptor,
                    deadline: deadline
                )
                let written = Darwin.write(
                    fileDescriptor,
                    baseAddress.advanced(by: offset),
                    buffer.count - offset
                )
                if written > 0 {
                    offset += written
                } else if written == -1, errno == EINTR || errno == EAGAIN {
                    continue
                } else {
                    throw NativeLocalAgentHostError.invalidFrame
                }
            }
        }
    }

    private static func readExactly(
        _ count: Int,
        from fileDescriptor: Int32,
        deadline: UInt64,
        shouldCancel: @Sendable () -> Bool
    ) throws -> Data {
        var result = Data(count: count)
        try result.withUnsafeMutableBytes { buffer in
            guard let baseAddress = buffer.baseAddress else {
                throw NativeLocalAgentHostError.invalidFrame
            }
            var offset = 0
            while offset < count {
                try wait(
                    for: Int16(POLLIN),
                    fileDescriptor: fileDescriptor,
                    deadline: deadline,
                    shouldCancel: shouldCancel
                )
                let received = Darwin.read(
                    fileDescriptor,
                    baseAddress.advanced(by: offset),
                    count - offset
                )
                if received > 0 {
                    offset += received
                } else if received == -1, errno == EINTR || errno == EAGAIN {
                    continue
                } else {
                    throw NativeLocalAgentHostError.invalidFrame
                }
            }
        }
        return result
    }

    private static func wait(
        for events: Int16,
        fileDescriptor: Int32,
        deadline: UInt64,
        shouldCancel: @Sendable () -> Bool = { false }
    ) throws {
        while true {
            if shouldCancel() { throw CancellationError() }
            let now = DispatchTime.now().uptimeNanoseconds
            guard now < deadline else {
                throw NativeLocalAgentHostError.requestTimedOut
            }
            let remainingNanoseconds = deadline - now
            let wholeMilliseconds = remainingNanoseconds / 1_000_000
            let roundedMilliseconds = wholeMilliseconds
                + (remainingNanoseconds % 1_000_000 == 0 ? 0 : 1)
            // `close(2)` from another thread does not reliably wake an existing `poll(2)` on
            // Darwin. Bound each wait so a stopped/restarted Host can retire its old response
            // reader instead of retaining a queue thread and pipe for days.
            let timeout = Int32(min(
                roundedMilliseconds,
                min(maximumPollSliceMilliseconds, UInt64(Int32.max))
            ))
            var descriptor = pollfd(fd: fileDescriptor, events: events, revents: 0)
            let result = Darwin.poll(&descriptor, 1, timeout)
            if result > 0 {
                if shouldCancel() { throw CancellationError() }
                if descriptor.revents & events != 0 { return }
                throw NativeLocalAgentHostError.invalidFrame
            }
            if result == 0 {
                continue
            }
            if errno != EINTR {
                throw NativeLocalAgentHostError.invalidFrame
            }
        }
    }
}

private struct HealthRequest: Encodable {
    let protocolVersion: Int
    let commandId: String
    let command: HealthCommand
}

private struct HealthCommand: Encodable {
    let type: String
}

private struct HealthResponse: Decodable {
    let protocolVersion: Int
    let commandId: String
    let ok: Bool
    let result: HealthResult?
    let error: HealthError?
}

private struct HealthResult: Decodable {
    let type: String
    let storageReady: Bool
}

private struct HealthError: Decodable {
    let code: String
    let message: String
    let retryable: Bool
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
