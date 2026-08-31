import Foundation
import Testing
@testable import ChatOSConnector

struct NativeConnectorGatewayDTOTests {
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

        let settingsData = Data(
            """
            {
              "model_request_max_retries": 4,
              "memory_summary_model_config_id": "memory-model",
              "memory_summary_thinking_level": "low",
              "project_management_agent_model_config_id": "project-model",
              "project_management_agent_thinking_level": "high"
            }
            """.utf8
        )
        let settings = try JSONDecoder().decode(GatewayModelSettingsDTO.self, from: settingsData)
        #expect(settings.modelRequestMaxRetries == 4)
        #expect(settings.memorySummaryModelConfigID == "memory-model")
        #expect(settings.memorySummaryThinkingLevel == "low")
        #expect(settings.projectManagementAgentModelConfigID == "project-model")
        #expect(settings.projectManagementAgentThinkingLevel == "high")
    }
}
