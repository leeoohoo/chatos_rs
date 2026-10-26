import Foundation

public enum ChatOSCodeReadWorkerProtocol {
    public static let minimumVersion = 3
    public static let maximumVersion = 3
    public static let buildVersion = "code-read-worker-v3"
    public static let maximumInlineBytes = 16 * 1_024 * 1_024
    public static let maximumBinaryPayloadBytes = 20 * 1_024 * 1_024
    public static let operations = [
        "read_file_raw", "read_file_range", "list_dir", "search_text",
        "read_file", "search_files", "open_file_in_pet", "verify_file_identity_v1",
        "artifact_stage_v1", "artifact_restore_v1",
        "cleanup_stale_temporary_files_v1",
        "project_list_v1", "project_search_entries_v1",
        "project_run_analyze_v1", "project_write_file_v1",
        "project_create_directory_v1", "project_delete_entry_v1",
        "attachment_authorize_v1",
        "attachment_resolve_v1", "attachment_verify_file_v1",
        "attachment_materialize_v1",
    ]
}

public enum CodeReadWorkerJSONValue: Codable, Sendable, Equatable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([CodeReadWorkerJSONValue])
    case object([String: CodeReadWorkerJSONValue])

    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() { self = .null }
        else if let value = try? container.decode(Bool.self) { self = .bool(value) }
        else if let value = try? container.decode(Double.self) { self = .number(value) }
        else if let value = try? container.decode(String.self) { self = .string(value) }
        else if let value = try? container.decode([CodeReadWorkerJSONValue].self) {
            self = .array(value)
        } else {
            self = .object(try container.decode([String: CodeReadWorkerJSONValue].self))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .null: try container.encodeNil()
        case let .bool(value): try container.encode(value)
        case let .number(value): try container.encode(value)
        case let .string(value): try container.encode(value)
        case let .array(value): try container.encode(value)
        case let .object(value): try container.encode(value)
        }
    }
}

public struct CodeReadWorkerFileIdentity: Codable, Sendable, Equatable {
    public let device: UInt64
    public let inode: UInt64

    public init(device: UInt64, inode: UInt64) {
        self.device = device
        self.inode = inode
    }
}

public struct CodeReadWorkerHelloRequest: Codable, Sendable, Equatable {
    public let type: String
    public let protocolMinVersion: Int
    public let protocolMaxVersion: Int
    public let clientInstanceID: String
    public let parentProcessID: Int32
    public let maximumInlineBytes: Int
    public let maximumBinaryPayloadBytes: Int

    public init(clientInstanceID: String, parentProcessID: Int32) {
        type = "hello"
        protocolMinVersion = ChatOSCodeReadWorkerProtocol.minimumVersion
        protocolMaxVersion = ChatOSCodeReadWorkerProtocol.maximumVersion
        self.clientInstanceID = clientInstanceID
        self.parentProcessID = parentProcessID
        maximumInlineBytes = ChatOSCodeReadWorkerProtocol.maximumInlineBytes
        maximumBinaryPayloadBytes = ChatOSCodeReadWorkerProtocol.maximumBinaryPayloadBytes
    }

    private enum CodingKeys: String, CodingKey {
        case type
        case protocolMinVersion = "protocol_min_version"
        case protocolMaxVersion = "protocol_max_version"
        case clientInstanceID = "client_instance_id"
        case parentProcessID = "parent_process_id"
        case maximumInlineBytes = "maximum_inline_bytes"
        case maximumBinaryPayloadBytes = "maximum_binary_payload_bytes"
    }
}

public struct CodeReadWorkerHelloResponse: Codable, Sendable, Equatable {
    public let type: String
    public let ok: Bool
    public let selectedProtocolVersion: Int?
    public let workerBuildVersion: String
    public let helperInstanceID: String
    public let operations: [String]
    public let maximumInlineBytes: Int
    public let maximumBinaryPayloadBytes: Int
    public let error: CodeReadWorkerErrorPayload?

    public init(
        ok: Bool,
        selectedProtocolVersion: Int?,
        helperInstanceID: String,
        error: CodeReadWorkerErrorPayload? = nil
    ) {
        type = "hello_response"
        self.ok = ok
        self.selectedProtocolVersion = selectedProtocolVersion
        workerBuildVersion = ChatOSCodeReadWorkerProtocol.buildVersion
        self.helperInstanceID = helperInstanceID
        operations = ChatOSCodeReadWorkerProtocol.operations
        maximumInlineBytes = ChatOSCodeReadWorkerProtocol.maximumInlineBytes
        maximumBinaryPayloadBytes = ChatOSCodeReadWorkerProtocol.maximumBinaryPayloadBytes
        self.error = error
    }

