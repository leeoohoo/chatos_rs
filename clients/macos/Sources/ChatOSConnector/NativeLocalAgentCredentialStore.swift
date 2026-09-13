// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation
import Security

public enum NativeLocalAgentCredentialStoreError: Error, Equatable, Sendable {
    case invalidReference
    case keychain(operation: String, status: OSStatus)
}

/// Account-scoped secure storage used by the signed Host bootstrap channel.
/// Local Agent credentials are never mirrored into preferences, SQLite, JSON
/// files, environment variables, or diagnostic payloads. Every operation uses
/// a non-interactive authentication context so a background lifecycle task
/// fails instead of opening a password or biometric prompt.
public actor NativeLocalAgentCredentialStore {
    public static let productionService = "com.chatos.local-agent.credentials.v7"

    private let service: String
    private let broker: MacOSKeychainBrokerClient

    public init(
        service: String = productionService,
        broker: MacOSKeychainBrokerClient = .init()
    ) throws {
        guard Self.valid(service) else {
            throw NativeLocalAgentCredentialStoreError.invalidReference
        }
        self.service = service
        self.broker = broker
    }

    public func load(accountID: String, reference: String) throws -> Data? {
        let account = try accountKey(accountID: accountID, reference: reference)
        do {
            return try broker.load(service: service, account: account)
        } catch {
            throw brokerError(error, operation: "load")
        }
    }

    public func save(_ secret: Data, accountID: String, reference: String) throws {
        guard !secret.isEmpty else {
            throw NativeLocalAgentCredentialStoreError.invalidReference
        }
        let account = try accountKey(accountID: accountID, reference: reference)
        do {
            try broker.save(secret, service: service, account: account)
        } catch {
            throw brokerError(error, operation: "save")
        }
    }

    public func delete(accountID: String, reference: String) throws {
        let account = try accountKey(accountID: accountID, reference: reference)
        do {
            try broker.delete(service: service, account: account)
        } catch {
            throw brokerError(error, operation: "delete")
        }
    }

    func isAvailableForNonInteractiveAccess() -> Bool {
        do {
            _ = try broker.load(service: service, account: "availability-probe")
            return true
        } catch {
            return false
        }
    }

    private func accountKey(accountID: String, reference: String) throws -> String {
        guard Self.valid(accountID), Self.valid(reference) else {
            throw NativeLocalAgentCredentialStoreError.invalidReference
        }
        return "v1:\(accountID.utf8.count):\(accountID)\(reference)"
    }

    private static func valid(_ value: String) -> Bool {
        !value.isEmpty
            && value.count <= 512
            && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
            && !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }

    private func brokerError(
        _ error: Error,
        operation: String
    ) -> NativeLocalAgentCredentialStoreError {
        let status: OSStatus
        if case let MacOSKeychainBrokerError.status(value) = error {
            status = value
        } else {
            status = errSecNotAvailable
        }
        return .keychain(operation: operation, status: status)
    }
}
