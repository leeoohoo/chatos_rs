import Darwin
import Foundation
import LocalAuthentication
import Security
import ChatOSMacSecurity

private struct BrokerRequest: Decodable {
    let operation: String
    let service: String
    let account: String
    let valueBase64: String?
}

private struct BrokerResponse: Encodable {
    let status: Int32
    let valueBase64: String?
}

private let allowedProductionServices: Set<String> = [
    "com.chatos.swift-client.authentication.v6",
    "com.chatos.local-agent.credentials.v7",
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

private func processPath(_ processID: pid_t) -> String? {
    var buffer = [UInt8](repeating: 0, count: 4 * 1_024)
    let length = proc_pidpath(processID, &buffer, UInt32(buffer.count))
    guard length > 0 else { return nil }
    return String(decoding: buffer.prefix(Int(length)), as: UTF8.self)
}

private func parentIsAllowed(for service: String) -> Bool {
    let parentProcessID = getppid()
    guard let parentPath = processPath(parentProcessID) else { return false }
    let normalizedParent = URL(fileURLWithPath: parentPath)
        .resolvingSymlinksInPath()
        .standardizedFileURL.path
    let ownExecutable = URL(fileURLWithPath: CommandLine.arguments[0])
        .resolvingSymlinksInPath()
        .standardizedFileURL

    if allowedProductionServices.contains(service) {
        guard ownExecutable.lastPathComponent == "chatos_keychain_broker" else { return false }
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
            return false
        }
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
        // LAContext.interactionNotAllowed only guarantees silent failure for
        // Data Protection keychain items on macOS. ChatOS local development
        // builds use the legacy login keychain because a self-signed binary
        // cannot carry Apple's restricted keychain-access-group entitlement.
        // The legacy query flag is therefore also required: without it an ACL
        // mismatch can still launch SecurityAgent and ask for the user's macOS
        // password even though the LAContext forbids interaction.
        kSecUseAuthenticationUI as String: kSecUseAuthenticationUIFail,
    ]
}

private func execute(_ request: BrokerRequest) -> BrokerResponse {
    guard serviceIsAllowed(request.service),
          parentIsAllowed(for: request.service),
          valid(request.account)
    else {
        return BrokerResponse(status: errSecParam, valueBase64: nil)
    }

    let base = query(service: request.service, account: request.account)
    switch request.operation {
    case "load":
        var loadQuery = base
        loadQuery[kSecReturnData as String] = true
        loadQuery[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(loadQuery as CFDictionary, &result)
        if status == errSecItemNotFound {
            return BrokerResponse(status: status, valueBase64: nil)
        }
        guard status == errSecSuccess, let data = result as? Data else {
            return BrokerResponse(status: status, valueBase64: nil)
        }
        return BrokerResponse(status: status, valueBase64: data.base64EncodedString())

    case "save":
        guard let encoded = request.valueBase64,
              encoded.utf8.count <= 96 * 1_024,
              let value = Data(base64Encoded: encoded),
              !value.isEmpty,
              value.count <= 64 * 1_024
        else {
            return BrokerResponse(status: errSecParam, valueBase64: nil)
        }
        let updateStatus = SecItemUpdate(
            base as CFDictionary,
            [kSecValueData as String: value] as CFDictionary
        )
        if updateStatus == errSecSuccess {
            return BrokerResponse(status: updateStatus, valueBase64: nil)
        }
        guard updateStatus == errSecItemNotFound else {
            return BrokerResponse(status: updateStatus, valueBase64: nil)
        }
        var addition = base
        addition[kSecValueData as String] = value
        addition[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        return BrokerResponse(
            status: SecItemAdd(addition as CFDictionary, nil),
            valueBase64: nil
        )

    case "delete":
        return BrokerResponse(
            status: SecItemDelete(base as CFDictionary),
            valueBase64: nil
        )

    default:
        return BrokerResponse(status: errSecParam, valueBase64: nil)
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
    write(BrokerResponse(status: errSecParam, valueBase64: nil))
    exit(1)
}
write(execute(request))
