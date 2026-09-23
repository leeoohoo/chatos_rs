import ChatOSCore
import Foundation
import ImageIO

public struct ChatOSMediaGenerationService: ResumableVideoGenerationServicing, SessionBoundMediaGenerationServicing, Sendable {
    private let client: ChatOSAPIClient
    private let providerTransport: any HTTPTransport
    private let videoPollIntervalNanoseconds: UInt64
    private let maximumVideoPollCount: Int
    private var authenticationSessionID: UUID?

    public func boundToCurrentSession() async throws -> any MediaGenerationServicing {
        var bound = self
        bound.authenticationSessionID = try await client.currentAuthenticationSessionID()
        return bound
    }

    private func sendProviderRequest(_ request: HTTPRequest) async throws -> HTTPResponse {
        try Task.checkCancellation()
        if let authenticationSessionID {
            guard try await client.currentAuthenticationSessionID() == authenticationSessionID else { throw ChatOSAPIError.unauthorized }
        }
        let response = try await providerTransport.send(request)
        // A completed submission still belongs to the original account. Never make any
        // further provider request after a switch; the caller retains its durable intent.
        if let authenticationSessionID {
            guard try await client.currentAuthenticationSessionID() == authenticationSessionID else { throw ChatOSAPIError.unauthorized }
        }
        return response
    }

    public init(
        client: ChatOSAPIClient,
        providerTransport: any HTTPTransport = URLSessionHTTPTransport(),
        videoPollIntervalNanoseconds: UInt64 = 10_000_000_000,
        maximumVideoPollCount: Int = 180
    ) {
        self.client = client
        self.providerTransport = providerTransport
        self.videoPollIntervalNanoseconds = videoPollIntervalNanoseconds
        self.maximumVideoPollCount = maximumVideoPollCount
    }

    public func fetchModels() async throws -> [MediaGenerationModel] {
        let models: [MediaGenerationModel] = try await client.request("/ai-model-configs", expectedAuthenticationSessionID: authenticationSessionID)
        return models
            .filter { $0.enabled && $0.hasAPIKey }
            .sorted { left, right in
                let leftImageRank = Self.imageModelRank(left)
                let rightImageRank = Self.imageModelRank(right)
                if leftImageRank != rightImageRank {
                    return leftImageRank < rightImageRank
                }
                return left.name.localizedCaseInsensitiveCompare(right.name) == .orderedAscending
            }
    }

    public func generateImage(
        _ request: ImageGenerationRequest
    ) async throws -> ImageGenerationResult {
        let runtime = try await loadRuntimeModel(id: request.modelConfigID)
        let providerRequest = try Self.makeProviderRequest(runtime: runtime, request: request)
        let response = try await sendProviderRequest(providerRequest)

        guard response.body.count <= 48 * 1024 * 1024 else {
            throw MediaGenerationClientError.responseTooLarge
        }
        guard (200..<300).contains(response.statusCode) else {
            throw MediaGenerationClientError.providerRejected(
                statusCode: response.statusCode,
                detail: Self.providerErrorDetail(response.body)
            )
        }

        let payload: ProviderImageResponse
        do {
            payload = try JSONDecoder().decode(ProviderImageResponse.self, from: response.body)
        } catch {
            throw MediaGenerationClientError.invalidProviderResponse
        }
        let resultID = payload.id ?? request.clientRequestID ?? "media_\(UUID().uuidString.lowercased())"
        let images = try payload.data.enumerated().map { index, item -> GeneratedMediaAsset in
            let assetID = item.id ?? "\(resultID):image:\(index)"
            if let base64Data = item.b64JSON ?? item.base64Data {
                guard let decoded = Data(base64Encoded: base64Data),
                      decoded.count <= 20 * 1024 * 1024 else {
                    throw MediaGenerationClientError.invalidProviderResponse
                }
                return GeneratedMediaAsset(
                    id: assetID,
                    mimeType: item.mimeType ?? "image/png",
                    base64Data: base64Data,
                    revisedPrompt: item.revisedPrompt
                )
            }
            if let url = item.url, url.scheme?.lowercased() == "https" {
                return GeneratedMediaAsset(
                    id: assetID,
                    mimeType: item.mimeType ?? "image/png",
                    url: url,
                    revisedPrompt: item.revisedPrompt
                )
            }
            throw MediaGenerationClientError.invalidProviderResponse
        }
        guard !images.isEmpty else {
            throw MediaGenerationClientError.invalidProviderResponse
        }

        return ImageGenerationResult(
            id: resultID,
            modelConfigID: request.modelConfigID,
            modelName: payload.model ?? runtime.model,
            createdAt: ISO8601DateFormatter().string(from: Date()),
            images: images,
            clientRequestID: request.clientRequestID,
            projectID: request.projectID,
            resourceID: request.resourceID
        )
    }

