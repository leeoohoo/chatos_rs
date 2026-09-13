// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation
import Security
import ChatOSMacSecurity

struct NativeLocalAgentHostIdentityError: Error, Equatable, Sendable {
    let operation: String
    let status: OSStatus
}

enum NativeLocalAgentHostIdentity {
    static let identifier = "com.chatos.swift-client.local-agent-host"

    static func validate(executableURL: URL) throws {
        var staticCode: SecStaticCode?
        let createStatus = SecStaticCodeCreateWithPath(
            executableURL as CFURL,
            SecCSFlags(),
            &staticCode
        )
        guard createStatus == errSecSuccess, let staticCode else {
            throw NativeLocalAgentHostIdentityError(
                operation: "open-host-code-signature",
                status: createStatus
            )
        }

        var requirement: SecRequirement?
        let requirementStatus = SecRequirementCreateWithString(
            "identifier \"\(identifier)\"" as CFString,
            SecCSFlags(),
            &requirement
        )
        guard requirementStatus == errSecSuccess, let requirement else {
            throw NativeLocalAgentHostIdentityError(
                operation: "create-host-code-requirement",
                status: requirementStatus
            )
        }
        let validationStatus = SecStaticCodeCheckValidity(
            staticCode,
            SecCSFlags(rawValue: kSecCSStrictValidate | kSecCSCheckAllArchitectures),
            requirement
        )
        guard validationStatus == errSecSuccess else {
            throw NativeLocalAgentHostIdentityError(
                operation: "validate-host-code-signature",
                status: validationStatus
            )
        }
        guard let appIdentity = MacOSCodeSigning.identityForCurrentProcess(),
              appIdentity.identifier == "com.chatos.swift-client",
              let hostIdentity = MacOSCodeSigning.identity(at: executableURL),
              hostIdentity.identifier == identifier,
              hostIdentity.leafCertificateData == appIdentity.leafCertificateData
        else {
            throw NativeLocalAgentHostIdentityError(
                operation: "match-host-code-signature",
                status: errSecCSReqFailed
            )
        }
    }

    static func validate(processID: pid_t) throws {
        guard processID > 1,
              let appIdentity = MacOSCodeSigning.identityForCurrentProcess(),
              appIdentity.identifier == "com.chatos.swift-client",
              let hostIdentity = MacOSCodeSigning.identity(forProcessID: processID),
              hostIdentity.identifier == identifier,
              hostIdentity.leafCertificateData == appIdentity.leafCertificateData
        else {
            throw NativeLocalAgentHostIdentityError(
                operation: "validate-running-host-code-signature",
                status: errSecCSReqFailed
            )
        }
    }
}
