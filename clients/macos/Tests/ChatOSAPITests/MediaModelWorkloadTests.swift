import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class MediaModelWorkloadTests: XCTestCase {
    func testImageRequestRejectsChatAndVideoModelsAndUsesTheSelectedImageModel() throws {
        let request = ImageGenerationRequest(modelConfigID: "selected", prompt: "scene", size: nil, count: 1)
        for name in ["gpt-6.1-sol", "MiniMax-H3"] {
            let runtime = RuntimeModelConfig(provider: "gpt", model: name, apiKey: "test-key", baseURL: "https://example.com/v1", enabled: true)
            XCTAssertThrowsError(try ChatOSMediaGenerationService.makeProviderRequest(runtime: runtime, request: request))
        }
        for name in ["gpt-image-1.5", "gpt-image-2", "gpt-image-2.5-sunburst"] {
            let runtime = RuntimeModelConfig(provider: "gpt", model: name, apiKey: "test-key", baseURL: "https://example.com/v1", enabled: true)
            let provider = try ChatOSMediaGenerationService.makeProviderRequest(runtime: runtime, request: request)
            let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(provider.body)) as? [String: Any])
            XCTAssertEqual(payload["model"] as? String, name)
            XCTAssertEqual(provider.url.path, "/v1/images/generations")
        }
    }

}
