// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSMacSecurity
import Darwin
import Foundation
import OSLog
import Security

public enum MacOSKeychainBrokerError: Error, Equatable, Sendable {
    case unavailable
    case invalidRequest
    case invalidResponse
    case status(OSStatus)
}

extension MacOSKeychainBrokerError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .unavailable:
            "ChatOS Keychain Broker 不可用，请在 Console 中查看 KeychainBrokerClient 日志。"
        case .invalidRequest:
            "ChatOS 拒绝了无效的钥匙串请求。"
        case .invalidResponse:
            "ChatOS Keychain Broker 返回了无效响应，请在 Console 中查看关联请求日志。"
        case let .status(status):
            "macOS 钥匙串访问失败（\(status)：\(Self.statusMessage(status))）。"
        }
    }

    private static func statusMessage(_ status: OSStatus) -> String {
        (SecCopyErrorMessageString(status, nil) as String?) ?? "未知 Security.framework 错误"
    }
}

public struct MacOSKeychainBrokerClient: Sendable {
    private struct Request: Encodable {
        let requestID: String
        let operation: String
        let service: String
        let account: String
        let valueBase64: String?
    }

    private struct Response: Decodable {
        let status: Int32
        let valueBase64: String?
        let phase: String
    }

    private static let logger = Logger(
        subsystem: "com.chatos.swift-client",
        category: "KeychainBrokerClient"
    )

    private let executableURL: URL
    private let validatesProductionIdentity: Bool

    public init(executableURL: URL? = nil) {
        if let executableURL {
            self.executableURL = executableURL
            validatesProductionIdentity = false
        } else {
            let resolved = Self.defaultExecutable()
            self.executableURL = resolved.url
            validatesProductionIdentity = resolved.validatesProductionIdentity
        }
    }

    public func load(service: String, account: String) throws -> Data? {
        let response = try call(
            request(operation: "load", service: service, account: account, valueBase64: nil)
        )
        if response.status == errSecItemNotFound { return nil }
        guard response.status == errSecSuccess,
              let encoded = response.valueBase64,
              encoded.utf8.count <= 96 * 1_024,
              let value = Data(base64Encoded: encoded),
              value.count <= 64 * 1_024
        else {
            if response.status != errSecSuccess {
                throw MacOSKeychainBrokerError.status(response.status)
            }
            throw MacOSKeychainBrokerError.invalidResponse
        }
        return value
    }

    public func save(_ value: Data, service: String, account: String) throws {
        guard !value.isEmpty, value.count <= 64 * 1_024 else {
            throw MacOSKeychainBrokerError.invalidRequest
        }
        let response = try call(
            request(
                operation: "save",
                service: service,
                account: account,
                valueBase64: value.base64EncodedString()
            )
        )
        guard response.status == errSecSuccess else {
            throw MacOSKeychainBrokerError.status(response.status)
        }
    }

    public func delete(service: String, account: String) throws {
        let response = try call(
            request(operation: "delete", service: service, account: account, valueBase64: nil)
        )
        guard response.status == errSecSuccess || response.status == errSecItemNotFound else {
            throw MacOSKeychainBrokerError.status(response.status)
        }
    }

    private func request(
        operation: String,
        service: String,
        account: String,
        valueBase64: String?
    ) -> Request {
        Request(
            requestID: UUID().uuidString,
            operation: operation,
            service: service,
            account: account,
            valueBase64: valueBase64
        )
    }

