import Foundation

public enum ChatOSMediaWorkerProtocol {
    public static let minimumVersion = 1
    public static let maximumVersion = 1
    public static let buildVersion = "media-worker-v1"
    public static let maximumInlineBytes = 512 * 1_024
    public static let maximumNormalizedImageBytes = 25 * 1_024 * 1_024
    public static let maximumNormalizedImagePixels = 64_000_000
    public static let normalizePNGOperation = "normalize_png_v1"
    public static let extractLastVideoFramePNGOperation = "extract_last_video_frame_png_v1"

    public static let capabilities = [
        normalizePNGOperation,
        extractLastVideoFramePNGOperation,
    ]
}

public struct MediaWorkerFileIdentity: Codable, Sendable, Equatable {
    public let device: UInt64
    public let inode: UInt64
    public let byteCount: Int64

    public init(device: UInt64, inode: UInt64, byteCount: Int64) {
        self.device = device
        self.inode = inode
        self.byteCount = byteCount
    }

    private enum CodingKeys: String, CodingKey {
        case device, inode
        case byteCount = "byte_count"
    }
}

public struct MediaWorkerHelloRequest: Codable, Sendable, Equatable {
    public let type: String
    public let protocolMinVersion: Int
    public let protocolMaxVersion: Int
    public let clientInstanceID: String
    public let parentProcessID: Int32
    public let capabilities: [String]
    public let maximumInlineBytes: Int

    public init(clientInstanceID: String, parentProcessID: Int32) {
        type = "hello"
        protocolMinVersion = ChatOSMediaWorkerProtocol.minimumVersion
        protocolMaxVersion = ChatOSMediaWorkerProtocol.maximumVersion
        self.clientInstanceID = clientInstanceID
        self.parentProcessID = parentProcessID
        capabilities = ChatOSMediaWorkerProtocol.capabilities
        maximumInlineBytes = ChatOSMediaWorkerProtocol.maximumInlineBytes
    }

    private enum CodingKeys: String, CodingKey {
        case type, capabilities
        case protocolMinVersion = "protocol_min_version"
        case protocolMaxVersion = "protocol_max_version"
        case clientInstanceID = "client_instance_id"
        case parentProcessID = "parent_process_id"
        case maximumInlineBytes = "maximum_inline_bytes"
    }
}

public struct MediaWorkerHelloResponse: Codable, Sendable, Equatable {
    public let type: String
    public let ok: Bool
    public let selectedProtocolVersion: Int?
    public let workerBuildVersion: String
    public let helperInstanceID: String
    public let capabilities: [String]
    public let maximumInlineBytes: Int
    public let error: MediaWorkerErrorPayload?

    public init(
        ok: Bool,
        selectedProtocolVersion: Int?,
        helperInstanceID: String,
        error: MediaWorkerErrorPayload? = nil
    ) {
        type = "hello_response"
        self.ok = ok
        self.selectedProtocolVersion = selectedProtocolVersion
        workerBuildVersion = ChatOSMediaWorkerProtocol.buildVersion
        self.helperInstanceID = helperInstanceID
        capabilities = ChatOSMediaWorkerProtocol.capabilities
        maximumInlineBytes = ChatOSMediaWorkerProtocol.maximumInlineBytes
        self.error = error
    }

    private enum CodingKeys: String, CodingKey {
        case type, ok, capabilities, error
        case selectedProtocolVersion = "selected_protocol_version"
        case workerBuildVersion = "worker_build_version"
        case helperInstanceID = "helper_instance_id"
        case maximumInlineBytes = "maximum_inline_bytes"
    }
}

