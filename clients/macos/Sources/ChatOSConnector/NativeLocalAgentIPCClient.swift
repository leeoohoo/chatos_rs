// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Darwin
import Foundation

public enum NativeLocalAgentIPCError: Error, Equatable, Sendable {
    case invalidConfiguration(String)
    case socketUnavailable(Int32)
    case serverIdentityMismatch
    case writeFailed(Int32)
    case connectionClosed
    case responseFrameTooLarge(Int)
    case invalidResponse
    case protocolMismatch(UInt32)
    case requestMismatch(expected: String, actual: String)
    case unexpectedResponse(expected: String, actual: String)
    case rejected(LocalAgentIPCErrorPayload)
}

extension NativeLocalAgentIPCError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case let .invalidConfiguration(message): message
        case let .socketUnavailable(code): "本地 Agent IPC 不可用（\(code)）"
        case .serverIdentityMismatch: "本地 Agent Host 身份校验失败"
        case let .writeFailed(code): "本地 Agent IPC 写入失败（\(code)）"
        case .connectionClosed: "本地 Agent Host 在返回完整结果前关闭了连接"
        case let .responseFrameTooLarge(size): "本地 Agent IPC 返回帧过大（\(size) 字节）"
        case .invalidResponse: "本地 Agent Host 返回了无效协议数据"
        case let .protocolMismatch(version): "本地 Agent 协议版本不匹配（\(version)）"
        case let .requestMismatch(expected, actual):
            "本地 Agent 响应串线（期望 \(expected)，收到 \(actual)）"
        case let .unexpectedResponse(expected, actual):
            "本地 Agent 返回类型错误（期望 \(expected)，收到 \(actual)）"
        case let .rejected(error): error.message
        }
    }
}

public protocol LocalAgentFrameTransport: Sendable {
    func exchange(_ request: Data) async throws -> Data
}

/// One-request-per-connection Unix-domain-socket transport. Before sending a
/// frame it verifies both the socket file and the connected peer belong to the
/// current user, matching the Rust Host's reciprocal UID check.
public final class NativeLocalAgentUnixTransport: LocalAgentFrameTransport, @unchecked Sendable {
    private let socketPath: String
    private let maximumFrameBytes: Int
    private let ioTimeoutSeconds: Int
    private let queue = DispatchQueue(
        label: "com.chatos.local.local-agent-ipc",
        qos: .userInitiated,
        attributes: .concurrent
    )

    public init(
        socketPath: String,
        maximumFrameBytes: Int = 8 * 1024 * 1024,
        ioTimeoutSeconds: Int = 35
    ) throws {
        guard socketPath.hasPrefix("/"),
              !socketPath.utf8.contains(0),
              maximumFrameBytes > 0,
              maximumFrameBytes <= Int(UInt32.max),
              (1...300).contains(ioTimeoutSeconds)
        else {
            throw NativeLocalAgentIPCError.invalidConfiguration(
                "本地 Agent socket 路径或帧限制无效"
            )
        }
        let address = sockaddr_un()
        let capacity = MemoryLayout.size(ofValue: address.sun_path)
        guard socketPath.utf8CString.count <= capacity else {
            throw NativeLocalAgentIPCError.invalidConfiguration(
                "本地 Agent socket 路径超过 macOS 限制"
            )
        }
        self.socketPath = socketPath
        self.maximumFrameBytes = maximumFrameBytes
        self.ioTimeoutSeconds = ioTimeoutSeconds
    }

    public func exchange(_ request: Data) async throws -> Data {
        guard !request.isEmpty, request.count <= maximumFrameBytes else {
            throw NativeLocalAgentIPCError.invalidConfiguration("本地 Agent 请求帧大小无效")
        }
        return try await withCheckedThrowingContinuation { continuation in
            queue.async { [self] in
                continuation.resume(with: Result { try exchangeSynchronously(request) })
            }
        }
    }

