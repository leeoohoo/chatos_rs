import Darwin
import Foundation
import LocalAuthentication
import OSLog
import Security
import ChatOSMacSecurity

private struct BrokerRequest: Decodable {
    let requestID: String
    let operation: String
    let service: String
    let account: String
    let valueBase64: String?
}

private struct BrokerResponse: Encodable {
    let status: Int32
    let valueBase64: String?
    let phase: String
}

private let logger = Logger(
    subsystem: "com.chatos.swift-client.keychain-broker",
    category: "KeychainBroker"
)

private let allowedProductionServices: Set<String> = [
    "com.chatos.swift-client.authentication.v6",
    "com.chatos.local-agent.credentials.v7",
    "com.chatos.native-connector.credentials.v1",
]

private func valid(_ value: String, maximumLength: Int = 2_048) -> Bool {
    !value.isEmpty
        && value.count <= maximumLength
        && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
        && !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
}

private func serviceIsAllowed(_ service: String) -> Bool {
    allowedProductionServices.contains(service)
        || service.hasPrefix("com.chatos.tests.")
}

private func credentialPurpose(for service: String) -> String {
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

private func statusMessage(_ status: OSStatus) -> String {
    (SecCopyErrorMessageString(status, nil) as String?) ?? "unknown"
}

private func logStatus(
    request: BrokerRequest,
    operation: String,
    phase: String,
    status: OSStatus
) {
    let purpose = credentialPurpose(for: request.service)
    let message = statusMessage(status)
    if status == errSecSuccess || status == errSecItemNotFound {
        logger.info(
            "request=\(request.requestID, privacy: .public) operation=\(operation, privacy: .public) purpose=\(purpose, privacy: .public) phase=\(phase, privacy: .public) status=\(status, privacy: .public) message=\(message, privacy: .public)"
        )
    } else {
        logger.error(
            "request=\(request.requestID, privacy: .public) operation=\(operation, privacy: .public) purpose=\(purpose, privacy: .public) phase=\(phase, privacy: .public) status=\(status, privacy: .public) message=\(message, privacy: .public)"
        )
    }
}

private func processPath(_ processID: pid_t) -> String? {
    var buffer = [UInt8](repeating: 0, count: 4 * 1_024)
    let length = proc_pidpath(processID, &buffer, UInt32(buffer.count))
    guard length > 0 else { return nil }
    return String(decoding: buffer.prefix(Int(length)), as: UTF8.self)
}

private func parentIsAllowed(for service: String) -> Bool {
    let parentProcessID = getppid()
    guard let parentPath = processPath(parentProcessID) else {
        logger.error("phase=parent-validation check=parent-path result=missing")
        return false
    }
    let normalizedParent = URL(fileURLWithPath: parentPath)
        .resolvingSymlinksInPath()
        .standardizedFileURL.path
    let ownExecutable = URL(fileURLWithPath: CommandLine.arguments[0])
        .resolvingSymlinksInPath()
        .standardizedFileURL

    if allowedProductionServices.contains(service) {
        guard ownExecutable.lastPathComponent == "chatos_keychain_broker" else {
            logger.error("phase=parent-validation check=broker-name result=mismatch")
            return false
        }
        let parentExecutable = URL(fileURLWithPath: normalizedParent)
        let appBundle = parentExecutable
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        guard parentExecutable.lastPathComponent == "ChatOSSwift",
              parentExecutable.deletingLastPathComponent().lastPathComponent == "MacOS",
              parentExecutable.deletingLastPathComponent()
                .deletingLastPathComponent().lastPathComponent == "Contents",
              appBundle.pathExtension == "app",
              let brokerIdentity = MacOSCodeSigning.identityForCurrentProcess(),
              brokerIdentity.identifier == "com.chatos.swift-client.keychain-broker",
              let parentIdentity = MacOSCodeSigning.identity(forProcessID: parentProcessID),
              parentIdentity.identifier == "com.chatos.swift-client",
              parentIdentity.leafCertificateData == brokerIdentity.leafCertificateData
        else {
            logger.error("phase=parent-validation check=production-identity result=rejected")
            return false
        }
        logger.debug("phase=parent-validation check=production-identity result=accepted")
        return true
    }

    return service.hasPrefix("com.chatos.tests.")
        && ownExecutable.path.contains("/.build/")
}

private func query(service: String, account: String) -> [String: Any] {
    let context = LAContext()
    context.interactionNotAllowed = true
    return [
        kSecClass as String: kSecClassGenericPassword,
        kSecAttrService as String: service,
        kSecAttrAccount as String: account,
        kSecAttrSynchronizable as String: false,
        kSecUseAuthenticationContext as String: context,
        // Legacy macOS Keychain ignores LAContext.interactionNotAllowed for
        // ACL confirmation. The stable Broker must therefore never be
        // overwritten after its first trusted installation; a real Broker
        // upgrade uses a new version directory and new service identifiers.
        kSecUseAuthenticationUI as String: kSecUseAuthenticationUIFail,
    ]
}

private func execute(_ request: BrokerRequest) -> BrokerResponse {
    let purpose = credentialPurpose(for: request.service)
    guard UUID(uuidString: request.requestID) != nil else {
        logger.error("phase=request-validation check=request-id result=rejected")
        return BrokerResponse(status: errSecParam, valueBase64: nil, phase: "request-validation")
    }
    guard serviceIsAllowed(request.service) else {
        logger.error(
            "request=\(request.requestID, privacy: .public) purpose=\(purpose, privacy: .public) phase=request-validation check=service result=rejected"
        )
        return BrokerResponse(status: errSecParam, valueBase64: nil, phase: "service-validation")
    }
    guard parentIsAllowed(for: request.service) else {
        logger.error(
            "request=\(request.requestID, privacy: .public) purpose=\(purpose, privacy: .public) phase=request-validation check=parent result=rejected"
        )
        return BrokerResponse(status: errSecParam, valueBase64: nil, phase: "parent-validation")
    }
    guard valid(request.account) else {
        logger.error(
            "request=\(request.requestID, privacy: .public) purpose=\(purpose, privacy: .public) phase=request-validation check=account-shape result=rejected"
        )
        return BrokerResponse(status: errSecParam, valueBase64: nil, phase: "account-validation")
    }

    let base = query(service: request.service, account: request.account)
    switch request.operation {
    case "load":
        var loadQuery = base
        loadQuery[kSecReturnData as String] = true
        loadQuery[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(loadQuery as CFDictionary, &result)
        logStatus(request: request, operation: "load", phase: "copy-matching", status: status)
        if status == errSecItemNotFound {
            return BrokerResponse(status: status, valueBase64: nil, phase: "copy-matching")
        }
        guard status == errSecSuccess, let data = result as? Data else {
            return BrokerResponse(status: status, valueBase64: nil, phase: "copy-matching")
        }
        return BrokerResponse(
            status: status,
            valueBase64: data.base64EncodedString(),
            phase: "copy-matching"
        )

    case "save":
        guard let encoded = request.valueBase64,
              encoded.utf8.count <= 96 * 1_024,
              let value = Data(base64Encoded: encoded),
              !value.isEmpty,
              value.count <= 64 * 1_024
        else {
            logStatus(
                request: request,
                operation: "save",
                phase: "value-validation",
                status: errSecParam
            )
            return BrokerResponse(status: errSecParam, valueBase64: nil, phase: "value-validation")
        }
        let updateStatus = SecItemUpdate(
            base as CFDictionary,
            [kSecValueData as String: value] as CFDictionary
        )
        logStatus(request: request, operation: "save", phase: "update", status: updateStatus)
        if updateStatus == errSecSuccess {
            return BrokerResponse(status: updateStatus, valueBase64: nil, phase: "update")
        }
        guard updateStatus == errSecItemNotFound else {
            return BrokerResponse(status: updateStatus, valueBase64: nil, phase: "update")
        }
        var addition = base
        addition[kSecValueData as String] = value
        addition[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let addStatus = SecItemAdd(addition as CFDictionary, nil)
        logStatus(request: request, operation: "save", phase: "add", status: addStatus)
        return BrokerResponse(status: addStatus, valueBase64: nil, phase: "add")

    case "delete":
        let status = SecItemDelete(base as CFDictionary)
        logStatus(request: request, operation: "delete", phase: "delete", status: status)
        return BrokerResponse(status: status, valueBase64: nil, phase: "delete")

    default:
        logger.error(
            "request=\(request.requestID, privacy: .public) purpose=\(purpose, privacy: .public) phase=operation-validation result=rejected"
        )
        return BrokerResponse(status: errSecParam, valueBase64: nil, phase: "operation-validation")
    }
}

private func write(_ response: BrokerResponse) {
    guard let data = try? JSONEncoder().encode(response), data.count <= 128 * 1_024 else {
        exit(2)
    }
    FileHandle.standardOutput.write(data)
}

let input = FileHandle.standardInput.readDataToEndOfFile()
guard !input.isEmpty, input.count <= 128 * 1_024,
      let request = try? JSONDecoder().decode(BrokerRequest.self, from: input)
else {
    logger.error("phase=input-decode result=invalid")
    write(BrokerResponse(status: errSecParam, valueBase64: nil, phase: "input-decode"))
    exit(1)
}
write(execute(request))
