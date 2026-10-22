import Combine
import ChatOSCore
import XCTest
@testable import ChatOSApp

@MainActor
final class StoryStudioActivityStateTests: XCTestCase {
    func testHighFrequencyActivityDoesNotPublishTheRootViewModel() {
        let viewModel = StoryStudioViewModel(media: StoryTestService())
        let key = StoryStudioViewModel.VideoGenerationKey(
            projectID: UUID(),
            segmentID: "segment-1"
        )
        var rootPublications = 0
        var generationPublications = 0
        var planningPublications = 0
        let rootSubscription = viewModel.objectWillChange.sink { rootPublications += 1 }
        let generationSubscription = viewModel.generationActivity.objectWillChange.sink {
            generationPublications += 1
        }
        let planningSubscription = viewModel.planningActivity.objectWillChange.sink {
            planningPublications += 1
        }

        viewModel.generationActivity.setVideoProgress(
            .init(status: "in_progress", percent: 42),
            for: key
        )
        viewModel.generationActivity.setVideoProgress(
            .init(status: "in_progress", percent: 42),
            for: key
        )
        viewModel.planningActivity.appendModelText("token")

        XCTAssertEqual(rootPublications, 0)
        XCTAssertEqual(generationPublications, 1, "Duplicate provider callbacks should be coalesced")
        XCTAssertEqual(planningPublications, 1)
        XCTAssertEqual(viewModel.videoProgress("segment-1", projectID: key.projectID)?.percent, 42)
        XCTAssertEqual(viewModel.streamingModelText, "token")
        withExtendedLifetime((rootSubscription, generationSubscription, planningSubscription)) {}
    }

    func testActivityResetClearsVideoBatchAndPlanningState() {
        let viewModel = StoryStudioViewModel(media: StoryTestService())
        let key = StoryStudioViewModel.VideoGenerationKey(
            projectID: UUID(),
            segmentID: "segment-1"
        )
        viewModel.generationActivity.setBatchProgress(.init(status: "queued"))
        viewModel.generationActivity.setVideoProgress(.init(status: "running", percent: 7), for: key)
        viewModel.planningActivity.appendModelText("partial")
        viewModel.planningActivity.appendToolName("story_")

        viewModel.reset()

        XCTAssertNil(viewModel.progress)
        XCTAssertTrue(viewModel.videoGenerationProgress.isEmpty)
        XCTAssertTrue(viewModel.streamingModelText.isEmpty)
        XCTAssertNil(viewModel.streamingToolName)
    }

    func testStreamingTokenBurstOnlyPublishesTheRootWhenItsPhaseChanges() {
        let viewModel = StoryStudioViewModel(media: StoryTestService())
        viewModel.operation = "文本模型正在流式输出…"
        var rootPublications = 0
        var planningPublications = 0
        let rootSubscription = viewModel.objectWillChange.sink { rootPublications += 1 }
        let planningSubscription = viewModel.planningActivity.objectWillChange.sink {
            planningPublications += 1
        }

        for _ in 0..<200 {
            viewModel.receiveModelStream(.textDelta("x"), token: viewModel.session)
        }

        XCTAssertEqual(rootPublications, 0)
        XCTAssertEqual(planningPublications, 200)
        XCTAssertEqual(viewModel.streamingModelText.count, 200)
        withExtendedLifetime((rootSubscription, planningSubscription)) {}
    }
}