public struct MediaWorkerExtractVideoFrameRequest: Codable, Sendable, Equatable {
    public let type: String
    public let protocolVersion: Int
    public let requestID: String
    public let ownerUserID: String
    public let accountSessionID: String
    public let sessionEpoch: String
    public let helperInstanceID: String
    public let operation: String
    public let deadlineUnixMilliseconds: Int64
    public let sourcePath: String
    public let sourceIdentity: MediaWorkerFileIdentity
    public let destinationPath: String
    public let destinationIdentity: MediaWorkerFileIdentity
    public let maximumSourceBytes: Int
    public let maximumOutputPixelSize: Int
    public let maximumOutputBytes: Int

    public init(
        protocolVersion: Int,
        requestID: String,
        ownerUserID: String,
        accountSessionID: String,
        sessionEpoch: String,
        helperInstanceID: String,
        deadlineUnixMilliseconds: Int64,
        sourcePath: String,
        sourceIdentity: MediaWorkerFileIdentity,
        destinationPath: String,
        destinationIdentity: MediaWorkerFileIdentity,
        maximumSourceBytes: Int,
        maximumOutputPixelSize: Int,
        maximumOutputBytes: Int
    ) {
        type = "request"
        self.protocolVersion = protocolVersion
        self.requestID = requestID
        self.ownerUserID = ownerUserID
        self.accountSessionID = accountSessionID
        self.sessionEpoch = sessionEpoch
        self.helperInstanceID = helperInstanceID
        operation = ChatOSMediaWorkerProtocol.extractLastVideoFramePNGOperation
        self.deadlineUnixMilliseconds = deadlineUnixMilliseconds
        self.sourcePath = sourcePath
        self.sourceIdentity = sourceIdentity
        self.destinationPath = destinationPath
        self.destinationIdentity = destinationIdentity
        self.maximumSourceBytes = maximumSourceBytes
        self.maximumOutputPixelSize = maximumOutputPixelSize
        self.maximumOutputBytes = maximumOutputBytes
    }

    private enum CodingKeys: String, CodingKey {
        case type, operation
        case protocolVersion = "protocol_version"
        case requestID = "request_id"
        case ownerUserID = "owner_user_id"
        case accountSessionID = "account_session_id"
        case sessionEpoch = "session_epoch"
        case helperInstanceID = "helper_instance_id"
        case deadlineUnixMilliseconds = "deadline_unix_milliseconds"
        case sourcePath = "source_path"
        case sourceIdentity = "source_identity"
        case destinationPath = "destination_path"
        case destinationIdentity = "destination_identity"
        case maximumSourceBytes = "maximum_source_bytes"
        case maximumOutputPixelSize = "maximum_output_pixel_size"
        case maximumOutputBytes = "maximum_output_bytes"
    }
}

public struct MediaWorkerNormalizePNGRequest: Codable, Sendable, Equatable {
    public let type: String
    public let protocolVersion: Int
    public let requestID: String
    public let ownerUserID: String
    public let accountSessionID: String
    public let sessionEpoch: String
    public let helperInstanceID: String
    public let operation: String
    public let deadlineUnixMilliseconds: Int64
    public let sourcePath: String
    public let sourceIdentity: MediaWorkerFileIdentity
    public let destinationPath: String
    public let destinationIdentity: MediaWorkerFileIdentity
    public let maximumSourceBytes: Int
    public let maximumSourcePixelCount: Int
    public let maximumOutputPixelSize: Int
    public let maximumOutputBytes: Int

