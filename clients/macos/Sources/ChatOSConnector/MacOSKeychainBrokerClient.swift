// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSMacSecurity
import Darwin
import Foundation
import Security

public enum MacOSKeychainBrokerError: Error, Equatable, Sendable {
    case unavailable
    case invalidRequest
    case invalidResponse
    case status(OSStatus)
}

public struct MacOSKeychainBrokerClient: Sendable {
    private struct Request: Encodable {
        let operation: String
        let service: String
        let account: String
        let valueBase64: String?
    }

    private struct Response: Decodable {
        let status: Int32
        let valueBase64: String?
    }

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
            Request(operation: "load", service: service, account: account, valueBase64: nil)
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
        let response = try call(Request(
            operation: "save",
            service: service,
            account: account,
            valueBase64: value.base64EncodedString()
        ))
        guard response.status == errSecSuccess else {
            throw MacOSKeychainBrokerError.status(response.status)
        }
    }

    public func delete(service: String, account: String) throws {
        let response = try call(
            Request(operation: "delete", service: service, account: account, valueBase64: nil)
        )
        guard response.status == errSecSuccess || response.status == errSecItemNotFound else {
            throw MacOSKeychainBrokerError.status(response.status)
        }
    }

    private func call(_ request: Request) throws -> Response {
        guard executableURL.isFileURL,
              FileManager.default.isExecutableFile(atPath: executableURL.path),
              let requestData = try? JSONEncoder().encode(request),
              !requestData.isEmpty,
              requestData.count <= 128 * 1_024
        else {
            throw MacOSKeychainBrokerError.unavailable
        }
        if validatesProductionIdentity {
            guard Self.isTrustedProductionBroker(at: executableURL) else {
                throw MacOSKeychainBrokerError.unavailable
            }
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
            throw MacOSKeychainBrokerError.unavailable
        }
        input.fileHandleForReading.closeFile()
        input.fileHandleForWriting.write(requestData)
        input.fileHandleForWriting.closeFile()
        let responseData = output.fileHandleForReading.readDataToEndOfFile()
        let errorData = errors.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0,
              errorData.isEmpty,
              !responseData.isEmpty,
              responseData.count <= 128 * 1_024,
              let response = try? JSONDecoder().decode(Response.self, from: responseData)
        else {
            throw MacOSKeychainBrokerError.invalidResponse
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
                return (try installStableBroker(from: bundled, to: installed), true)
            } catch {
                return (installed, true)
            }
        }

        var directory = Bundle.main.executableURL?.deletingLastPathComponent()
        for _ in 0..<8 {
            guard let current = directory else { break }
            let candidate = current.appendingPathComponent("ChatOSKeychainBroker")
            if FileManager.default.isExecutableFile(atPath: candidate.path) {
                return (candidate, false)
            }
            directory = current.deletingLastPathComponent()
        }
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
            .appendingPathComponent("KeychainBrokerV1", isDirectory: true)
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
                throw MacOSKeychainBrokerError.unavailable
            }
            return destination
        }

        guard isTrustedProductionBroker(at: bundled) else {
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
            throw MacOSKeychainBrokerError.unavailable
        }
        do {
            try fileManager.moveItem(at: temporary, to: destination)
        } catch {
            guard fileManager.fileExists(atPath: destination.path),
                  isTrustedProductionBroker(at: destination)
            else {
                throw error
            }
        }
        return destination
    }

    private static func isTrustedProductionBroker(at url: URL) -> Bool {
        guard secureRegularFile(at: url),
              let appIdentity = MacOSCodeSigning.identityForCurrentProcess(),
              appIdentity.identifier == "com.chatos.swift-client",
              let brokerIdentity = MacOSCodeSigning.identity(at: url),
              brokerIdentity.identifier == "com.chatos.swift-client.keychain-broker",
              brokerIdentity.leafCertificateData == appIdentity.leafCertificateData
        else {
            return false
        }
        return true
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
