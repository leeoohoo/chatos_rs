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

public actor NativeLocalAgentHostLifecycle: LocalAgentHostLifecycleServicing {
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
        stopLocked()
        do {
            let configuration = configuration
            let managed = try await Task.detached(priority: .userInitiated) {
                try ManagedLocalAgentHostProcess.launch(
                    configuration: configuration,
                    ownerUserID: ownerUserID
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
}

enum NativeLocalAgentHostError: LocalizedError, Equatable {
    case invalidConfiguration(String)
    case invalidOwner
    case launchFailed(String)
    case invalidFrame
    case invalidResponse
    case hostError(String)

    var errorDescription: String? {
        switch self {
        case let .invalidConfiguration(message), let .launchFailed(message), let .hostError(message):
            message
        case .invalidOwner:
            "Local Agent owner must be 1...256 non-control characters."
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
        ownerUserID: String
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
        process.environment = safeEnvironment()
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
        try LocalAgentHostFrameCodec.write(
            try JSONEncoder.localAgent.encode(request),
            to: input
        )
        let response = try JSONDecoder.localAgent.decode(
            HealthResponse.self,
            from: LocalAgentHostFrameCodec.read(from: output)
        )
        guard response.protocolVersion == 25,
              response.commandId == commandID else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        guard response.ok else {
            throw NativeLocalAgentHostError.hostError(
                response.error?.message ?? "Local Agent Host health check failed."
            )
        }
        guard response.result?.type == "health",
              response.result?.storageReady == true else {
            throw NativeLocalAgentHostError.invalidResponse
        }
    }

    private static func safeEnvironment() -> [String: String] {
        let allowed = ["HOME", "LANG", "LC_ALL", "PATH", "SHELL", "TMPDIR", "USER"]
        let source = ProcessInfo.processInfo.environment
        return Dictionary(uniqueKeysWithValues: allowed.compactMap { key in
            source[key].map { (key, $0) }
        })
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
    let message: String
}

private extension JSONEncoder {
    static var localAgent: JSONEncoder {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        return encoder
    }
}

private extension JSONDecoder {
    static var localAgent: JSONDecoder {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }
}
