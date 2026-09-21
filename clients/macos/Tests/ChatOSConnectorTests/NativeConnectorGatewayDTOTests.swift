import Foundation
import Testing
@testable import ChatOSConnector

struct NativeConnectorGatewayDTOTests {
    @Test
    func pluginArtifactDownloadRetriesTransientNetworkFailuresOnly() {
        #expect(NativeConnectorGateway.shouldRetryArtifactDownload(
            after: URLError(.networkConnectionLost)
        ))
        #expect(NativeConnectorGateway.shouldRetryArtifactDownload(
            after: URLError(.timedOut)
        ))
        #expect(!NativeConnectorGateway.shouldRetryArtifactDownload(
            after: URLError(.userAuthenticationRequired)
        ))
        #expect(!NativeConnectorGateway.shouldRetryArtifactDownload(
            after: NativeConnectorError.server(status: 404, message: "missing")
        ))
    }

    @Test
    func connectorUnauthorizedResponseIsKeptSeparateFromPrimaryAuthentication() {
        #expect(NativeConnectorGateway.isConnectorAuthenticationRejected(
            statusCode: 401,
            token: "expired-token"
        ))
        #expect(!NativeConnectorGateway.isConnectorAuthenticationRejected(
            statusCode: 401,
            token: nil
        ))
        #expect(!NativeConnectorGateway.isConnectorAuthenticationRejected(
            statusCode: 500,
            token: "token"
        ))
    }

    @Test
    func pluginSourceDecodesPublisherObjectAndNestedCategory() throws {
        let data = Data(
            """
            {
              "items": [{
                "catalog": {
                  "id": "open-computer-use",
                  "display_name": "Open Computer Use",
                  "description": "Control local desktop applications.",
                  "publisher": { "name": "Open Computer Use" },
                  "interface": {
                    "category": "Developer Tools",
                    "developerName": "OpenAI"
                  }
                },
                "release": {
                  "id": "release-1",
                  "version": "0.3.42",
                  "artifact_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                  "npm_package": {
                    "name": "open-computer-use",
                    "version": "0.3.42",
                    "integrity": "sha512-YWJj"
                  }
                },
                "preference": { "enabled": true }
              }]
            }
            """.utf8
        )

        let decoded = try JSONDecoder().decode(GatewayPluginSourceListDTO.self, from: data)
        let source = try #require(decoded.items.first)

        #expect(source.catalog.publisher?.name == "Open Computer Use")
        #expect(source.catalog.interface?.category == "Developer Tools")
        #expect(source.catalog.interface?.developerName == "OpenAI")
        #expect(source.release.version == "0.3.42")
        #expect(source.release.artifactSHA256?.count == 64)
        #expect(source.release.npmPackage?.name == "open-computer-use")
        #expect(source.release.npmPackage?.integrity == "sha512-YWJj")
        #expect(source.preference?.enabled == true)
    }

    @Test
    func providerAndCompleteModelSettingsDecode() throws {
        let providerData = Data(
            """
            {
              "id": "provider-1",
              "name": "OpenAI Production",
              "provider": "gpt",
              "prompt_vendor": "gpt",
              "base_url": "https://api.openai.com/v1",
              "has_api_key": true,
              "enabled": true,
              "supports_images": true,
              "supports_reasoning": true,
              "supports_responses": true,
              "last_sync_status": "success",
              "imported_model_count": 8
            }
            """.utf8
        )
        let provider = try JSONDecoder().decode(GatewayModelProviderDTO.self, from: providerData)
        #expect(provider.name == "OpenAI Production")
        #expect(provider.promptVendor == "gpt")
        #expect(provider.hasAPIKey == true)
        #expect(provider.supportsResponses == true)
        #expect(provider.importedModelCount == 8)

        let modelData = Data(
            """
            {
              "id": "chat-only-model",
              "name": "Chat Only",
              "provider": "gpt",
              "model": "gpt-chat",
              "enabled": true,
              "task_enabled": false,
              "has_api_key": true
            }
            """.utf8
        )
        let model = try JSONDecoder().decode(GatewayModelConfigDTO.self, from: modelData)
        #expect(model.enabled == true)
        #expect(model.taskEnabled == false)

        let settingsData = Data(
            """
            {
              "model_request_max_retries": 4,
              "memory_summary_model_config_id": "memory-model",
              "memory_summary_thinking_level": "low",
            }
            """.utf8
        )
        let settings = try JSONDecoder().decode(GatewayModelSettingsDTO.self, from: settingsData)
        #expect(settings.modelRequestMaxRetries == 4)
        #expect(settings.memorySummaryModelConfigID == "memory-model")
        #expect(settings.memorySummaryThinkingLevel == "low")
    }

    @Test
    func managedAgentPromptBundleDecodesGatewayContract() throws {
        let data = Data(
            """
            {
              "bundle_version": 12,
              "updated_at": "2026-09-21T00:00:00Z",
              "prompts": [{
                "agent_key": "local_connector_command_approval_agent",
                "vendor": "gpt",
                "content": "managed approval prompt",
                "revision": 3,
                "checksum": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "published_at": "2026-09-21T00:00:00Z"
              }]
            }
            """.utf8
        )

        let bundle = try JSONDecoder().decode(GatewayAgentPromptBundleDTO.self, from: data)
        let prompt = try #require(bundle.prompts.first)
        #expect(bundle.bundleVersion == 12)
        #expect(prompt.agentKey == NativeApprovalAgent.agentKey)
        #expect(prompt.vendor == "gpt")
        #expect(prompt.revision == 3)
    }

    @Test
    func managedAgentCapabilityDecodesGatewayContract() throws {
        let data = Data(
            """
            {
              "agent_key": "local_connector_command_approval_agent",
              "owner_user_id": "owner-1",
              "policy_revision": "policy-9",
              "agent_enabled": true
            }
            """.utf8
        )

        let capability = try JSONDecoder().decode(GatewayAgentCapabilityDTO.self, from: data)
        #expect(capability.agentKey == NativeApprovalAgent.agentKey)
        #expect(capability.ownerUserID == "owner-1")
        #expect(capability.policyRevision == "policy-9")
        #expect(capability.agentEnabled)
    }
}