    private func call(_ request: Request) throws -> Response {
        let purpose = Self.credentialPurpose(for: request.service)
        Self.logger.info(
            "request=\(request.requestID, privacy: .public) operation=\(request.operation, privacy: .public) purpose=\(purpose, privacy: .public) phase=prepare"
        )
        guard executableURL.isFileURL,
              FileManager.default.isExecutableFile(atPath: executableURL.path),
              let requestData = try? JSONEncoder().encode(request),
              !requestData.isEmpty,
              requestData.count <= 128 * 1_024
        else {
            Self.logger.error(
                "request=\(request.requestID, privacy: .public) operation=\(request.operation, privacy: .public) purpose=\(purpose, privacy: .public) phase=prepare result=unavailable"
            )
            throw MacOSKeychainBrokerError.unavailable
        }
        if validatesProductionIdentity {
            guard Self.isTrustedProductionBroker(at: executableURL) else {
                Self.logger.error(
                    "request=\(request.requestID, privacy: .public) operation=\(request.operation, privacy: .public) purpose=\(purpose, privacy: .public) phase=identity-validation result=rejected"
                )
                throw MacOSKeychainBrokerError.unavailable
            }
            Self.logger.debug(
                "request=\(request.requestID, privacy: .public) operation=\(request.operation, privacy: .public) purpose=\(purpose, privacy: .public) phase=identity-validation result=accepted"
            )
        }

        let process = Process()
        let input = Pipe()
        let output = Pipe()
        let errors = Pipe()
        process.executableURL = executableURL
        process.arguments = []
        process.environment = ["LANG": "en_US.UTF-8"]
        process.currentDirectoryURL = URL(fileURLWithPath: "/", isDirectory: true)
        process.standardInput = input
        process.standardOutput = output
        process.standardError = errors
        do {
            try process.run()
        } catch {
            Self.logger.error(
                "request=\(request.requestID, privacy: .public) operation=\(request.operation, privacy: .public) purpose=\(purpose, privacy: .public) phase=launch result=failed error=\(String(describing: error), privacy: .public)"
            )
            throw MacOSKeychainBrokerError.unavailable
        }
        Self.logger.debug(
            "request=\(request.requestID, privacy: .public) operation=\(request.operation, privacy: .public) purpose=\(purpose, privacy: .public) phase=launch result=started pid=\(process.processIdentifier, privacy: .public)"
        )
        input.fileHandleForReading.closeFile()
        input.fileHandleForWriting.write(requestData)
        input.fileHandleForWriting.closeFile()
        let responseData = output.fileHandleForReading.readDataToEndOfFile()
        let errorData = errors.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0, errorData.isEmpty else {
            Self.logger.error(
                "request=\(request.requestID, privacy: .public) operation=\(request.operation, privacy: .public) purpose=\(purpose, privacy: .public) phase=exit result=failed termination=\(process.terminationStatus, privacy: .public) stderrBytes=\(errorData.count, privacy: .public) responseBytes=\(responseData.count, privacy: .public)"
            )
            throw MacOSKeychainBrokerError.invalidResponse
        }
        guard !responseData.isEmpty,
              responseData.count <= 128 * 1_024,
              let response = try? JSONDecoder().decode(Response.self, from: responseData)
        else {
            Self.logger.error(
                "request=\(request.requestID, privacy: .public) operation=\(request.operation, privacy: .public) purpose=\(purpose, privacy: .public) phase=decode result=invalid-response responseBytes=\(responseData.count, privacy: .public)"
            )
            throw MacOSKeychainBrokerError.invalidResponse
        }
        let statusMessage = Self.statusMessage(response.status)
        if response.status == errSecSuccess || response.status == errSecItemNotFound {
            Self.logger.info(
                "request=\(request.requestID, privacy: .public) operation=\(request.operation, privacy: .public) purpose=\(purpose, privacy: .public) phase=\(response.phase, privacy: .public) status=\(response.status, privacy: .public) message=\(statusMessage, privacy: .public)"
            )
        } else {
            Self.logger.error(
                "request=\(request.requestID, privacy: .public) operation=\(request.operation, privacy: .public) purpose=\(purpose, privacy: .public) phase=\(response.phase, privacy: .public) status=\(response.status, privacy: .public) message=\(statusMessage, privacy: .public)"
            )
        }
        return response
    }

    private static func defaultExecutable() -> (
        url: URL,
        validatesProductionIdentity: Bool
    ) {
        let bundled = Bundle.main.bundleURL
            .appendingPathComponent("Contents", isDirectory: true)
            .appendingPathComponent("MacOS", isDirectory: true)
            .appendingPathComponent("chatos_keychain_broker", isDirectory: false)
        if FileManager.default.isExecutableFile(atPath: bundled.path) {
            let installed = stableExecutableURL()
            do {
                let executable = try installStableBroker(from: bundled, to: installed)
                logger.info("phase=stable-install result=ready")
                return (executable, true)
            } catch {
                logger.error(
                    "phase=stable-install result=failed error=\(String(describing: error), privacy: .public)"
                )
                return (installed, true)
            }
        }

        var directory = Bundle.main.executableURL?.deletingLastPathComponent()
        for _ in 0..<8 {
            guard let current = directory else { break }
            let candidate = current.appendingPathComponent("ChatOSKeychainBroker")
            if FileManager.default.isExecutableFile(atPath: candidate.path) {
                logger.info("phase=broker-resolution result=development-executable")
                return (candidate, false)
            }
            directory = current.deletingLastPathComponent()
        }
        logger.error("phase=broker-resolution result=missing")
        return (stableExecutableURL(), true)
    }

