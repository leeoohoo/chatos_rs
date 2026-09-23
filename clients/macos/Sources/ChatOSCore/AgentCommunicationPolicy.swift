import CryptoKit
import Foundation

/// Product-owned limits shared by local Agent communication tools, prompts and tests.
/// Callers must use `standard` instead of duplicating numeric thresholds.
public struct AgentCommunicationPolicy: Codable, Sendable, Equatable {
    public let recommendedMessageCharacters: Int
    public let maximumMessageCharacters: Int
    public let maximumDocumentsPerMessage: Int
    public let maximumDocumentsPerRun: Int
    public let maximumDocumentBytes: Int
    public let maximumDocumentBytesPerRun: Int

    /// Lower edge of the product's recommended 300–800 character range. Keeping this derived
    /// property here prevents observability buckets from owning a second threshold source.
    public var conciseMessageCharacters: Int {
        min(300, recommendedMessageCharacters)
    }

    public static let standard = AgentCommunicationPolicy(
        recommendedMessageCharacters: 800,
        maximumMessageCharacters: 2_000,
        maximumDocumentsPerMessage: 5,
        maximumDocumentsPerRun: 20,
        maximumDocumentBytes: 2 * 1_024 * 1_024,
        maximumDocumentBytesPerRun: 8 * 1_024 * 1_024
    )

    public init(
        recommendedMessageCharacters: Int,
        maximumMessageCharacters: Int,
        maximumDocumentsPerMessage: Int,
        maximumDocumentsPerRun: Int,
        maximumDocumentBytes: Int,
        maximumDocumentBytesPerRun: Int
    ) {
        precondition(recommendedMessageCharacters > 0)
        precondition(maximumMessageCharacters >= recommendedMessageCharacters)
        precondition(maximumDocumentsPerMessage > 0)
        precondition(maximumDocumentsPerRun >= maximumDocumentsPerMessage)
        precondition(maximumDocumentBytes > 0)
        precondition(maximumDocumentBytesPerRun >= maximumDocumentBytes)
        self.recommendedMessageCharacters = recommendedMessageCharacters
        self.maximumMessageCharacters = maximumMessageCharacters
        self.maximumDocumentsPerMessage = maximumDocumentsPerMessage
        self.maximumDocumentsPerRun = maximumDocumentsPerRun
        self.maximumDocumentBytes = maximumDocumentBytes
        self.maximumDocumentBytesPerRun = maximumDocumentBytesPerRun
    }
}

public enum LocalAgentCommunicationSkillAudience: String, Codable, Sendable {
    case manager
    case executor
}

public struct LocalAgentCommunicationSkillSnapshot: Codable, Sendable, Equatable {
    public let name: String
    public let version: Int
    public let language: ChatOSLanguage
    public let audience: LocalAgentCommunicationSkillAudience
    public let markdown: String
    public let contentSHA256: String

    public var promptBlock: String {
        """
        <skill name="\(name)" binding="product-owned" version="\(version)" audience="\(audience.rawValue)" sha256="\(contentSHA256)">
        \(markdown)
        </skill>
        """
    }
}

/// Immutable product Skill used by every local Agent communication run.
public enum LocalAgentCompactCommunicationSkill {
    public static let name = "chatos-compact-communication"
    public static let version = 2

    public static func snapshot(
        language: ChatOSLanguage,
        audience: LocalAgentCommunicationSkillAudience
    ) -> LocalAgentCommunicationSkillSnapshot {
        let markdown = content(language: language, audience: audience)
        let digest = SHA256.hash(data: Data(markdown.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
        return .init(
            name: name,
            version: version,
            language: language,
            audience: audience,
            markdown: markdown,
            contentSHA256: digest
        )
    }

    private static func content(
        language: ChatOSLanguage,
        audience: LocalAgentCommunicationSkillAudience
    ) -> String {
        let resource: String
        switch (audience, language) {
        case (.manager, .simplifiedChinese): resource = "SKILL.zh-CN"
        case (.manager, .english): resource = "SKILL.en"
        case (.executor, .simplifiedChinese): resource = "EXECUTOR.zh-CN"
        case (.executor, .english): resource = "EXECUTOR.en"
        }
        let subdirectory = "Skills/chatos-compact-communication"
        guard let url = Bundle.module.url(
            forResource: resource,
            withExtension: "md",
            subdirectory: subdirectory
        ) ?? Bundle.module.url(forResource: resource, withExtension: "md"),
        let value = try? String(contentsOf: url, encoding: .utf8) else {
            fatalError("Bundled compact communication Skill is missing: \(resource)")
        }
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalized.isEmpty else {
            fatalError("Bundled compact communication Skill is empty: \(resource)")
        }
        return normalized
    }
}