    public func generateVideo(
        _ request: VideoGenerationRequest,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult {
        try await runVideo(request, existingJobID: nil, progress: progress)
    }

    public func resumeVideo(
        _ request: VideoGenerationRequest, jobID: String,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult {
        guard !jobID.isEmpty else { throw MediaGenerationClientError.invalidProviderResponse }
        return try await runVideo(request, existingJobID: jobID, progress: progress)
    }

    private func runVideo(
        _ request: VideoGenerationRequest, existingJobID: String?,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult {
        let runtime: RuntimeModelConfig
        do {
            runtime = try await loadRuntimeModel(id: request.modelConfigID)
        } catch {
            throw MediaGenerationClientError.preflightFailed(error.localizedDescription)
        }
        let profile = VideoGenerationProfile(modelName: runtime.model)
        guard profile == .miniMaxH3 else {
            throw MediaGenerationClientError.unsupportedVideoModel(runtime.model)
        }
        var job: ProviderVideoJob
        if let existingJobID {
            let url = try Self.videoJobEndpoint(
                baseURL: runtime.baseURL ?? "", jobID: existingJobID
            )
            job = try await sendVideoJobRequest(.init(
                url: url, method: "GET", headers: Self.providerHeaders(runtime: runtime)
            ))
        } else {
            try Self.validateUnifiedVideoRequest(request, profile: profile)
            let createRequest = try Self.makeVideoCreateRequest(
                runtime: runtime, request: request
            )
            job = try await sendVideoJobRequest(createRequest, isCreation: true)
        }
        await progress(.init(status: job.status, percent: job.progress, jobID: job.id))

        var pollCount = 0
        while job.shouldPoll {
            try Task.checkCancellation()
            guard pollCount < maximumVideoPollCount else {
                throw MediaGenerationClientError.videoTimedOut
            }
            pollCount += 1
            if videoPollIntervalNanoseconds > 0 {
                try await Task.sleep(nanoseconds: videoPollIntervalNanoseconds)
            }
            let statusURL = try Self.videoJobEndpoint(
                baseURL: runtime.baseURL ?? "", jobID: job.id
            )
            job = try await sendVideoJobRequest(
                HTTPRequest(
                    url: statusURL,
                    method: "GET",
                    headers: Self.providerHeaders(runtime: runtime),
                    timeoutInterval: 60
                )
            )
            await progress(.init(status: job.status, percent: job.progress, jobID: job.id))
        }

        guard !job.isFailed else {
            throw MediaGenerationClientError.videoFailed(job.errorMessage)
        }
        guard job.isCompleted else { throw MediaGenerationClientError.invalidProviderResponse }
        try Task.checkCancellation()
        let contentRequest: HTTPRequest
        if job.hasMetadataContentURL {
            guard let contentURL = job.contentURL,
                  contentURL.scheme?.lowercased() == "https", contentURL.host != nil,
                  contentURL.user == nil, contentURL.password == nil else {
                throw MediaGenerationClientError.invalidProviderResponse
            }
            // Signed CDN URLs authorize themselves; never forward the NewAPI token.
            contentRequest = HTTPRequest(
                url: contentURL,
                method: "GET",
                headers: ["Accept": "video/mp4"],
                timeoutInterval: 10 * 60
            )
        } else {
            contentRequest = HTTPRequest(
                url: try Self.videoContentEndpoint(
                    baseURL: runtime.baseURL ?? "", jobID: job.id
                ),
                method: "GET",
                headers: Self.providerHeaders(runtime: runtime, accept: "video/mp4"),
                timeoutInterval: 10 * 60
            )
        }
        await progress(.init(status: "downloading"))
        let contentResponse = try await sendProviderRequest(contentRequest)
        guard (200..<300).contains(contentResponse.statusCode) else {
            throw MediaGenerationClientError.providerRejected(
                statusCode: contentResponse.statusCode,
                detail: "下载视频内容（\(contentRequest.url.host ?? "")\(contentRequest.url.path)）：\(Self.providerErrorDetail(contentResponse.body))"
            )
        }
        guard !contentResponse.body.isEmpty,
              contentResponse.body.count <= 512 * 1024 * 1024 else {
            throw MediaGenerationClientError.invalidVideoContent
        }
        let mimeType = contentResponse.headers["content-type"]?
            .split(separator: ";")
            .first
            .map(String.init) ?? "video/mp4"
        return VideoGenerationResult(
            id: job.id,
            modelConfigID: request.modelConfigID,
            modelName: job.model ?? runtime.model,
            createdAt: ISO8601DateFormatter().string(from: Date()),
            mimeType: mimeType,
            videoData: contentResponse.body
        )
    }

    private func sendVideoJobRequest(
        _ request: HTTPRequest,
        isCreation: Bool = false
    ) async throws -> ProviderVideoJob {
        let response = try await sendProviderRequest(request)
        guard response.body.count <= 2 * 1024 * 1024 else {
            throw MediaGenerationClientError.responseTooLarge
        }
        guard (200..<300).contains(response.statusCode) else {
            throw MediaGenerationClientError.providerRejected(
                statusCode: response.statusCode,
                detail: "\(isCreation ? "创建视频任务" : "查询视频任务")（\(request.url.host ?? "")\(request.url.path)）：\(Self.providerErrorDetail(response.body))"
            )
        }
        let prefix = String(decoding: response.body.prefix(256), as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if response.headers["content-type"]?.lowercased().contains("text/html") == true
            || prefix.hasPrefix("<!doctype html") || prefix.hasPrefix("<html") {
            throw MediaGenerationClientError.videoEndpointReturnedHTML("\(request.url.host ?? "")\(request.url.path)")
        }
        return try Self.decodeVideoJob(response.body)
    }

    private func loadRuntimeModel(id: String) async throws -> RuntimeModelConfig {
        let encodedID = id.urlPathEncoded
        let runtime: RuntimeModelConfig = try await client.request(
            "/ai-model-configs/\(encodedID)?include_secret=true", expectedAuthenticationSessionID: authenticationSessionID
        )
        guard runtime.enabled,
              let apiKey = runtime.apiKey?.trimmingCharacters(in: .whitespacesAndNewlines),
              !apiKey.isEmpty,
              let baseURLText = runtime.baseURL?.trimmingCharacters(in: .whitespacesAndNewlines),
              !baseURLText.isEmpty,
              URL(string: baseURLText) != nil,
              !runtime.model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw MediaGenerationClientError.invalidModelConfiguration
        }
        return runtime
    }

    private static func makeVideoCreateRequest(
        runtime: RuntimeModelConfig,
        request: VideoGenerationRequest
    ) throws -> HTTPRequest {
        let profile = VideoGenerationProfile(modelName: runtime.model)
        let endpoint = try videoCreateEndpoint(baseURL: runtime.baseURL ?? "")
        var headers = providerHeaders(runtime: runtime)
        headers["Content-Type"] = "application/json"
        let payload = try unifiedVideoPayload(
            runtime: runtime, request: request, profile: profile
        )
        return HTTPRequest(
            url: endpoint,
            method: "POST",
            headers: headers,
            body: try JSONSerialization.data(withJSONObject: payload),
            timeoutInterval: 10 * 60
        )
    }

    private static func unifiedVideoPayload(
        runtime: RuntimeModelConfig,
        request: VideoGenerationRequest,
        profile: VideoGenerationProfile
    ) throws -> [String: Any] {
        try validateUnifiedVideoRequest(request, profile: profile)
        var metadata: [String: Any] = [
            "ratio": request.referencePurpose == .reference
                ? (request.inputImage == nil ? request.ratio : "adaptive") : "adaptive",
        ]
        if let value = request.inputImage {
            metadata["first_frame_image"] = dataURL(
                mimeType: value.mimeType, base64Data: value.base64Data
            )
        }
        if let value = request.lastFrameImage {
            metadata["last_frame_image"] = dataURL(
                mimeType: value.mimeType, base64Data: value.base64Data
            )
        }
        if let value = request.referenceVideo {
            metadata["video_url"] = dataURL(
                mimeType: value.mimeType, base64Data: value.base64Data
            )
        }
        if let value = request.referenceAudio {
            metadata["audio_url"] = dataURL(
                mimeType: value.mimeType, base64Data: value.base64Data
            )
        }
        return [
            "model": runtime.model,
            "prompt": request.prompt.trimmingCharacters(in: .whitespacesAndNewlines),
            "duration": request.seconds,
            "size": request.size.lowercased(),
            "metadata": metadata,
        ]
    }

    private static func dataURL(mimeType: String, base64Data: String) -> String {
        "data:\(mimeType.lowercased());base64,\(base64Data)"
    }

    private static func validateUnifiedVideoRequest(
        _ request: VideoGenerationRequest,
        profile: VideoGenerationProfile
    ) throws {
        guard profile.sizes.contains(request.size),
              profile.durations.contains(request.seconds) else {
            throw MediaGenerationClientError.invalidVideoOptions
        }
        guard (request.referenceVideo == nil && request.referenceAudio == nil)
                || (request.inputImage == nil && request.lastFrameImage == nil) else {
            throw MediaGenerationClientError.mixedFrameAndReferenceVideoInputs
        }
        if request.referencePurpose != .reference, request.referenceVideo == nil {
            throw MediaGenerationClientError.missingReferenceVideo
        }
        if let image = request.inputImage {
            try validateMiniMaxImage(image)
        }
        if let image = request.lastFrameImage {
            guard request.inputImage != nil, profile.supportsLastFrame else {
                throw MediaGenerationClientError.unsupportedLastFrameProtocol
            }
            try validateMiniMaxImage(image)
        }
        if let video = request.referenceVideo {
            guard profile.supportsReferenceVideo else {
                throw MediaGenerationClientError.unsupportedReferenceVideoProtocol
            }
            try validateMiniMaxReferenceVideo(video)
        }
        if let audio = request.referenceAudio {
            guard profile.supportsReferenceVideo else {
                throw MediaGenerationClientError.unsupportedReferenceVideoProtocol
            }
            try validateReferenceAudio(audio)
        }
        let prompt = request.prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !prompt.isEmpty, prompt.unicodeScalars.count <= 7_000 else {
            throw MediaGenerationClientError.invalidMiniMaxPrompt
        }
        guard request.inputImage != nil
                || VideoGenerationProfile.miniMaxRatios.contains(request.ratio) else {
            throw MediaGenerationClientError.invalidVideoOptions
        }
    }

    private static func validateMiniMaxImage(_ image: ImageGenerationInputImage) throws {
        let mimeType = image.mimeType.lowercased()
        guard ["image/png", "image/jpeg", "image/webp"].contains(mimeType),
              let data = Data(base64Encoded: image.base64Data), !data.isEmpty,
              data.count <= 20 * 1024 * 1024 else {
            throw MediaGenerationClientError.invalidInputImage
        }
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? Int,
              let height = properties[kCGImagePropertyPixelHeight] as? Int,
              (256...5760).contains(width), (256...5760).contains(height),
              (0.4...2.5).contains(Double(width) / Double(height)) else {
            throw MediaGenerationClientError.invalidMiniMaxImageDimensions
        }
    }

    /// Base64 adds roughly one third to the request size. Keep the local source below
    /// 47 MiB so the complete JSON body remains under MiniMax's 64 MiB request limit.
    private static func validateMiniMaxReferenceVideo(_ video: VideoGenerationInputVideo) throws {
        guard ["video/mp4", "video/quicktime"].contains(video.mimeType.lowercased()),
              let data = Data(base64Encoded: video.base64Data), !data.isEmpty,
              data.count <= 47 * 1024 * 1024 else {
            throw MediaGenerationClientError.invalidReferenceVideo
        }
    }

    private static func validateReferenceAudio(_ audio: VideoGenerationInputAudio) throws {
        guard [
            "audio/mpeg", "audio/mp3", "audio/wav", "audio/x-wav", "audio/vnd.wave",
            "audio/mp4", "audio/x-m4a", "audio/aac",
        ].contains(audio.mimeType.lowercased()),
              let data = Data(base64Encoded: audio.base64Data), !data.isEmpty,
              data.count <= 20 * 1024 * 1024 else {
            throw MediaGenerationClientError.invalidReferenceAudio
        }
    }

    private static func providerHeaders(
        runtime: RuntimeModelConfig,
        accept: String = "application/json"
    ) -> [String: String] {
        [
            "Accept": accept,
            "Authorization": "Bearer \(runtime.apiKey ?? "")",
        ]
    }

    static func providerEndpoint(
        baseURL: String,
        operation: ProviderImageOperation
    ) throws -> URL {
        let base = normalizedProviderBaseURL(baseURL)
        guard let url = URL(string: "\(base)/images/\(operation.rawValue)") else {
            throw MediaGenerationClientError.invalidModelConfiguration
        }
        return url
    }

    private static func videoCreateEndpoint(baseURL: String) throws -> URL {
        guard let url = URL(string: "\(normalizedProviderBaseURL(baseURL))/videos") else {
            throw MediaGenerationClientError.invalidModelConfiguration
        }
        return url
    }

    private static func videoJobEndpoint(baseURL: String, jobID: String) throws -> URL {
        guard let url = URL(
            string: "\(normalizedProviderBaseURL(baseURL))/videos/\(jobID.urlPathEncoded)"
        ) else {
            throw MediaGenerationClientError.invalidModelConfiguration
        }
        return url
    }

    private static func videoContentEndpoint(baseURL: String, jobID: String) throws -> URL {
        guard let url = URL(
            string: "\(normalizedProviderBaseURL(baseURL))/videos/\(jobID.urlPathEncoded)/content"
        ) else {
            throw MediaGenerationClientError.invalidModelConfiguration
        }
        return url
    }

    private static func normalizedProviderBaseURL(_ rawValue: String) -> String {
        var value = rawValue.trimmingCharacters(in: .whitespacesAndNewlines)
        while value.hasSuffix("/") { value.removeLast() }
        let knownSuffixes = [
            "/images/generations", "/images/edits", "/chat/completions", "/responses",
        ]
        if let suffix = knownSuffixes.first(where: value.hasSuffix) {
            value.removeLast(suffix.count)
        }
        if let videosRange = value.range(of: "/videos/", options: .backwards) {
            value = String(value[..<videosRange.lowerBound])
        } else if value.hasSuffix("/videos") {
            value.removeLast("/videos".count)
        }
        return value
    }

    private static func decodeVideoJob(_ body: Data) throws -> ProviderVideoJob {
        guard let root = try? JSONSerialization.jsonObject(with: body) as? [String: Any] else {
            throw MediaGenerationClientError.invalidProviderResponse
        }
        guard
              let id = ((root["task_id"] ?? root["id"]) as? String)?
                .trimmingCharacters(in: .whitespacesAndNewlines),
              !id.isEmpty, let rawStatus = root["status"] as? String else {
            throw MediaGenerationClientError.invalidProviderResponse
        }
        let status = rawStatus.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let progress: Double?
        if let value = root["progress"] as? Double {
            progress = value
        } else if let value = root["progress"] as? Int {
            progress = Double(value)
        } else {
            progress = nil
        }
        let explicitError = root["error"].flatMap { $0 is NSNull ? nil : $0 }
        let errorMessage: String?
        if let error = explicitError as? [String: Any] {
            errorMessage = (error["message"] as? String) ?? (error["detail"] as? String)
        } else if let error = explicitError as? String {
            let trimmed = error.trimmingCharacters(in: .whitespacesAndNewlines)
            errorMessage = trimmed.isEmpty ? nil : trimmed
        } else {
            errorMessage = nil
        }
        let metadata = root["metadata"] as? [String: Any]
        let rawContentURL = metadata?["url"] as? String
        return ProviderVideoJob(
            id: id,
            status: status,
            progress: progress,
            model: root["model"] as? String,
            errorMessage: errorMessage,
            hasExplicitError: explicitError != nil,
            contentURL: rawContentURL.flatMap(URL.init(string:)),
            hasMetadataContentURL: rawContentURL != nil
        )
    }

}
