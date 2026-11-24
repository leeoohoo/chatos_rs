import ChatOSCore

extension MediaStudioViewModel {
    var videoProfile: VideoGenerationProfile {
        VideoGenerationProfile(modelName: selectedVideoModel?.modelName ?? "")
    }

    func normalizeVideoOptions() {
        if !videoProfile.sizes.contains(videoSize) { videoSize = videoProfile.sizes[0] }
        if !videoProfile.durations.contains(videoSeconds) { videoSeconds = videoProfile.durations[0] }
    }
}