    private enum CodingKeys: String, CodingKey {
        case type, ok, operations, error
        case selectedProtocolVersion = "selected_protocol_version"
        case workerBuildVersion = "worker_build_version"
        case helperInstanceID = "helper_instance_id"
        case maximumInlineBytes = "maximum_inline_bytes"
        case maximumBinaryPayloadBytes = "maximum_binary_payload_bytes"
    }
}

public struct CodeReadWorkerRequest: Codable, Sendable, Equatable {
    public let type: String
    public let protocolVersion: Int
    public let requestID: String
    public let helperInstanceID: String
    public let deadlineUnixMilliseconds: Int64
    public let operation: String
    public let arguments: [String: CodeReadWorkerJSONValue]
    public let projectRootPath: String
    public let projectRootIdentity: CodeReadWorkerFileIdentity
    public let requestCWD: String?
    public let defaultToolRoot: String?
    public let ripgrepPath: String?
    public let ripgrepIdentity: CodeReadWorkerFileIdentity?
    public let binaryPayloadByteCount: UInt64

    public init(
        protocolVersion: Int,
        requestID: String,
        helperInstanceID: String,
        deadlineUnixMilliseconds: Int64,
        operation: String,
        arguments: [String: CodeReadWorkerJSONValue],
        projectRootPath: String,
        projectRootIdentity: CodeReadWorkerFileIdentity,
        requestCWD: String?,
        defaultToolRoot: String?,
        ripgrepPath: String?,
        ripgrepIdentity: CodeReadWorkerFileIdentity?,
        binaryPayloadByteCount: UInt64 = 0
    ) {
        type = "request"
        self.protocolVersion = protocolVersion
        self.requestID = requestID
        self.helperInstanceID = helperInstanceID
        self.deadlineUnixMilliseconds = deadlineUnixMilliseconds
        self.operation = operation
        self.arguments = arguments
        self.projectRootPath = projectRootPath
        self.projectRootIdentity = projectRootIdentity
        self.requestCWD = requestCWD
        self.defaultToolRoot = defaultToolRoot
        self.ripgrepPath = ripgrepPath
        self.ripgrepIdentity = ripgrepIdentity
        self.binaryPayloadByteCount = binaryPayloadByteCount
    }

    private enum CodingKeys: String, CodingKey {
        case type, operation, arguments
        case protocolVersion = "protocol_version"
        case requestID = "request_id"
        case helperInstanceID = "helper_instance_id"
        case deadlineUnixMilliseconds = "deadline_unix_milliseconds"
        case projectRootPath = "project_root_path"
        case projectRootIdentity = "project_root_identity"
        case requestCWD = "request_cwd"
        case defaultToolRoot = "default_tool_root"
        case ripgrepPath = "ripgrep_path"
        case ripgrepIdentity = "ripgrep_identity"
        case binaryPayloadByteCount = "binary_payload_byte_count"
    }
}

public struct CodeReadWorkerResponse: Codable, Sendable, Equatable {
    public let type: String
    public let requestID: String
    public let ok: Bool
    public let result: CodeReadWorkerJSONValue?
    public let error: CodeReadWorkerErrorPayload?
    public let retryable: Bool

    public init(
        requestID: String,
        result: CodeReadWorkerJSONValue? = nil,
        error: CodeReadWorkerErrorPayload? = nil,
        retryable: Bool = false
    ) {
        type = "response"
        self.requestID = requestID
        ok = result != nil && error == nil
        self.result = result
        self.error = error
        self.retryable = retryable
    }

    private enum CodingKeys: String, CodingKey {
        case type, ok, result, error, retryable
        case requestID = "request_id"
    }
}

public struct CodeReadWorkerErrorPayload: Codable, Sendable, Equatable {
    public let code: String
    public let message: String

    public init(code: String, message: String) {
        self.code = code
        self.message = message
    }
}

public enum CodeReadWorkerWireCodec {
    public static func encodeLine<T: Encodable>(_ value: T) throws -> Data {
        var data = try JSONEncoder().encode(value)
        guard data.count < ChatOSCodeReadWorkerProtocol.maximumInlineBytes else {
            throw CodeReadWorkerWireError.messageTooLarge
        }
        data.append(0x0A)
        return data
    }

    public static func decode<T: Decodable>(_ type: T.Type, from data: Data) throws -> T {
        guard data.count <= ChatOSCodeReadWorkerProtocol.maximumInlineBytes else {
            throw CodeReadWorkerWireError.messageTooLarge
        }
        return try JSONDecoder().decode(type, from: data)
    }
}

public enum CodeReadWorkerWireError: Error, Sendable {
    case messageTooLarge
}
