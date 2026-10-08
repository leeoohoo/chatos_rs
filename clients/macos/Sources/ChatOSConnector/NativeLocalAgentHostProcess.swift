import ChatOSCore
import Darwin
import Foundation

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
        credentialEnvironment: [String: String],
        workersEnabled: Bool
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
            ownerUserID: ownerUserID,
            workersEnabled: workersEnabled
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
        ownerUserID: String,
        workersEnabled: Bool = true
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
        if !workersEnabled {
            arguments.append("--disable-workers")
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
