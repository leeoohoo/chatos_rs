import Foundation

struct NativeRelayRequest: Decodable, Sendable {
    var type: String
    var requestID: String
    var ownerUserID: String?
    var deviceID: String?
    var workspaceID: String
    var method: String?
    var path: String?
    var headers: [String: String]
    var body: NativeJSONValue
    var platformSignature: String?
    var platformSignatureKeyID: String?
    var platformSignatureAlgorithm: String?
    var platformTimestamp: Int64?
    var platformNonce: String?

    enum CodingKeys: String, CodingKey {
        case type, method, path, headers, body
        case requestID = "request_id"
        case ownerUserID = "owner_user_id"
        case deviceID = "device_id"
        case workspaceID = "workspace_id"
        case platformSignature = "platform_signature"
        case platformSignatureKeyID = "platform_signature_key_id"
        case platformSignatureAlgorithm = "platform_signature_alg"
        case platformTimestamp = "platform_timestamp"
        case platformNonce = "platform_nonce"
    }

    func header(_ name: String) -> String? {
        headers.first { $0.key.caseInsensitiveCompare(name) == .orderedSame }?
            .value
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .nilIfEmpty
    }
}

struct NativeTerminalRelayBody: Decodable, Sendable {
    var command: String
    var args: [String]
    var cwd: String?
    var timeoutMilliseconds: Int?
    var source: String?

    enum CodingKeys: String, CodingKey {
        case command, args, cwd, source
        case timeoutMilliseconds = "timeout_ms"
    }
}

struct NativeRelayResponse: Encodable, Sendable {
    var type: String
    var requestID: String
    var status: Int
    var headers: [String: String] = [:]
    var body: NativeJSONValue

    enum CodingKeys: String, CodingKey {
        case type, status, headers, body
        case requestID = "request_id"
    }
}

enum NativeJSONValue: Codable, Sendable, Equatable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([NativeJSONValue])
    case object([String: NativeJSONValue])

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() { self = .null }
        else if let value = try? container.decode(Bool.self) { self = .bool(value) }
        else if let value = try? container.decode(Double.self) { self = .number(value) }
        else if let value = try? container.decode(String.self) { self = .string(value) }
        else if let value = try? container.decode([NativeJSONValue].self) { self = .array(value) }
        else { self = .object(try container.decode([String: NativeJSONValue].self)) }
    }

    func encode(to encoder: Encoder) throws {
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

    func decode<T: Decodable>(_ type: T.Type) throws -> T {
        try JSONDecoder().decode(type, from: JSONEncoder().encode(self))
    }

    var canonicalJSONString: String {
        switch self {
        case .null: "null"
        case let .bool(value): value ? "true" : "false"
        case let .number(value):
            value.rounded() == value ? String(Int64(value)) : String(value)
        case let .string(value):
            Self.canonicalJSONString(value)
        case let .array(values):
            "[" + values.map(\.canonicalJSONString).joined(separator: ",") + "]"
        case let .object(values):
            "{" + values.keys.sorted().map { key in
                let encodedKey = Self.canonicalJSONString(key)
                return encodedKey + ":" + (values[key]?.canonicalJSONString ?? "null")
            }.joined(separator: ",") + "}"
        }
    }

    private static func canonicalJSONString(_ value: String) -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.withoutEscapingSlashes]
        return String(data: try! encoder.encode(value), encoding: .utf8) ?? "\"\""
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
