// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation
import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerCore
import ChatOSCodeReadWorkerProtocol
import Darwin

func waitForTestFile(at url: URL) async throws {
    for _ in 0..<100 {
        if FileManager.default.fileExists(atPath: url.path) {
            return
        }
        try await Task.sleep(for: .milliseconds(10))
    }
    throw CocoaError(.fileReadNoSuchFile)
}

struct ConnectorCoreCodeReadWorker: CodeReadWorkerBinaryExecuting {
    func execute(
        operation: String,
        arguments: [String: CodeReadWorkerJSONValue],
        projectRoot: URL,
        requestCWD: String?,
        defaultToolRoot: String?,
        ripgrepURL: URL?
    ) async throws -> CodeReadWorkerJSONValue {
        var status = stat()
        guard projectRoot.path.withCString({ Darwin.lstat($0, &status) }) == 0 else {
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        let ripgrepIdentity: CodeReadWorkerFileIdentity?
        if let ripgrepURL {
            var ripgrepStatus = stat()
            guard ripgrepURL.path.withCString({ Darwin.lstat($0, &ripgrepStatus) }) == 0 else {
                throw CodeReadWorkerProcessClientError.invalidResponse
            }
            ripgrepIdentity = .init(
                device: UInt64(ripgrepStatus.st_dev),
                inode: UInt64(ripgrepStatus.st_ino)
            )
        } else {
            ripgrepIdentity = nil
        }
        do {
            return try ChatOSCodeReadWorkerEngine.execute(.init(
                protocolVersion: ChatOSCodeReadWorkerProtocol.maximumVersion,
                requestID: UUID().uuidString,
                helperInstanceID: UUID().uuidString,
                deadlineUnixMilliseconds: Int64((Date().timeIntervalSince1970 + 10) * 1_000),
                operation: operation,
                arguments: arguments,
                projectRootPath: projectRoot.path,
                projectRootIdentity: .init(
                    device: UInt64(status.st_dev),
                    inode: UInt64(status.st_ino)
                ),
                requestCWD: requestCWD,
                defaultToolRoot: defaultToolRoot,
                ripgrepPath: ripgrepURL?.path,
                ripgrepIdentity: ripgrepIdentity
            ))
        } catch let error as CodeReadWorkerCoreError {
            throw CodeReadWorkerProcessClientError.requestRejected(
                code: error.payload.code,
                message: error.payload.message
            )
        }
    }


    func execute(
        operation: String,
        arguments: [String: CodeReadWorkerJSONValue],
        binaryPayloads: [Data],
        projectRoot: URL,
        requestCWD: String?,
        defaultToolRoot: String?,
        ripgrepURL: URL?
    ) async throws -> CodeReadWorkerJSONValue {
        var status = stat()
        guard projectRoot.path.withCString({ Darwin.lstat($0, &status) }) == 0 else {
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        let total = binaryPayloads.reduce(0) { $0 + $1.count }
        let request = CodeReadWorkerRequest(
            protocolVersion: ChatOSCodeReadWorkerProtocol.maximumVersion,
            requestID: UUID().uuidString,
            helperInstanceID: UUID().uuidString,
            deadlineUnixMilliseconds: Int64((Date().timeIntervalSince1970 + 10) * 1_000),
            operation: operation,
            arguments: arguments,
            projectRootPath: projectRoot.path,
            projectRootIdentity: .init(
                device: UInt64(status.st_dev),
                inode: UInt64(status.st_ino)
            ),
            requestCWD: requestCWD,
            defaultToolRoot: defaultToolRoot,
            ripgrepPath: nil,
            ripgrepIdentity: nil,
            binaryPayloadByteCount: UInt64(total)
        )
        var payloadIndex = 0
        var payloadOffset = 0
        do {
            return try ChatOSCodeReadWorkerEngine.executeStreaming(request) { maximumCount in
                while payloadIndex < binaryPayloads.count,
                      payloadOffset == binaryPayloads[payloadIndex].count {
                    payloadIndex += 1
                    payloadOffset = 0
                }
                guard payloadIndex < binaryPayloads.count else { return Data() }
                let payload = binaryPayloads[payloadIndex]
                let end = min(payload.count, payloadOffset + maximumCount)
                defer { payloadOffset = end }
                return payload.subdata(in: payloadOffset..<end)
            }
        } catch let error as CodeReadWorkerCoreError {
            throw CodeReadWorkerProcessClientError.requestRejected(
                code: error.payload.code,
                message: error.payload.message
            )
        }
    }
}
