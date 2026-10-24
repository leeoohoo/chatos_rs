import ChatOSCodeReadWorkerProtocol
import CryptoKit
import Darwin
import Foundation

struct AttachmentWorkerOperations {
    private static let maximumAttachmentBytes = 20 * 1_024 * 1_024
    private static let maximumReadBytes = 64 * 1_024
    private static let maximumExactJSONInteger = 9_007_199_254_740_991.0

    let root: URL
    let rootIdentity: CodeReadWorkerFileIdentity
    let deadlineUnixMilliseconds: Int64

    func execute(
        operation: String,
        arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerJSONValue {
        let authorization = try authorization(arguments)
        switch operation {
        case "attachment_resolve_v1":
            return try resolve(authorization, arguments: arguments)
        case "attachment_verify_file_v1":
            return try verifyFile(authorization)
        case "attachment_materialize_v1":
            return try materialize(authorization, arguments: arguments)
        default:
            throw CodeReadWorkerCoreError.unsupportedTool(operation)
        }
    }

    private func resolve(
        _ authorization: Authorization,
        arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerJSONValue {
        let offset = try requiredUInt64(arguments, "offset")
        let limit = Int(try requiredUInt64(arguments, "limit"))
        guard (1...Self.maximumReadBytes).contains(limit), offset <= authorization.byteSize else {
            throw CodeReadWorkerCoreError.attachmentInvalid
        }
        let descriptor = try openAttachment(authorization)
        defer { _ = Darwin.close(descriptor) }
        let expectedCount = min(limit, Int(authorization.byteSize - offset))
        let verified = try hashAndCapture(
            descriptor: descriptor,
            expected: authorization,
            offset: offset,
            count: expectedCount
        )
        let end = offset + UInt64(verified.capture.count)
        let text = String(data: verified.capture, encoding: .utf8)
        return .object([
            "display_name": .string(authorization.displayName),
            "media_type": .string(authorization.mediaType),
            "byte_size": .number(Double(authorization.byteSize)),
            "sha256": .string(authorization.sha256),
            "offset": .number(Double(offset)),
            "next_offset": end < authorization.byteSize ? .number(Double(end)) : .null,
            "encoding": .string(text == nil ? "base64" : "utf-8"),
            "content": .string(text ?? verified.capture.base64EncodedString()),
        ])
    }

    private func verifyFile(_ authorization: Authorization) throws -> CodeReadWorkerJSONValue {
        let descriptor = try openAttachment(authorization)
        defer { _ = Darwin.close(descriptor) }
        let verified = try hashAndCapture(
            descriptor: descriptor,
            expected: authorization,
            offset: 0,
            count: 0
        )
        return .object([
            "relative_path": .string(authorization.relativePath),
            "device": .string(String(verified.status.st_dev)),
            "inode": .string(String(verified.status.st_ino)),
            "byte_count": .number(Double(verified.status.st_size)),
        ])
    }

    private func materialize(
        _ authorization: Authorization,
        arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerJSONValue {
        let source = try openAttachment(authorization)
        defer { _ = Darwin.close(source) }
        _ = try hashAndCapture(
            descriptor: source,
            expected: authorization,
            offset: 0,
            count: 0
        )
        let destinationPath = try requiredString(arguments, "destination_root_path")
        let destinationDevice = try requiredUInt64String(arguments, "destination_root_device")
        let destinationInode = try requiredUInt64String(arguments, "destination_root_inode")
        let destination = destinationPath.withCString {
            Darwin.open($0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard destination >= 0 else {
            throw CodeReadWorkerCoreError.attachmentWorkspaceUnavailable
        }
        defer { _ = Darwin.close(destination) }
        var destinationStatus = stat()
        guard Darwin.fstat(destination, &destinationStatus) == 0,
              UInt64(destinationStatus.st_dev) == destinationDevice,
              UInt64(destinationStatus.st_ino) == destinationInode else {
            throw CodeReadWorkerCoreError.attachmentWorkspaceUnavailable
        }

        let components = [
            ".chatos-attachments",
            hash(Data((authorization.ownerUserID + "\n" + authorization.conversationID).utf8)),
            hash(Data(("local-attachment:" + authorization.token).utf8)),
        ]
        let directory = try createDirectoryChain(components, from: destination)
        defer { _ = Darwin.close(directory) }
        let fileExtension = safeExtension(authorization.displayName)
        let name = "source" + (fileExtension.map { "." + $0 } ?? "")
        let temporaryName = ".import-" + UUID().uuidString.lowercased()
        let temporary = temporaryName.withCString {
            Darwin.openat(
                directory,
                $0,
                O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC,
                0o600
            )
        }
        guard temporary >= 0 else {
            throw CodeReadWorkerCoreError.attachmentWorkspaceUnavailable
        }
        defer {
            _ = Darwin.close(temporary)
            temporaryName.withCString { _ = Darwin.unlinkat(directory, $0, 0) }
        }

        try copyAndVerify(source: source, destination: temporary, expected: authorization)
        guard Darwin.fsync(temporary) == 0 else {
            throw CodeReadWorkerCoreError.attachmentWorkspaceUnavailable
        }
        let linkResult = temporaryName.withCString { temporaryPointer in
            name.withCString { namePointer in
                Darwin.linkat(directory, temporaryPointer, directory, namePointer, 0)
            }
        }
        if linkResult != 0, errno != EEXIST {
            throw CodeReadWorkerCoreError.attachmentWorkspaceUnavailable
        }
        let installed = name.withCString {
            Darwin.openat(directory, $0, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        }
        guard installed >= 0 else {
            throw CodeReadWorkerCoreError.attachmentWorkspaceConflict
        }
        defer { _ = Darwin.close(installed) }
        do {
            _ = try hashAndCapture(
                descriptor: installed,
                expected: authorization,
                offset: 0,
                count: 0
            )
        } catch {
            throw CodeReadWorkerCoreError.attachmentWorkspaceConflict
        }
        return .object([
            "relative_path": .string((components + [name]).joined(separator: "/")),
        ])
    }

    private func authorization(
        _ arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> Authorization {
        let ownerUserID = try requiredString(arguments, "owner_user_id")
        let conversationID = try requiredString(arguments, "conversation_id")
        let token = try requiredString(arguments, "token")
        let displayName = try requiredString(arguments, "display_name")
        let mediaType = try requiredString(arguments, "media_type")
        let byteSize = try requiredUInt64(arguments, "byte_size")
        let sha256 = try requiredString(arguments, "sha256").lowercased()
        guard !ownerUserID.isEmpty, !conversationID.isEmpty,
              let uuid = UUID(uuidString: token), uuid.uuidString.lowercased() == token else {
            throw CodeReadWorkerCoreError.attachmentReferenceInvalid
        }
        guard byteSize > 0, byteSize <= UInt64(Self.maximumAttachmentBytes),
              sha256.count == 64,
              sha256.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }) else {
            throw CodeReadWorkerCoreError.attachmentInvalid
        }
        return .init(
            ownerUserID: ownerUserID,
            conversationID: conversationID,
            token: token,
            displayName: displayName,
            mediaType: mediaType,
            byteSize: byteSize,
            sha256: sha256
        )
    }

    private func openAttachment(_ authorization: Authorization) throws -> Int32 {
        let rootDescriptor = try openRoot()
        defer { _ = Darwin.close(rootDescriptor) }
        let components = [
            hash(Data(authorization.ownerUserID.utf8)),
            hash(Data(authorization.conversationID.utf8)),
        ]
        var current = rootDescriptor
        var ownsCurrent = false
        do {
            for component in components {
                let next = component.withCString {
                    Darwin.openat(current, $0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
                }
                guard next >= 0 else { throw CodeReadWorkerCoreError.attachmentInvalid }
                if ownsCurrent { _ = Darwin.close(current) }
                current = next
                ownsCurrent = true
            }
            let file = authorization.token.withCString {
                Darwin.openat(current, $0, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
            }
            guard file >= 0 else { throw CodeReadWorkerCoreError.attachmentInvalid }
            if ownsCurrent { _ = Darwin.close(current) }
            var status = stat()
            guard Darwin.fstat(file, &status) == 0,
                  (status.st_mode & S_IFMT) == S_IFREG,
                  status.st_size == Int64(authorization.byteSize) else {
                _ = Darwin.close(file)
                throw CodeReadWorkerCoreError.attachmentInvalid
            }
            return file
        } catch {
            if ownsCurrent { _ = Darwin.close(current) }
            throw error
        }
    }

    private func openRoot() throws -> Int32 {
        let descriptor = root.path.withCString {
            Darwin.open($0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw CodeReadWorkerCoreError.attachmentInvalid }
        var status = stat()
        guard Darwin.fstat(descriptor, &status) == 0,
              UInt64(status.st_dev) == rootIdentity.device,
              UInt64(status.st_ino) == rootIdentity.inode else {
            _ = Darwin.close(descriptor)
            throw CodeReadWorkerCoreError.invalidRequest("附件根目录身份已经变化")
        }
        return descriptor
    }

    private func hashAndCapture(
        descriptor: Int32,
        expected: Authorization,
        offset: UInt64,
        count: Int
    ) throws -> (status: stat, capture: Data) {
        var before = stat()
        guard Darwin.fstat(descriptor, &before) == 0,
              (before.st_mode & S_IFMT) == S_IFREG,
              before.st_size == Int64(expected.byteSize),
              Darwin.lseek(descriptor, 0, SEEK_SET) == 0 else {
            throw CodeReadWorkerCoreError.attachmentIntegrityMismatch
        }
        var hasher = SHA256()
        var total: UInt64 = 0
        var captured = Data()
        captured.reserveCapacity(count)
        var buffer = [UInt8](repeating: 0, count: 64 * 1_024)
        while true {
            try checkDeadline()
            let readCount = Darwin.read(descriptor, &buffer, buffer.count)
            if readCount < 0, errno == EINTR { continue }
            guard readCount >= 0 else {
                throw CodeReadWorkerCoreError.attachmentIntegrityMismatch
            }
            if readCount == 0 { break }
            let chunkStart = total
            let chunkEnd = total + UInt64(readCount)
            hasher.update(data: Data(buffer[0..<readCount]))
            let wantedStart = offset
            let wantedEnd = offset + UInt64(count)
            if count > 0, chunkEnd > wantedStart, chunkStart < wantedEnd {
                let localStart = Int(max(wantedStart, chunkStart) - chunkStart)
                let localEnd = Int(min(wantedEnd, chunkEnd) - chunkStart)
                captured.append(contentsOf: buffer[localStart..<localEnd])
            }
            total = chunkEnd
            guard total <= UInt64(Self.maximumAttachmentBytes) else {
                throw CodeReadWorkerCoreError.attachmentIntegrityMismatch
            }
        }
        var after = stat()
        let digest = hasher.finalize().map { String(format: "%02x", $0) }.joined()
        guard Darwin.fstat(descriptor, &after) == 0,
              sameIdentity(before, after), total == expected.byteSize,
              captured.count == count, digest == expected.sha256 else {
            throw CodeReadWorkerCoreError.attachmentIntegrityMismatch
        }
        return (after, captured)
    }

    private func copyAndVerify(
        source: Int32,
        destination: Int32,
        expected: Authorization
    ) throws {
        var before = stat()
        guard Darwin.fstat(source, &before) == 0,
              Darwin.lseek(source, 0, SEEK_SET) == 0 else {
            throw CodeReadWorkerCoreError.attachmentIntegrityMismatch
        }
        var hasher = SHA256()
        var total: UInt64 = 0
        var buffer = [UInt8](repeating: 0, count: 64 * 1_024)
        while true {
            try checkDeadline()
            let readCount = Darwin.read(source, &buffer, buffer.count)
            if readCount < 0, errno == EINTR { continue }
            guard readCount >= 0 else {
                throw CodeReadWorkerCoreError.attachmentIntegrityMismatch
            }
            if readCount == 0 { break }
            let data = Data(buffer[0..<readCount])
            hasher.update(data: data)
            try writeAll(data, to: destination)
            total += UInt64(readCount)
            guard total <= UInt64(Self.maximumAttachmentBytes) else {
                throw CodeReadWorkerCoreError.attachmentIntegrityMismatch
            }
        }
        var after = stat()
        let digest = hasher.finalize().map { String(format: "%02x", $0) }.joined()
        guard Darwin.fstat(source, &after) == 0, sameIdentity(before, after),
              total == expected.byteSize, digest == expected.sha256 else {
            throw CodeReadWorkerCoreError.attachmentIntegrityMismatch
        }
    }

    private func createDirectoryChain(_ components: [String], from root: Int32) throws -> Int32 {
        var current = Darwin.dup(root)
        guard current >= 0 else {
            throw CodeReadWorkerCoreError.attachmentWorkspaceUnavailable
        }
        do {
            for component in components {
                try checkDeadline()
                if component.withCString({ Darwin.mkdirat(current, $0, 0o700) }) != 0,
                   errno != EEXIST {
                    throw CodeReadWorkerCoreError.attachmentWorkspaceUnavailable
                }
                let next = component.withCString {
                    Darwin.openat(current, $0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
                }
                guard next >= 0 else {
                    throw CodeReadWorkerCoreError.attachmentWorkspaceUnavailable
                }
                _ = Darwin.close(current)
                current = next
            }
            return current
        } catch {
            _ = Darwin.close(current)
            throw error
        }
    }

    private func writeAll(_ data: Data, to descriptor: Int32) throws {
        try data.withUnsafeBytes { bytes in
            guard let base = bytes.baseAddress else { return }
            var offset = 0
            while offset < bytes.count {
                try checkDeadline()
                let count = Darwin.write(descriptor, base.advanced(by: offset), bytes.count - offset)
                if count < 0, errno == EINTR { continue }
                guard count > 0 else {
                    throw CodeReadWorkerCoreError.attachmentWorkspaceUnavailable
                }
                offset += count
            }
        }
    }

    private func sameIdentity(_ lhs: stat, _ rhs: stat) -> Bool {
        lhs.st_dev == rhs.st_dev && lhs.st_ino == rhs.st_ino
            && lhs.st_size == rhs.st_size
            && lhs.st_mtimespec.tv_sec == rhs.st_mtimespec.tv_sec
            && lhs.st_mtimespec.tv_nsec == rhs.st_mtimespec.tv_nsec
            && lhs.st_ctimespec.tv_sec == rhs.st_ctimespec.tv_sec
            && lhs.st_ctimespec.tv_nsec == rhs.st_ctimespec.tv_nsec
    }

    private func safeExtension(_ displayName: String) -> String? {
        let value = (displayName as NSString).pathExtension.lowercased()
        guard !value.isEmpty, value.utf8.count <= 12,
              value.utf8.allSatisfy({ (97...122).contains($0) || (48...57).contains($0) }) else {
            return nil
        }
        return value
    }

    private func requiredString(
        _ arguments: [String: CodeReadWorkerJSONValue],
        _ key: String
    ) throws -> String {
        guard case let .string(value)? = arguments[key] else {
            throw CodeReadWorkerCoreError.invalidRequest("缺少字符串字段：\(key)")
        }
        return value
    }

    private func requiredUInt64(
        _ arguments: [String: CodeReadWorkerJSONValue],
        _ key: String
    ) throws -> UInt64 {
        guard case let .number(value)? = arguments[key], value.isFinite,
              value >= 0, value.rounded() == value,
              value <= Self.maximumExactJSONInteger else {
            throw CodeReadWorkerCoreError.invalidRequest("缺少无符号整数字段：\(key)")
        }
        return UInt64(value)
    }

    private func requiredUInt64String(
        _ arguments: [String: CodeReadWorkerJSONValue],
        _ key: String
    ) throws -> UInt64 {
        guard let value = try? requiredString(arguments, key), let parsed = UInt64(value) else {
            throw CodeReadWorkerCoreError.invalidRequest("缺少无符号整数字符串字段：\(key)")
        }
        return parsed
    }

    private func checkDeadline() throws {
        guard Int64(Date().timeIntervalSince1970 * 1_000) <= deadlineUnixMilliseconds else {
            throw CodeReadWorkerCoreError.deadlineExceeded
        }
    }

    private func hash(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    private struct Authorization {
        let ownerUserID: String
        let conversationID: String
        let token: String
        let displayName: String
        let mediaType: String
        let byteSize: UInt64
        let sha256: String

        var relativePath: String {
            [
                AttachmentWorkerOperations.hashStatic(Data(ownerUserID.utf8)),
                AttachmentWorkerOperations.hashStatic(Data(conversationID.utf8)),
                token,
            ].joined(separator: "/")
        }
    }

    private static func hashStatic(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}