    private static func stableExecutableURL() -> URL {
        let support = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first ?? FileManager.default.homeDirectoryForCurrentUser
        return support
            .appendingPathComponent("ChatOSSwift", isDirectory: true)
            .appendingPathComponent("Security", isDirectory: true)
            .appendingPathComponent("KeychainBrokerV3", isDirectory: true)
            .appendingPathComponent("chatos_keychain_broker", isDirectory: false)
    }

    private static func installStableBroker(
        from bundled: URL,
        to destination: URL
    ) throws -> URL {
        let fileManager = FileManager.default
        let directory = destination.deletingLastPathComponent()
        try fileManager.createDirectory(
            at: directory,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        try fileManager.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: directory.path
        )
        guard secureDirectory(at: directory) else {
            throw MacOSKeychainBrokerError.unavailable
        }

        if fileManager.fileExists(atPath: destination.path) {
            guard isTrustedProductionBroker(at: destination) else {
                logger.error("phase=stable-install-existing-identity result=rejected")
                throw MacOSKeychainBrokerError.unavailable
            }
            logger.debug("phase=stable-install result=existing-pinned-broker")
            return destination
        }

        guard isTrustedProductionBroker(at: bundled) else {
            logger.error("phase=bundled-broker-identity result=rejected")
            throw MacOSKeychainBrokerError.unavailable
        }
        let temporary = directory
            .appendingPathComponent(".install-\(UUID().uuidString)", isDirectory: false)
        defer { try? fileManager.removeItem(at: temporary) }
        try fileManager.copyItem(at: bundled, to: temporary)
        try fileManager.setAttributes(
            [.posixPermissions: 0o500],
            ofItemAtPath: temporary.path
        )
        guard isTrustedProductionBroker(at: temporary) else {
            logger.error("phase=temporary-broker-identity result=rejected")
            throw MacOSKeychainBrokerError.unavailable
        }
        do {
            try fileManager.moveItem(at: temporary, to: destination)
        } catch {
            guard fileManager.fileExists(atPath: destination.path),
                  isTrustedProductionBroker(at: destination) else {
                throw error
            }
        }
        guard isTrustedProductionBroker(at: destination) else {
            logger.error("phase=installed-broker-identity result=rejected")
            throw MacOSKeychainBrokerError.unavailable
        }
        logger.info("phase=stable-install result=installed")
        return destination
    }

    private static func isTrustedProductionBroker(at url: URL) -> Bool {
        guard secureRegularFile(at: url) else {
            logger.error("phase=identity-validation check=secure-file result=rejected")
            return false
        }
        guard let appIdentity = MacOSCodeSigning.identityForCurrentProcess() else {
            logger.error("phase=identity-validation check=app-signature result=missing")
            return false
        }
        guard appIdentity.identifier == "com.chatos.swift-client" else {
            logger.error("phase=identity-validation check=app-identifier result=mismatch")
            return false
        }
        guard let brokerIdentity = MacOSCodeSigning.identity(at: url) else {
            logger.error("phase=identity-validation check=broker-signature result=missing")
            return false
        }
        guard brokerIdentity.identifier == "com.chatos.swift-client.keychain-broker" else {
            logger.error("phase=identity-validation check=broker-identifier result=mismatch")
            return false
        }
        guard brokerIdentity.leafCertificateData == appIdentity.leafCertificateData else {
            logger.error("phase=identity-validation check=leaf-certificate result=mismatch")
            return false
        }
        return true
    }

    private static func credentialPurpose(for service: String) -> String {
        switch service {
        case "com.chatos.swift-client.authentication.v6":
            "authentication"
        case "com.chatos.local-agent.credentials.v7":
            "local-agent"
        case "com.chatos.native-connector.credentials.v1":
            "native-connector"
        default:
            service.hasPrefix("com.chatos.tests.") ? "test" : "unknown"
        }
    }

    private static func statusMessage(_ status: OSStatus) -> String {
        (SecCopyErrorMessageString(status, nil) as String?) ?? "unknown"
    }

    private static func secureDirectory(at url: URL) -> Bool {
        secureItem(at: url, expectedType: .typeDirectory, forbiddenPermissions: 0o077)
    }

    private static func secureRegularFile(at url: URL) -> Bool {
        secureItem(at: url, expectedType: .typeRegular, forbiddenPermissions: 0o022)
    }

    private static func secureItem(
        at url: URL,
        expectedType: FileAttributeType,
        forbiddenPermissions: Int
    ) -> Bool {
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: url.path),
              attributes[.type] as? FileAttributeType == expectedType,
              let owner = attributes[.ownerAccountID] as? NSNumber,
              owner.uint32Value == geteuid(),
              let permissions = attributes[.posixPermissions] as? NSNumber,
              permissions.intValue & forbiddenPermissions == 0
        else {
            return false
        }
        return true
    }
}
