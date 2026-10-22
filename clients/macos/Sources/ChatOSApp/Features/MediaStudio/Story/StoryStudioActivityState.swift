import ChatOSCore
import Combine

/// High-frequency provider callbacks live outside `StoryStudioViewModel` so a video
/// percentage tick does not invalidate the whole story workbench.
@MainActor
final class StoryGenerationActivityState: ObservableObject {
    @Published private(set) var batchProgress: VideoGenerationProgress?
    @Published private(set) var videoProgress: [StoryStudioViewModel.VideoGenerationKey: VideoGenerationProgress] = [:]

    func setBatchProgress(_ value: VideoGenerationProgress?) {
        guard batchProgress != value else { return }
        batchProgress = value
    }

    func setVideoProgress(
        _ value: VideoGenerationProgress?,
        for key: StoryStudioViewModel.VideoGenerationKey
    ) {
        guard videoProgress[key] != value else { return }
        videoProgress[key] = value
    }

    func reset() {
        batchProgress = nil
        videoProgress = [:]
    }
}

/// Token streaming is rendered by the small agent activity panel. Keeping it in a
/// separate observation domain avoids rebuilding the project editor for every token.
@MainActor
final class StoryPlanningActivityState: ObservableObject {
    @Published private(set) var modelText = ""
    @Published private(set) var toolName: String?

    func beginResponse() {
        modelText = ""
        toolName = nil
    }

    func appendModelText(_ delta: String, maximumLength: Int = 12_000) {
        guard !delta.isEmpty else { return }
        modelText += delta
        if modelText.count > maximumLength {
            modelText.removeFirst(modelText.count - maximumLength)
        }
    }

    func appendToolName(_ fragment: String?) {
        guard let fragment, !fragment.isEmpty else { return }
        toolName = (toolName ?? "") + fragment
    }

    func reset() {
        modelText = ""
        toolName = nil
    }
}