    private func exchangeSynchronously(_ request: Data) throws -> Data {
        try verifySocketFile()
        let descriptor = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard descriptor >= 0 else {
            throw NativeLocalAgentIPCError.socketUnavailable(errno)
        }
        defer { Darwin.close(descriptor) }

        var noSignal: Int32 = 1
        guard setsockopt(
            descriptor,
            SOL_SOCKET,
            SO_NOSIGPIPE,
            &noSignal,
            socklen_t(MemoryLayout<Int32>.size)
        ) == 0 else {
            throw NativeLocalAgentIPCError.socketUnavailable(errno)
        }
        var timeout = timeval(tv_sec: ioTimeoutSeconds, tv_usec: 0)
        guard setsockopt(
            descriptor,
            SOL_SOCKET,
            SO_RCVTIMEO,
            &timeout,
            socklen_t(MemoryLayout<timeval>.size)
        ) == 0,
            setsockopt(
                descriptor,
                SOL_SOCKET,
                SO_SNDTIMEO,
                &timeout,
                socklen_t(MemoryLayout<timeval>.size)
            ) == 0
        else {
            throw NativeLocalAgentIPCError.socketUnavailable(errno)
        }
        try connect(descriptor)
        try verifyPeer(descriptor)

        var length = UInt32(request.count).bigEndian
        try withUnsafeBytes(of: &length) { try writeAll(descriptor, bytes: $0) }
        try request.withUnsafeBytes { try writeAll(descriptor, bytes: $0) }

        var responseLengthBytes = [UInt8](repeating: 0, count: 4)
        try responseLengthBytes.withUnsafeMutableBytes { try readAll(descriptor, bytes: $0) }
        let responseLength = responseLengthBytes.reduce(UInt32(0)) { value, byte in
            (value << 8) | UInt32(byte)
        }
        guard responseLength > 0, responseLength <= UInt32(maximumFrameBytes) else {
            throw NativeLocalAgentIPCError.responseFrameTooLarge(Int(responseLength))
        }
        var response = Data(count: Int(responseLength))
        try response.withUnsafeMutableBytes { try readAll(descriptor, bytes: $0) }
        return response
    }

    private func verifySocketFile() throws {
        var metadata = stat()
        guard lstat(socketPath, &metadata) == 0 else {
            throw NativeLocalAgentIPCError.socketUnavailable(errno)
        }
        guard metadata.st_mode & S_IFMT == S_IFSOCK,
              metadata.st_uid == geteuid(),
              metadata.st_mode & 0o077 == 0
        else {
            throw NativeLocalAgentIPCError.serverIdentityMismatch
        }
    }

    private func connect(_ descriptor: Int32) throws {
        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        let path = socketPath.utf8CString
        withUnsafeMutableBytes(of: &address.sun_path) { destination in
            path.withUnsafeBytes { source in destination.copyBytes(from: source) }
        }
        let addressLength = socklen_t(
            MemoryLayout<sa_family_t>.size + path.count
        )
        address.sun_len = UInt8(addressLength)
        let result = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.connect(descriptor, $0, addressLength)
            }
        }
        guard result == 0 else {
            throw NativeLocalAgentIPCError.socketUnavailable(errno)
        }
    }

    private func verifyPeer(_ descriptor: Int32) throws {
        var userID: uid_t = 0
        var groupID: gid_t = 0
        guard getpeereid(descriptor, &userID, &groupID) == 0,
              userID == geteuid()
        else {
            throw NativeLocalAgentIPCError.serverIdentityMismatch
        }
    }

    private func writeAll(_ descriptor: Int32, bytes: UnsafeRawBufferPointer) throws {
        var offset = 0
        while offset < bytes.count {
            let result = Darwin.write(
                descriptor,
                bytes.baseAddress!.advanced(by: offset),
                bytes.count - offset
            )
            if result > 0 { offset += result }
            else if result < 0, errno == EINTR { continue }
            else { throw NativeLocalAgentIPCError.writeFailed(errno) }
        }
    }

    private func readAll(_ descriptor: Int32, bytes: UnsafeMutableRawBufferPointer) throws {
        var offset = 0
        while offset < bytes.count {
            let result = Darwin.read(
                descriptor,
                bytes.baseAddress!.advanced(by: offset),
                bytes.count - offset
            )
            if result > 0 { offset += result }
            else if result < 0, errno == EINTR { continue }
            else if result == 0 { throw NativeLocalAgentIPCError.connectionClosed }
            else { throw NativeLocalAgentIPCError.socketUnavailable(errno) }
        }
    }
}

