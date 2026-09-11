import XCTest
@testable import ChatOSCore

final class MediaGenerationModelsTests: XCTestCase {
    func testH3TailFrameCapabilityIsAvailableThroughNativeAndCompatibleContentProtocols() {
        let compatible = MediaGenerationModel(
            id: "new-api-h3", name: "MiniMax H3", provider: "gpt", modelName: "MiniMax-H3",
            enabled: true, taskEnabled: false, hasAPIKey: true
        )
        let native = MediaGenerationModel(
            id: "native-h3", name: "MiniMax H3", provider: "minimax", modelName: "MiniMax-H3",
            enabled: true, taskEnabled: false, hasAPIKey: true
        )
        XCTAssertTrue(compatible.supportsVideoLastFrame,
                      "NewAPI /v1/videos accepts MiniMax V2 first_frame and last_frame content roles")
        XCTAssertTrue(native.supportsVideoLastFrame,
                      "Native MiniMax V2 accepts the same first_frame and last_frame roles")
    }
}
