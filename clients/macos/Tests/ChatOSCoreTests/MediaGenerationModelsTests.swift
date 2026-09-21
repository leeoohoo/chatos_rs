import XCTest
@testable import ChatOSCore

final class MediaGenerationModelsTests: XCTestCase {
    func testH3TailFrameCapabilityIsAvailableThroughUnifiedNewAPIProtocol() {
        let compatible = MediaGenerationModel(
            id: "new-api-h3", name: "MiniMax H3", provider: "gpt", modelName: "MiniMax-H3",
            enabled: true, taskEnabled: false, hasAPIKey: true
        )
        XCTAssertTrue(compatible.supportsVideoLastFrame,
                      "NewAPI /v1/videos accepts first_frame_image and last_frame_image metadata")
    }

    func testSeedance25AdvertisesEditExtendAndOfficialDurationRange() {
        let model = MediaGenerationModel(
            id: "seedance", name: "Seedance 2.5", provider: "volcengine",
            modelName: "doubao-seedance-2-5-260628",
            enabled: true, taskEnabled: false, hasAPIKey: true
        )
        let profile = VideoGenerationProfile(modelName: model.modelName)
        XCTAssertTrue(model.supportsVideoLastFrame)
        XCTAssertTrue(model.supportsVideoReference)
        XCTAssertTrue(model.supportsVideoEditing)
        XCTAssertTrue(model.supportsVideoExtension)
        XCTAssertEqual(profile.durations, Array(4...30))
        XCTAssertEqual(profile.sizes, ["720p", "1080p", "480p"])

        var compatibleOnly = model
        compatibleOnly.provider = "openai"
        XCTAssertTrue(compatibleOnly.supportsVideoEditing)
        XCTAssertTrue(compatibleOnly.supportsVideoExtension)
        XCTAssertTrue(compatibleOnly.supportsVideoReference)
    }
}