public actor NativeLocalAgentIPCClient {
    private struct Request: Encodable {
        let protocolVersion: UInt32
        let requestID: String
        let ownerUserID: String
        let command: LocalAgentCommand
    }

    private let ownerUserID: String
    private let transport: any LocalAgentFrameTransport
    private let encoder: JSONEncoder
    private let decoder: JSONDecoder

    public init(ownerUserID: String, transport: any LocalAgentFrameTransport) throws {
        guard !ownerUserID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              ownerUserID == ownerUserID.trimmingCharacters(in: .whitespacesAndNewlines)
        else {
            throw NativeLocalAgentIPCError.invalidConfiguration("本地 Agent 用户身份无效")
        }
        self.ownerUserID = ownerUserID
        self.transport = transport
        self.encoder = JSONEncoder.localAgentEncoder()
        self.decoder = JSONDecoder.localAgentDecoder()
    }

    public func send(_ command: LocalAgentCommand) async throws -> LocalAgentResponse {
        let requestID = UUID().uuidString.lowercased()
        let request = Request(
            protocolVersion: localAgentProtocolVersion,
            requestID: requestID,
            ownerUserID: ownerUserID,
            command: command
        )
        let replyData = try await transport.exchange(try encoder.encode(request))
        let reply: LocalAgentIPCReply
        do {
            reply = try decoder.decode(LocalAgentIPCReply.self, from: replyData)
        } catch {
            throw NativeLocalAgentIPCError.invalidResponse
        }
        guard reply.protocolVersion == localAgentProtocolVersion else {
            throw NativeLocalAgentIPCError.protocolMismatch(reply.protocolVersion)
        }
        guard reply.requestID == requestID else {
            throw NativeLocalAgentIPCError.requestMismatch(
                expected: requestID,
                actual: reply.requestID
            )
        }
        if case let .error(error) = reply.response {
            throw NativeLocalAgentIPCError.rejected(error)
        }
        return reply.response
    }

    public func accepted(_ command: LocalAgentCommand) async throws -> String {
        let response = try await send(command)
        guard case let .accepted(operationID) = response else {
            throw unexpected("accepted", response)
        }
        return operationID
    }

    public func run(id: String) async throws -> LocalAgentRunSnapshot {
        let response = try await send(.getRun(runID: id))
        guard case let .run(run) = response else { throw unexpected("run", response) }
        return run
    }

    public func runs(cursor: String? = nil, limit: UInt32 = 100) async throws -> (
        runs: [LocalAgentRunSnapshot], nextCursor: String?
    ) {
        let response = try await send(.listRuns(cursor: cursor, limit: limit))
        guard case let .runs(runs, nextCursor) = response else {
            throw unexpected("runs", response)
        }
        return (runs, nextCursor)
    }

    public func events(after sequence: UInt64, limit: UInt32 = 200) async throws -> (
        events: [LocalAgentUIEvent], nextSequence: UInt64, hasMore: Bool
    ) {
        let response = try await send(.subscribeRunEvents(afterSequence: sequence, limit: limit))
        guard case let .events(events, nextSequence, hasMore) = response else {
            throw unexpected("events", response)
        }
        return (events, nextSequence, hasMore)
    }

    private func unexpected(
        _ expected: String,
        _ response: LocalAgentResponse
    ) -> NativeLocalAgentIPCError {
        .unexpectedResponse(expected: expected, actual: response.typeName)
    }
}

private extension LocalAgentResponse {
    var typeName: String {
        switch self {
        case .accepted: "accepted"
        case .run: "run"
        case .runs: "runs"
        case .events: "events"
        case .storageProfile: "storage_profile"
        case .postgresConnectionTest: "postgres_connection_test"
        case .dataTransfer: "data_transfer"
        case .success: "success"
        case .error: "error"
        }
    }
}

private extension JSONEncoder {
    static func localAgentEncoder() -> JSONEncoder {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }
}

private extension JSONDecoder {
    static func localAgentDecoder() -> JSONDecoder {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .custom { path in
            let source = path.last?.stringValue ?? ""
            let components = source.split(separator: "_")
            guard let first = components.first else { return AnyCodingKey(source) }
            let value = String(first) + components.dropFirst().map { component in
                switch component.lowercased() {
                case "id": "ID"
                case "ids": "IDs"
                case "ok": "OK"
                case "url": "URL"
                default: component.prefix(1).uppercased() + component.dropFirst()
                }
            }.joined()
            return AnyCodingKey(value)
        }
        return decoder
    }
}

private struct AnyCodingKey: CodingKey {
    let stringValue: String
    let intValue: Int? = nil
    init(_ stringValue: String) { self.stringValue = stringValue }
    init?(stringValue: String) { self.init(stringValue) }
    init?(intValue: Int) { return nil }
}
