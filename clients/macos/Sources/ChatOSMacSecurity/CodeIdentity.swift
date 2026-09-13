// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Darwin
import Foundation
import Security

public struct MacOSCodeIdentity: Equatable, Sendable {
    public let identifier: String
    public let leafCertificateData: Data

    public init(identifier: String, leafCertificateData: Data) {
        self.identifier = identifier
        self.leafCertificateData = leafCertificateData
    }
}

public enum MacOSCodeSigning {
    public static func identityForCurrentProcess() -> MacOSCodeIdentity? {
        var code: SecCode?
        guard SecCodeCopySelf([], &code) == errSecSuccess,
              let code
        else {
            return nil
        }
        return identity(for: code)
    }

    public static func identity(forProcessID processID: pid_t) -> MacOSCodeIdentity? {
        var code: SecCode?
        let attributes = [
            kSecGuestAttributePid as String: NSNumber(value: processID),
        ] as CFDictionary
        guard SecCodeCopyGuestWithAttributes(nil, attributes, [], &code) == errSecSuccess,
              let code
        else {
            return nil
        }
        return identity(for: code)
    }

    public static func identity(at executableURL: URL) -> MacOSCodeIdentity? {
        guard executableURL.isFileURL else { return nil }
        var code: SecStaticCode?
        guard SecStaticCodeCreateWithPath(executableURL as CFURL, [], &code) == errSecSuccess,
              let code
        else {
            return nil
        }
        return identity(for: code)
    }

    private static func identity(for dynamicCode: SecCode) -> MacOSCodeIdentity? {
        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(dynamicCode, [], &staticCode) == errSecSuccess,
              let staticCode
        else {
            return nil
        }
        return identity(for: staticCode)
    }

    private static func identity(for staticCode: SecStaticCode) -> MacOSCodeIdentity? {
        guard SecStaticCodeCheckValidity(
            staticCode,
            SecCSFlags(rawValue: kSecCSStrictValidate),
            nil
        ) == errSecSuccess else {
            return nil
        }
        var rawInformation: CFDictionary?
        guard SecCodeCopySigningInformation(
            staticCode,
            SecCSFlags(rawValue: kSecCSSigningInformation),
            &rawInformation
        ) == errSecSuccess,
        let information = rawInformation as? [String: Any],
        let identifier = information[kSecCodeInfoIdentifier as String] as? String,
        let certificates = information[kSecCodeInfoCertificates as String] as? [SecCertificate],
        let leafCertificate = certificates.first
        else {
            return nil
        }
        return MacOSCodeIdentity(
            identifier: identifier,
            leafCertificateData: SecCertificateCopyData(leafCertificate) as Data
        )
    }
}
