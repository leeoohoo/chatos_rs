import ChatOSCore
import Foundation

public struct NativeLocalAgentHostConfiguration: Sendable, Equatable {
    public let executableURL: URL
    public let databaseURL: URL
    public let startupTimeout: Duration

    public init(
        executableURL: URL,
        databaseURL: URL,
        startupTimeout: Duration = .seconds(10)
    ) {
        self.executableURL = executableURL
        self.databaseURL = databaseURL
        self.startupTimeout = startupTimeout
    }
}

public actor NativeLocalAgentHostLifecycle: LocalAgentHostClientServicing {
    private let configuration: NativeLocalAgentHostConfiguration
    private var managedProcess: ManagedLocalAgentHostProcess?
    public private(set) var activeOwnerUserID: String?

    public init(configuration: NativeLocalAgentHostConfiguration) {
        self.configuration = configuration
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
            "protocol_version": 25,
            "command_id": commandID,
            "command": commandObject,
        ]
        let request = try JSONSerialization.data(withJSONObject: envelope)
        let responseData = try managedProcess.roundTrip(request)
        guard let response = try JSONSerialization.jsonObject(with: responseData)
            as? [String: Any],
              response["protocol_version"] as? Int == 25,
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
            guard name.hasPrefix("CHATOS_LOCAL_AGENT_MODEL_"),
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
        case .invalidResponse:
            "Local Agent Host health response is invalid."
        }
    }
}

final class ManagedLocalAgentHostProcess: @unchecked Sendable {
    let process: Process
    private let input: FileHandle
    private let output: FileHandle
    private let errors: FileHandle

    private init(process: Process, input: FileHandle, output: FileHandle, errors: FileHandle) {
        self.process = process
        self.input = input
        self.output = output
        self.errors = errors
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

        let process = Process()
        let inputPipe = Pipe()
        let outputPipe = Pipe()
        let errorPipe = Pipe()
        process.executableURL = executable
        process.currentDirectoryURL = executable.deletingLastPathComponent()
        process.arguments = [
            "--database", database.path,
            "--owner-user-id", ownerUserID,
            "--stdio",
        ]
        process.environment = safeEnvironment(credentialEnvironment: credentialEnvironment)
        process.standardInput = inputPipe
        process.standardOutput = outputPipe
        process.standardError = errorPipe
        errorPipe.fileHandleForReading.readabilityHandler = { handle in
            _ = handle.availableData
        }
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
            errors: errorPipe.fileHandleForReading
        )
        let watchdog = Task.detached { [managed] in
            try? await Task.sleep(for: configuration.startupTimeout)
            guard !Task.isCancelled, managed.process.isRunning else { return }
            managed.process.terminate()
        }
        defer { watchdog.cancel() }
        do {
            try managed.verifyHealth()
            return managed
        } catch {
            managed.terminate()
            throw error
        }
    }

    func terminate() {
        errors.readabilityHandler = nil
        try? input.close()
        try? output.close()
        try? errors.close()
        if process.isRunning {
            process.terminate()
        }
    }

    private func verifyHealth() throws {
        let commandID = "native-health-\(UUID().uuidString.lowercased())"
        let request = HealthRequest(
            protocolVersion: 25,
            commandId: commandID,
            command: .init(type: "health")
        )
        let payload = try roundTrip(try JSONEncoder.localAgent.encode(request))
        let response = try JSONDecoder.localAgent.decode(
            HealthResponse.self,
            from: payload
        )
        guard response.protocolVersion == 25,
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

    func roundTrip(_ payload: Data) throws -> Data {
        try LocalAgentHostFrameCodec.write(payload, to: input)
        return try LocalAgentHostFrameCodec.read(from: output)
    }

    private static func safeEnvironment(
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
    static let maximumFrameBytes = 1_024 * 1_024

    static func write(_ payload: Data, to handle: FileHandle) throws {
        guard !payload.isEmpty, payload.count <= maximumFrameBytes else {
            throw NativeLocalAgentHostError.invalidFrame
        }
        var length = UInt32(payload.count).bigEndian
        let header = withUnsafeBytes(of: &length) { Data($0) }
        try handle.write(contentsOf: header)
        try handle.write(contentsOf: payload)
    }

    static func read(from handle: FileHandle) throws -> Data {
        let header = try readExactly(4, from: handle)
        let length = header.withUnsafeBytes { rawBuffer in
            rawBuffer.loadUnaligned(as: UInt32.self).bigEndian
        }
        guard length > 0, length <= maximumFrameBytes else {
            throw NativeLocalAgentHostError.invalidFrame
        }
        return try readExactly(Int(length), from: handle)
    }

    private static func readExactly(_ count: Int, from handle: FileHandle) throws -> Data {
        var result = Data()
        result.reserveCapacity(count)
        while result.count < count {
            guard let chunk = try handle.read(upToCount: count - result.count),
                  !chunk.isEmpty else {
                throw NativeLocalAgentHostError.invalidFrame
            }
            result.append(chunk)
        }
        return result
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