    public init(
        protocolVersion: Int,
        requestID: String,
        ownerUserID: String,
        accountSessionID: String,
        sessionEpoch: String,
        helperInstanceID: String,
        deadlineUnixMilliseconds: Int64,
        sourcePath: String,
        sourceIdentity: MediaWorkerFileIdentity,
        destinationPath: String,
        destinationIdentity: MediaWorkerFileIdentity,
        maximumSourceBytes: Int,
        maximumSourcePixelCount: Int,
        maximumOutputPixelSize: Int,
        maximumOutputBytes: Int
    ) {
        type = "request"
        self.protocolVersion = protocolVersion
        self.requestID = requestID
        self.ownerUserID = ownerUserID
        self.accountSessionID = accountSessionID
        self.sessionEpoch = sessionEpoch
        self.helperInstanceID = helperInstanceID
        operation = ChatOSMediaWorkerProtocol.normalizePNGOperation
        self.deadlineUnixMilliseconds = deadlineUnixMilliseconds
        self.sourcePath = sourcePath
        self.sourceIdentity = sourceIdentity
        self.destinationPath = destinationPath
        self.destinationIdentity = destinationIdentity
        self.maximumSourceBytes = maximumSourceBytes
        self.maximumSourcePixelCount = maximumSourcePixelCount
        self.maximumOutputPixelSize = maximumOutputPixelSize
        self.maximumOutputBytes = maximumOutputBytes
    }

    private enum CodingKeys: String, CodingKey {
        case type, operation
        case protocolVersion = "protocol_version"
        case requestID = "request_id"
        case ownerUserID = "owner_user_id"
        case accountSessionID = "account_session_id"
        case sessionEpoch = "session_epoch"
        case helperInstanceID = "helper_instance_id"
        case deadlineUnixMilliseconds = "deadline_unix_milliseconds"
        case sourcePath = "source_path"
        case sourceIdentity = "source_identity"
        case destinationPath = "destination_path"
        case destinationIdentity = "destination_identity"
        case maximumSourceBytes = "maximum_source_bytes"
        case maximumSourcePixelCount = "maximum_source_pixel_count"
        case maximumOutputPixelSize = "maximum_output_pixel_size"
        case maximumOutputBytes = "maximum_output_bytes"
    }
}

public struct MediaWorkerImageDescriptor: Codable, Sendable, Equatable {
    public let path: String
    public let mimeType: String
    public let byteCount: Int
    public let sha256: String
    public let pixelWidth: Int
    public let pixelHeight: Int

    public init(
        path: String,
        mimeType: String,
        byteCount: Int,
        sha256: String,
        pixelWidth: Int,
        pixelHeight: Int
    ) {
        self.path = path
        self.mimeType = mimeType
        self.byteCount = byteCount
        self.sha256 = sha256
        self.pixelWidth = pixelWidth
        self.pixelHeight = pixelHeight
    }

    private enum CodingKeys: String, CodingKey {
        case path, sha256
        case mimeType = "mime_type"
        case byteCount = "byte_count"
        case pixelWidth = "pixel_width"
        case pixelHeight = "pixel_height"
    }
}

public struct MediaWorkerNormalizePNGResponse: Codable, Sendable, Equatable {
    public let type: String
    public let requestID: String
    public let ok: Bool
    public let result: MediaWorkerImageDescriptor?
    public let error: MediaWorkerErrorPayload?
    public let retryable: Bool

    public init(
        requestID: String,
        result: MediaWorkerImageDescriptor? = nil,
        error: MediaWorkerErrorPayload? = nil,
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

public struct MediaWorkerErrorPayload: Codable, Sendable, Equatable {
    public let code: String
    public let message: String

    public init(code: String, message: String) {
        self.code = code
        self.message = message
    }
}

public enum MediaWorkerWireCodec {
    public static func encodeLine<T: Encodable>(_ value: T) throws -> Data {
        let encoder = JSONEncoder()
        var data = try encoder.encode(value)
        guard data.count < ChatOSMediaWorkerProtocol.maximumInlineBytes else {
            throw MediaWorkerWireError.messageTooLarge
        }
        data.append(0x0A)
        return data
    }

    public static func decode<T: Decodable>(_ type: T.Type, from data: Data) throws -> T {
        guard data.count <= ChatOSMediaWorkerProtocol.maximumInlineBytes else {
            throw MediaWorkerWireError.messageTooLarge
        }
        let decoder = JSONDecoder()
        return try decoder.decode(type, from: data)
    }
}

public enum MediaWorkerWireError: Error, Sendable {
    case messageTooLarge
}
