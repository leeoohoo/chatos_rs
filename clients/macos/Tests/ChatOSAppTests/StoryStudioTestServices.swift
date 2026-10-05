import AppKit
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import XCTest
@testable import ChatOSApp

actor ConcurrentStoryImageService: MediaGenerationServicing {
    private let imageData: Data
    private var activeCalls = 0
    private var maximumCalls = 0
    private var calls: [String: Int] = [:]
    private var continuations: [String: CheckedContinuation<Void, Never>] = [:]

    init(imageData: Data) { self.imageData = imageData }

    func fetchModels() async throws -> [MediaGenerationModel] { [] }

    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        guard let resourceID = request.resourceID else { throw StoryError.invalidPlan }
        calls[resourceID, default: 0] += 1
        activeCalls += 1
        maximumCalls = max(maximumCalls, activeCalls)
        await withCheckedContinuation { continuation in continuations[resourceID] = continuation }
        activeCalls -= 1
        return .init(id: "result-\(resourceID)", modelConfigID: request.modelConfigID,
                     modelName: "image-model", createdAt: "", images: [
                        .init(id: "asset-\(resourceID)", mimeType: "image/png", base64Data: imageData.base64EncodedString()),
                     ], clientRequestID: request.clientRequestID, projectID: request.projectID, resourceID: resourceID)
    }

    func generateVideo(_ request: VideoGenerationRequest,
                       progress: @escaping @Sendable (VideoGenerationProgress) async -> Void) async throws -> VideoGenerationResult {
        throw StoryError.unavailable
    }

    func maximumConcurrentCalls() -> Int { maximumCalls }
    func callCount(resourceID: String) -> Int { calls[resourceID, default: 0] }
    func finish(resourceID: String) { continuations.removeValue(forKey: resourceID)?.resume() }
}

actor RecordingFrameImageService: MediaGenerationServicing {
    private let imageData: Data
    private var request: ImageGenerationRequest?
    init(imageData: Data) { self.imageData = imageData }
    func fetchModels() async throws -> [MediaGenerationModel] { [] }
    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        self.request = request
        return .init(id: "frame-result", modelConfigID: request.modelConfigID, modelName: "image-model",
                     createdAt: "", images: [.init(id: "frame-image", mimeType: "image/png",
                                                    base64Data: imageData.base64EncodedString())],
                     clientRequestID: request.clientRequestID, projectID: request.projectID,
                     resourceID: request.resourceID)
    }
    func generateVideo(_ request: VideoGenerationRequest,
                       progress: @escaping @Sendable (VideoGenerationProgress) async -> Void) async throws -> VideoGenerationResult {
        throw StoryError.unavailable
    }
    func lastRequest() -> ImageGenerationRequest? { request }
}

struct StoryTestPreflightError: LocalizedError, MediaGenerationSubmissionFailure {
    var errorDescription: String? { "preflight rejected" }
    var requestMayHaveBeenSubmitted: Bool { false }
}

actor StoryTestService: ResumableVideoGenerationServicing, StoryPlanningServicing {
    private var calls: [String] = []
    private var videoRequests: [VideoGenerationRequest] = []
    private var failVideo = false
    private var preflightVideoFailure = false
    private var delayed = false
    private var activeVideoCalls = 0
    private var maximumVideoCalls = 0
    func events() -> [String] { calls }
    func setVideoFailure(_ value: Bool) { failVideo = value }
    func setPreflightVideoFailure(_ value: Bool) { preflightVideoFailure = value }
    func setDelay(_ value: Bool) { delayed = value }
    func maximumConcurrentVideoCalls() -> Int { maximumVideoCalls }
    func lastVideoRequest() -> VideoGenerationRequest? { videoRequests.last }
    func fetchModels() async throws -> [MediaGenerationModel] { [] }
    func plan(_ request: StoryPlanningRequest) async throws -> Data {
        calls.append(request.toolName)
        if delayed { try await Task.sleep(for: .milliseconds(50)) }
        if request.toolName == "story_suggest_optimized_text" {
            return Data(#"{"optimizedText":"Improved candidate","rationale":"Clearer pacing"}"#.utf8)
        }
        if request.toolName == "story_save_outline" {
            return Data(#"{"summary":"beginning to ending","props":[],"segments":[{"id":"s1","title":"opening","synopsis":"start","kind":"story","seconds":15,"propIDs":[]},{"id":"s2","title":"middle","synopsis":"middle","kind":"story","seconds":15,"propIDs":[]},{"id":"s3","title":"ending","synopsis":"end","kind":"story","seconds":15,"propIDs":[]}]}"#.utf8)
        }
        return Data(#"{"firstFramePrompt":"a girl","lastFramePrompt":"girl at the exit","shots":[{"start":0,"end":15,"prompt":"tracking shot"}],"continuityIn":"enter","continuityOut":"exit","audio":"wind","constraints":"same character"}"#.utf8)
    }
    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        calls.append("image"); throw URLError(.badServerResponse)
    }
    func generateVideo(_ request: VideoGenerationRequest, progress: @escaping @Sendable (VideoGenerationProgress) async -> Void) async throws -> VideoGenerationResult {
        calls.append("video")
        videoRequests.append(request)
        if preflightVideoFailure { throw StoryTestPreflightError() }
        activeVideoCalls += 1
        maximumVideoCalls = max(maximumVideoCalls, activeVideoCalls)
        defer { activeVideoCalls -= 1 }
        await progress(.init(status: "queued", jobID: "remote-job"))
        if delayed { try await Task.sleep(for: .milliseconds(80)) }
        if failVideo { throw URLError(.networkConnectionLost) }
        return result(request)
    }
    func resumeVideo(_ request: VideoGenerationRequest, jobID: String, progress: @escaping @Sendable (VideoGenerationProgress) async -> Void) async throws -> VideoGenerationResult {
        calls.append("resume"); return result(request)
    }
    private func result(_ request: VideoGenerationRequest) -> VideoGenerationResult {
        .init(id: "remote-job", modelConfigID: request.modelConfigID, modelName: "MiniMax-H3", createdAt: "", mimeType: "video/mp4", videoData: Data("video".utf8))
    }
}
