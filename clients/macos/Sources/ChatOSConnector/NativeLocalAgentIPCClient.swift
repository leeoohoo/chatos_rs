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
    typealias PeerIdentityVerifier = @Sendable (pid_t) throws -> Void

    private let socketPath: String
    private let maximumFrameBytes: Int
    private let ioTimeoutSeconds: Int
    private let peerIdentityVerifier: PeerIdentityVerifier
    private let queue = DispatchQueue(
        label: "com.chatos.local.local-agent-ipc",
        qos: .userInitiated,
        attributes: .concurrent
    )

    public convenience init(
        socketPath: String,
        maximumFrameBytes: Int = 8 * 1024 * 1024,
        ioTimeoutSeconds: Int = 35
    ) throws {
        try self.init(
            socketPath: socketPath,
            maximumFrameBytes: maximumFrameBytes,
            ioTimeoutSeconds: ioTimeoutSeconds,
            peerIdentityVerifier: NativeLocalAgentHostIdentity.validate(processID:)
        )
    }

    init(
        socketPath: String,
        maximumFrameBytes: Int = 8 * 1024 * 1024,
        ioTimeoutSeconds: Int = 35,
        peerIdentityVerifier: @escaping PeerIdentityVerifier
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
        self.peerIdentityVerifier = peerIdentityVerifier
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
        let descriptor = try openVerifiedConnection()
        defer { Darwin.close(descriptor) }

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

    func connectedPeerProcessID() async throws -> pid_t {
        try await withCheckedThrowingContinuation { continuation in
            queue.async { [self] in
                continuation.resume(with: Result {
                    let descriptor = try openVerifiedConnection()
                    defer { Darwin.close(descriptor) }
                    return try peerProcessID(descriptor)
                })
            }
        }
    }

    private func openVerifiedConnection() throws -> Int32 {
        try verifySocketFile()
        let descriptor = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard descriptor >= 0 else {
            throw NativeLocalAgentIPCError.socketUnavailable(errno)
        }
        do {
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
            let processID = try verifyPeer(descriptor)
            do {
                try peerIdentityVerifier(processID)
            } catch {
                throw NativeLocalAgentIPCError.serverIdentityMismatch
            }
            return descriptor
        } catch {
            Darwin.close(descriptor)
            throw error
        }
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

    private func verifyPeer(_ descriptor: Int32) throws -> pid_t {
        var userID: uid_t = 0
        var groupID: gid_t = 0
        guard getpeereid(descriptor, &userID, &groupID) == 0,
              userID == geteuid()
        else {
            throw NativeLocalAgentIPCError.serverIdentityMismatch
        }
        return try peerProcessID(descriptor)
    }

    private func peerProcessID(_ descriptor: Int32) throws -> pid_t {
        var processID: pid_t = 0
        var length = socklen_t(MemoryLayout<pid_t>.size)
        guard getsockopt(
            descriptor,
            SOL_LOCAL,
            LOCAL_PEERPID,
            &processID,
            &length
        ) == 0,
            length == socklen_t(MemoryLayout<pid_t>.size),
            processID > 1
        else {
            throw NativeLocalAgentIPCError.serverIdentityMismatch
        }
        return processID
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

    public let ownerUserID: String
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
        self.encoder = LocalAgentProtocolJSON.encoder()
        self.decoder = LocalAgentProtocolJSON.decoder()
    }

    public func send(_ command: LocalAgentCommand) async throws -> LocalAgentResponse {
        let requestID = UUID().uuidString.lowercased()
        let containsSensitivePayload = command.containsSensitivePayload
        let request = Request(
            protocolVersion: localAgentProtocolVersion,
            requestID: requestID,
            ownerUserID: ownerUserID,
            command: command
        )
        var requestData = try encoder.encode(request)
        defer {
            if containsSensitivePayload {
                requestData.resetBytes(in: 0..<requestData.count)
            }
        }
        let replyData = try await transport.exchange(requestData)
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

    public func updateAccessToken(_ accessToken: String) async throws {
        let response = try await send(.updateAccessToken(accessToken))
        guard case .success = response else {
            throw unexpected("success", response)
        }
    }

    public func createMainChatTurn(
        _ command: LocalAgentCreateMainChatTurn
    ) async throws -> (operationID: String, run: LocalAgentRunSnapshot) {
        let response = try await send(.createMainChatTurn(command))
        guard case let .runCreated(operationID, run) = response else {
            throw unexpected("run_created", response)
        }
        return (operationID, run)
    }

    public func createTask(
        _ command: LocalAgentCreateTask
    ) async throws -> (operationID: String, run: LocalAgentRunSnapshot) {
        let response = try await send(.createTask(command))
        guard case let .runCreated(operationID, run) = response else {
            throw unexpected("run_created", response)
        }
        return (operationID, run)
    }

    public func retryTask(
        _ command: LocalAgentRetryTask
    ) async throws -> (operationID: String, run: LocalAgentRunSnapshot) {
        let response = try await send(.retryTask(command))
        guard case let .runCreated(operationID, run) = response else {
            throw unexpected("run_created", response)
        }
        return (operationID, run)
    }

    public func run(id: String) async throws -> LocalAgentRunSnapshot {
        let response = try await send(.getRun(runID: id))
        guard case let .run(run) = response else { throw unexpected("run", response) }
        return run
    }

    public func runDetail(
        id: String,
        eventLimit: UInt32,
        eventOffset: UInt32
    ) async throws -> LocalAgentRunDetail {
        let response = try await send(
            .getRunDetail(
                runID: id,
                eventLimit: eventLimit,
                eventOffset: eventOffset
            )
        )
        guard case let .runDetail(detail) = response else {
            throw unexpected("run_detail", response)
        }
        return detail
    }

    public func task(id: String) async throws -> LocalAgentTaskSnapshot {
        let response = try await send(.getTask(taskID: id))
        guard case let .task(task) = response else { throw unexpected("task", response) }
        return task
    }

    public func project(id: String) async throws -> LocalAgentProjectSnapshot {
        let response = try await send(.getProject(projectID: id))
        guard case let .project(project) = response else {
            throw unexpected("project", response)
        }
        return project
    }

    public func projects(includeInactive: Bool) async throws -> [LocalAgentProjectSnapshot] {
        var records: [LocalAgentProjectSnapshot] = []
        var cursor: String?
        repeat {
            let response = try await send(
                .listProjects(cursor: cursor, limit: 500, includeInactive: includeInactive)
            )
            guard case let .projects(page, nextCursor) = response else {
                throw unexpected("projects", response)
            }
            records.append(contentsOf: page)
            if let nextCursor, nextCursor == cursor {
                throw NativeLocalAgentIPCError.invalidResponse
            }
            cursor = nextCursor
        } while cursor != nil
        return records.sorted {
            $0.draft.name == $1.draft.name
                ? $0.projectID < $1.projectID
                : $0.draft.name < $1.draft.name
        }
    }

    public func createProject(
        projectID: String,
        draft: LocalAgentProjectDraft
    ) async throws -> LocalAgentProjectSnapshot {
        let response = try await send(.createProject(projectID: projectID, draft: draft))
        guard case let .project(project) = response else {
            throw unexpected("project", response)
        }
        return project
    }

    public func updateProject(
        projectID: String,
        expectedRevision: UInt64,
        draft: LocalAgentProjectDraft,
        status: LocalAgentProjectStatus
    ) async throws -> LocalAgentProjectSnapshot {
        let response = try await send(
            .updateProject(
                projectID: projectID,
                expectedRevision: expectedRevision,
                draft: draft,
                status: status
            )
        )
        guard case let .project(project) = response else {
            throw unexpected("project", response)
        }
        return project
    }

    public func clipboardEntry(id: String) async throws -> LocalAgentClipboardSnapshot {
        let response = try await send(.getClipboard(entryID: id))
        guard case let .clipboard(entry) = response else {
            throw unexpected("clipboard", response)
        }
        return entry
    }

    public func clipboardEntries() async throws -> [LocalAgentClipboardSnapshot] {
        var entries: [LocalAgentClipboardSnapshot] = []
        var cursor: String?
        repeat {
            let response = try await send(.listClipboard(cursor: cursor, limit: 500))
            guard case let .clipboardRecords(page, nextCursor) = response else {
                throw unexpected("clipboard_records", response)
            }
            entries.append(contentsOf: page)
            if let nextCursor, nextCursor == cursor {
                throw NativeLocalAgentIPCError.invalidResponse
            }
            cursor = nextCursor
        } while cursor != nil
        return entries.sorted {
            if $0.isPinned != $1.isPinned { return $0.isPinned }
            if $0.updatedAt != $1.updatedAt { return $0.updatedAt > $1.updatedAt }
            return $0.entryID < $1.entryID
        }
    }

    public func storeClipboardEntry(
        id: String,
        draft: LocalAgentClipboardDraft
    ) async throws -> LocalAgentClipboardMutationResult {
        try await clipboardMutation(.storeClipboard(entryID: id, draft: draft))
    }

    public func setClipboardEntryPinned(
        id: String,
        expectedRevision: UInt64,
        isPinned: Bool
    ) async throws -> LocalAgentClipboardMutationResult {
        try await clipboardMutation(.setClipboardPinned(
            entryID: id,
            expectedRevision: expectedRevision,
            isPinned: isPinned
        ))
    }

    public func deleteClipboardEntry(
        id: String,
        expectedRevision: UInt64
    ) async throws -> LocalAgentClipboardMutationResult {
        try await clipboardMutation(.deleteClipboard(
            entryID: id,
            expectedRevision: expectedRevision
        ))
    }

    private func clipboardMutation(
        _ command: LocalAgentCommand
    ) async throws -> LocalAgentClipboardMutationResult {
        let response = try await send(command)
        guard case let .clipboardMutation(result) = response else {
            throw unexpected("clipboard_mutation", response)
        }
        return result
    }

    public func mediaRecord(id: String) async throws -> LocalAgentMediaSnapshot {
        let response = try await send(.getMedia(recordID: id))
        guard case let .media(record) = response else {
            throw unexpected("media", response)
        }
        return record
    }

    public func mediaRecords() async throws -> [LocalAgentMediaSnapshot] {
        var records: [LocalAgentMediaSnapshot] = []
        var cursor: String?
        repeat {
            let response = try await send(.listMedia(cursor: cursor, limit: 500))
            guard case let .mediaRecords(page, nextCursor) = response else {
                throw unexpected("media_records", response)
            }
            records.append(contentsOf: page)
            if let nextCursor, nextCursor == cursor {
                throw NativeLocalAgentIPCError.invalidResponse
            }
            cursor = nextCursor
        } while cursor != nil
        return records.sorted {
            if $0.createdAt != $1.createdAt { return $0.createdAt > $1.createdAt }
            return $0.recordID < $1.recordID
        }
    }

    public func putMediaRecord(
        id: String,
        expectedRevision: UInt64?,
        draft: LocalAgentMediaDraft
    ) async throws -> LocalAgentMediaMutationResult {
        try await mediaMutation(.putMedia(
            recordID: id,
            expectedRevision: expectedRevision,
            draft: draft
        ))
    }

    public func deleteMediaRecord(
        id: String,
        expectedRevision: UInt64
    ) async throws -> LocalAgentMediaMutationResult {
        try await mediaMutation(.deleteMedia(recordID: id, expectedRevision: expectedRevision))
    }

    private func mediaMutation(
        _ command: LocalAgentCommand
    ) async throws -> LocalAgentMediaMutationResult {
        let response = try await send(command)
        guard case let .mediaMutation(result) = response else {
            throw unexpected("media_mutation", response)
        }
        return result
    }

    public func storyRecord(id: String) async throws -> LocalAgentStorySnapshot {
        let response = try await send(.getStory(recordID: id))
        guard case let .story(record) = response else {
            throw unexpected("story", response)
        }
        return record
    }

    public func storyRecords() async throws -> [LocalAgentStorySnapshot] {
        var records: [LocalAgentStorySnapshot] = []
        var cursor: String?
        repeat {
            let response = try await send(.listStories(cursor: cursor, limit: 500))
            guard case let .storyRecords(page, nextCursor) = response else {
                throw unexpected("story_records", response)
            }
            records.append(contentsOf: page)
            if let nextCursor, nextCursor == cursor {
                throw NativeLocalAgentIPCError.invalidResponse
            }
            cursor = nextCursor
        } while cursor != nil
        return records.sorted {
            if $0.updatedAt != $1.updatedAt { return $0.updatedAt > $1.updatedAt }
            return $0.recordID < $1.recordID
        }
    }

    public func putStoryRecord(
        id: String,
        expectedRevision: UInt64?,
        draft: LocalAgentStoryDraft
    ) async throws -> LocalAgentStorySnapshot {
        let response = try await send(.putStory(
            recordID: id,
            expectedRevision: expectedRevision,
            draft: draft
        ))
        guard case let .story(record) = response else {
            throw unexpected("story", response)
        }
        return record
    }

    public func deleteStoryRecord(id: String, expectedRevision: UInt64) async throws {
        let response = try await send(.deleteStory(
            recordID: id,
            expectedRevision: expectedRevision
        ))
        guard case .success = response else {
            throw unexpected("success", response)
        }
    }

    public func notepadRecord(id: String) async throws -> LocalAgentNotepadSnapshot {
        let response = try await send(.getNotepad(recordID: id))
        guard case let .notepad(record) = response else {
            throw unexpected("notepad", response)
        }
        return record
    }

    public func notepadRecords() async throws -> [LocalAgentNotepadSnapshot] {
        var records: [LocalAgentNotepadSnapshot] = []
        var cursor: String?
        repeat {
            let response = try await send(.listNotepad(cursor: cursor, limit: 500))
            guard case let .notepadRecords(page, nextCursor) = response else {
                throw unexpected("notepad_records", response)
            }
            records.append(contentsOf: page)
            if let nextCursor, nextCursor == cursor {
                throw NativeLocalAgentIPCError.invalidResponse
            }
            cursor = nextCursor
        } while cursor != nil
        return records.sorted {
            if $0.updatedAt != $1.updatedAt { return $0.updatedAt > $1.updatedAt }
            return $0.recordID < $1.recordID
        }
    }

    public func putNotepadRecord(
        id: String,
        expectedRevision: UInt64?,
        draft: LocalAgentNotepadDraft
    ) async throws -> LocalAgentNotepadSnapshot {
        let response = try await send(.putNotepad(
            recordID: id,
            expectedRevision: expectedRevision,
            draft: draft
        ))
        guard case let .notepad(record) = response else {
            throw unexpected("notepad", response)
        }
        return record
    }

    public func deleteNotepadRecord(id: String, expectedRevision: UInt64) async throws {
        let response = try await send(.deleteNotepad(
            recordID: id,
            expectedRevision: expectedRevision
        ))
        guard case .success = response else {
            throw unexpected("success", response)
        }
    }

    public func renameNotepadFolder(_ folder: String, replacement: String) async throws {
        let response = try await send(.renameNotepadFolder(
            folder: folder,
            replacement: replacement
        ))
        guard case .success = response else {
            throw unexpected("success", response)
        }
    }

    public func deleteNotepadFolder(_ folder: String, recursive: Bool) async throws {
        let response = try await send(.deleteNotepadFolder(folder: folder, recursive: recursive))
        guard case .success = response else {
            throw unexpected("success", response)
        }
    }

    public func clientSetting(key: String) async throws -> LocalAgentClientSettingSnapshot {
        let response = try await send(.getClientSetting(key: key))
        guard case let .clientSetting(setting) = response else {
            throw unexpected("client_setting", response)
        }
        return setting
    }

    public func putClientSetting(
        key: String,
        expectedRevision: UInt64?,
        value: LocalAgentJSONValue
    ) async throws -> LocalAgentClientSettingSnapshot {
        let response = try await send(.putClientSetting(
            key: key,
            expectedRevision: expectedRevision,
            value: value
        ))
        guard case let .clientSetting(setting) = response else {
            throw unexpected("client_setting", response)
        }
        return setting
    }

    public func deleteClientSetting(key: String, expectedRevision: UInt64) async throws {
        let response = try await send(.deleteClientSetting(
            key: key,
            expectedRevision: expectedRevision
        ))
        guard case .success = response else {
            throw unexpected("success", response)
        }
    }

    public func appendTerminalHistory(
        recordID: String,
        draft: LocalAgentTerminalHistoryDraft
    ) async throws -> LocalAgentTerminalHistorySnapshot {
        let response = try await send(.appendTerminalHistory(recordID: recordID, draft: draft))
        guard case let .terminalHistory(record) = response else {
            throw unexpected("terminal_history", response)
        }
        return record
    }

    public func terminalHistoryRecords() async throws -> [LocalAgentTerminalHistorySnapshot] {
        var records: [LocalAgentTerminalHistorySnapshot] = []
        var cursor: String?
        repeat {
            let response = try await send(.listTerminalHistory(cursor: cursor, limit: 500))
            guard case let .terminalHistoryRecords(page, nextCursor) = response else {
                throw unexpected("terminal_history_records", response)
            }
            records.append(contentsOf: page)
            if let nextCursor, nextCursor == cursor {
                throw NativeLocalAgentIPCError.invalidResponse
            }
            cursor = nextCursor
        } while cursor != nil
        return records.sorted {
            if $0.createdAt != $1.createdAt { return $0.createdAt > $1.createdAt }
            return $0.recordID > $1.recordID
        }
    }

    public func deleteTerminalHistory(
        recordID: String,
        expectedRevision: UInt64
    ) async throws {
        let response = try await send(.deleteTerminalHistory(
            recordID: recordID,
            expectedRevision: expectedRevision
        ))
        guard case .success = response else {
            throw unexpected("success", response)
        }
    }

    public func clearTerminalHistory() async throws {
        let response = try await send(.clearTerminalHistory)
        guard case .success = response else {
            throw unexpected("success", response)
        }
    }

    public func taskGraph(
        sourceThreadID: String,
        sourceTurnID: String
    ) async throws -> LocalAgentTaskGraphSnapshot {
        let response = try await send(
            .getTaskGraph(sourceThreadID: sourceThreadID, sourceTurnID: sourceTurnID)
        )
        guard case let .taskGraph(graph) = response else {
            throw unexpected("task_graph", response)
        }
        return graph
    }

    public func taskRunDetail(
        taskID: String,
        runID: String,
        eventLimit: UInt32,
        eventOffset: UInt32
    ) async throws -> LocalAgentTaskRunDetail {
        let response = try await send(
            .getTaskRunDetail(
                taskID: taskID,
                runID: runID,
                eventLimit: eventLimit,
                eventOffset: eventOffset
            )
        )
        guard case let .taskRunDetail(detail) = response else {
            throw unexpected("task_run_detail", response)
        }
        return detail
    }

    public func mainChatRunBinding(
        runID: String
    ) async throws -> LocalAgentMainChatRunBinding {
        let response = try await send(.getMainChatRunBinding(runID: runID))
        guard case let .mainChatRunBinding(binding) = response else {
            throw unexpected("main_chat_run_binding", response)
        }
        return binding
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

    public func tasks(cursor: String? = nil, limit: UInt32 = 100) async throws -> (
        tasks: [LocalAgentTaskSnapshot], nextCursor: String?
    ) {
        let response = try await send(.listTasks(cursor: cursor, limit: limit))
        guard case let .tasks(tasks, nextCursor) = response else {
            throw unexpected("tasks", response)
        }
        return (tasks, nextCursor)
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

    public func uiEventCursor() async throws -> UInt64 {
        let response = try await send(.getUIEventCursor)
        guard case let .uiEventCursor(sequence) = response else {
            throw unexpected("ui_event_cursor", response)
        }
        return sequence
    }

    @discardableResult
    public func acknowledgeUIEvents(through sequence: UInt64) async throws -> UInt64 {
        let response = try await send(.acknowledgeUIEvents(throughSequence: sequence))
        guard case let .uiEventCursor(acknowledged) = response else {
            throw unexpected("ui_event_cursor", response)
        }
        return acknowledged
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
        case .runCreated: "run_created"
        case .run: "run"
        case .runDetail: "run_detail"
        case .task: "task"
        case .taskGraph: "task_graph"
        case .taskRunDetail: "task_run_detail"
        case .mainChatRunBinding: "main_chat_run_binding"
        case .runs: "runs"
        case .tasks: "tasks"
        case .project: "project"
        case .projects: "projects"
        case .clipboard: "clipboard"
        case .clipboardRecords: "clipboard_records"
        case .clipboardMutation: "clipboard_mutation"
        case .media: "media"
        case .mediaRecords: "media_records"
        case .mediaMutation: "media_mutation"
        case .story: "story"
        case .storyRecords: "story_records"
        case .notepad: "notepad"
        case .notepadRecords: "notepad_records"
        case .clientSetting: "client_setting"
        case .terminalHistory: "terminal_history"
        case .terminalHistoryRecords: "terminal_history_records"
        case .events: "events"
        case .uiEventCursor: "ui_event_cursor"
        case .storageProfile: "storage_profile"
        case .postgresConnectionTest: "postgres_connection_test"
        case .dataTransfer: "data_transfer"
        case .success: "success"
        case .error: "error"
        }
    }
}
