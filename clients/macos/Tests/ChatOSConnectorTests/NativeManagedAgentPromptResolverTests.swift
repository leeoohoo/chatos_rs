import CryptoKit
import Foundation
import XCTest
@testable import ChatOSConnector

final class NativeManagedAgentPromptResolverTests: XCTestCase {
    func testResolvesPublishedPromptsForEachAgentAndModelVendor() throws {
        let bundle = promptBundle([
            prompt(agentKey: "chatos_conversation_agent", vendor: "gpt", content: "main-gpt"),
            prompt(agentKey: "local_agent_execution_agent", vendor: "gpt", content: "task-gpt"),
            prompt(agentKey: "chatos_conversation_agent", vendor: "deepseek", content: "main-deepseek"),
            prompt(agentKey: "local_agent_execution_agent", vendor: "deepseek", content: "task-deepseek"),
        ])

        let openAI = model(id: "gpt-model", provider: "openai", promptVendor: nil)
        XCTAssertEqual(try NativeManagedAgentPromptResolver.resolve(
            agentKey: "chatos_conversation_agent",
            model: openAI,
            bundle: bundle
        ).content, "main-gpt")
        XCTAssertEqual(try NativeManagedAgentPromptResolver.resolve(
            agentKey: "local_agent_execution_agent",
            model: openAI,
            bundle: bundle
        ).content, "task-gpt")

        let compatible = model(
            id: "deepseek-model",
            provider: "openai_compatible",
            promptVendor: "deepseek"
        )
        XCTAssertEqual(try NativeManagedAgentPromptResolver.resolve(
            agentKey: "chatos_conversation_agent",
            model: compatible,
            bundle: bundle
        ).content, "main-deepseek")
    }

    func testRejectsMissingOrModifiedPublishedPrompt() throws {
        var modified = prompt(
            agentKey: "chatos_conversation_agent",
            vendor: "gpt",
            content: "managed"
        )
        modified.checksum = "sha256:\(String(repeating: "0", count: 64))"
        XCTAssertThrowsError(try NativeManagedAgentPromptResolver.resolve(
            agentKey: "chatos_conversation_agent",
            model: model(id: "gpt-model", provider: "gpt", promptVendor: nil),
            bundle: promptBundle([modified])
        ))
        XCTAssertThrowsError(try NativeManagedAgentPromptResolver.resolve(
            agentKey: "local_agent_execution_agent",
            model: model(id: "gpt-model", provider: "gpt", promptVendor: nil),
            bundle: promptBundle([
                prompt(
                    agentKey: "chatos_conversation_agent",
                    vendor: "gpt",
                    content: "managed"
                ),
            ])
        ))
    }

    func testBootstrapKeepsModelSpecificCapabilityRevisions() throws {
        let first = modelSnapshot(id: "model-1")
        let second = modelSnapshot(id: "model-2")
        let firstCapability = capability(revision: "prompt-gpt")
        let secondCapability = capability(revision: "prompt-deepseek")
        let result = NativeLocalAgentBootstrapResult(
            modelSnapshots: [first, second],
            modelOptions: [],
            capabilitySnapshot: firstCapability,
            capabilitySnapshotsByModelConfigRef: [
                "model-1": firstCapability,
                "model-2": secondCapability,
            ]
        )

        XCTAssertEqual(
            result.capabilitySnapshot(forModelConfigRef: "model-1")?.capabilityPolicyRevision,
            "prompt-gpt"
        )
        XCTAssertEqual(
            result.capabilitySnapshot(forModelConfigRef: "model-2")?.capabilityPolicyRevision,
            "prompt-deepseek"
        )
    }

    private func model(
        id: String,
        provider: String,
        promptVendor: String?
    ) -> GatewayModelConfigDTO {
        .init(
            id: id,
            sourceProviderID: nil,
            name: id,
            provider: provider,
            promptVendor: promptVendor,
            model: id,
            apiKey: "key",
            baseURL: "https://example.test/v1",
            taskUsageScenario: nil,
            taskThinkingLevel: nil,
            temperature: nil,
            maxOutputTokens: nil,
            enabled: true,
            taskEnabled: true,
            hasAPIKey: true,
            supportsImages: false,
            supportsReasoning: true,
            supportsResponses: true
        )
    }

    private func promptBundle(
        _ prompts: [GatewayAgentPromptDTO]
    ) -> GatewayAgentPromptBundleDTO {
        .init(
            bundleVersion: 7,
            updatedAt: "2026-10-07T00:00:00Z",
            prompts: prompts
        )
    }

    private func prompt(
        agentKey: String,
        vendor: String,
        content: String
    ) -> GatewayAgentPromptDTO {
        let checksum = SHA256.hash(data: Data(content.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
        return .init(
            agentKey: agentKey,
            vendor: vendor,
            content: content,
            revision: 3,
            checksum: "sha256:\(checksum)",
            publishedAt: "2026-10-07T00:00:00Z"
        )
    }

    private func modelSnapshot(id: String) -> LocalAgentModelConfigSnapshot {
        .init(
            ownerUserID: "user-1",
            modelConfigRef: id,
            modelConfigRevision: "revision-1",
            credentialRef: "env:MODEL_KEY",
            baseURL: "https://example.test/v1",
            model: id,
            provider: "openai",
            supportsResponses: true
        )
    }

    private func capability(revision: String) -> LocalAgentCapabilityPolicySnapshot {
        .init(
            ownerUserID: "user-1",
            profileKey: "main_chat",
            capabilityPolicyRevision: revision
        )
    }
}
