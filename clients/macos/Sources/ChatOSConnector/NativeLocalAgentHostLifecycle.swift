import ChatOSCore
import Darwin
import Foundation

public extension Notification.Name {
    static let nativeLocalAgentHostDidExit = Notification.Name(
        "com.chatos.swift.local-agent-host-did-exit"
    )
}

enum NativeLocalAgentHostProtocol {
    static let version = 40
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
