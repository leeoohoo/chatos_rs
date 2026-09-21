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
        var job: ProviderVideoJob
        if let existingJobID {
            let url = try Self.videoJobEndpoint(
                baseURL: runtime.baseURL ?? "", jobID: existingJobID
            )
            job = try await sendVideoJobRequest(.init(
                url: url, method: "GET", headers: Self.providerHeaders(runtime: runtime)
            ))
        } else {
            try Self.validateUnifiedVideoRequest(
                request, profile: .init(modelName: runtime.model)
            )
            let unifiedMedia: UnifiedVideoMedia?
            if request.inputImage != nil || request.lastFrameImage != nil
                || request.referenceVideo != nil || request.referenceAudio != nil {
                await progress(.init(status: "uploading"))
                unifiedMedia = try await uploadUnifiedVideoMedia(request)
            } else {
                unifiedMedia = nil
            }
            let createRequest = try Self.makeVideoCreateRequest(
                runtime: runtime, request: request, unifiedMedia: unifiedMedia
            )
            job = try await sendVideoJobRequest(createRequest, isCreation: true)
        }
        await progress(.init(status: job.status, percent: job.progress, jobID: job.id))

        var pollCount = 0
        while job.isPending {
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

        guard job.status == "completed" else {
            throw MediaGenerationClientError.videoFailed(job.errorMessage)
        }
        try Task.checkCancellation()
        guard let contentURL = job.contentURL,
              contentURL.scheme?.lowercased() == "https", contentURL.host != nil,
              contentURL.user == nil, contentURL.password == nil else {
            throw MediaGenerationClientError.invalidProviderResponse
        }
        // Signed CDN URLs authorize themselves; never forward the NewAPI token.
        let downloadHeaders = ["Accept": "video/mp4"]
        await progress(.init(status: "downloading"))
        let contentResponse = try await sendProviderRequest(
            HTTPRequest(
                url: contentURL,
                method: "GET",
                headers: downloadHeaders,
                timeoutInterval: 10 * 60
            )
        )
        guard (200..<300).contains(contentResponse.statusCode) else {
            throw MediaGenerationClientError.providerRejected(
                statusCode: contentResponse.statusCode,
                detail: "下载视频内容（\(contentURL.host ?? "")\(contentURL.path)）：\(Self.providerErrorDetail(contentResponse.body))"
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

    private func uploadUnifiedVideoMedia(
        _ request: VideoGenerationRequest
    ) async throws -> UnifiedVideoMedia {
        var assets: [UnifiedVideoUploadAsset] = []
        if let image = request.inputImage {
            assets.append(.init(
                role: .firstFrame, name: image.name, mimeType: image.mimeType,
                base64Data: image.base64Data
            ))
        }
        if let image = request.lastFrameImage {
            assets.append(.init(
                role: .lastFrame, name: image.name, mimeType: image.mimeType,
                base64Data: image.base64Data
            ))
        }
        if let video = request.referenceVideo {
            assets.append(.init(
                role: .referenceVideo, name: video.name, mimeType: video.mimeType,
                base64Data: video.base64Data
            ))
        }
        if let audio = request.referenceAudio {
            assets.append(.init(
                role: .referenceAudio, name: audio.name, mimeType: audio.mimeType,
                base64Data: audio.base64Data
            ))
        }
        guard !assets.isEmpty else { return .init() }

        let decoded: [(asset: UnifiedVideoUploadAsset, data: Data)]
        do {
            decoded = try assets.map { asset in
                guard let data = Data(base64Encoded: asset.base64Data), !data.isEmpty else {
                    throw MediaGenerationClientError.mediaUploadFailed("“\(asset.name)”内容无效。")
                }
                return (asset, data)
            }
        } catch let error as MediaGenerationClientError {
            throw error
        } catch {
            throw MediaGenerationClientError.mediaUploadFailed(error.localizedDescription)
        }

        let uploadRequest = UnifiedVideoUploadsRequest(assets: decoded.map {
            .init(name: $0.asset.name, mimeType: $0.asset.mimeType, size: $0.data.count)
        })
        let uploadResponse: UnifiedVideoUploadsResponse
        do {
            uploadResponse = try await client.request(
                "/media/uploads", method: "POST",
                body: try JSONEncoder().encode(uploadRequest),
                expectedAuthenticationSessionID: authenticationSessionID
            )
        } catch {
            throw MediaGenerationClientError.mediaUploadFailed(error.localizedDescription)
        }
        guard uploadResponse.uploads.count == decoded.count else {
            throw MediaGenerationClientError.mediaUploadFailed("上传地址数量与素材数量不一致。")
        }

        var media = UnifiedVideoMedia()
        for ((asset, data), target) in zip(decoded, uploadResponse.uploads) {
            try Task.checkCancellation()
            guard let uploadURL = URL(string: target.uploadURL),
                  ["https", "http"].contains(uploadURL.scheme?.lowercased() ?? ""),
                  uploadURL.host != nil else {
                throw MediaGenerationClientError.mediaUploadFailed("对象存储上传地址无效。")
            }
            var headers = (target.uploadHeaders ?? [:]).filter { key, _ in
                key.caseInsensitiveCompare("Host") != .orderedSame
                    && key.caseInsensitiveCompare("Content-Length") != .orderedSame
            }
            if !headers.keys.contains(where: { $0.caseInsensitiveCompare("Content-Type") == .orderedSame }) {
                headers["Content-Type"] = asset.mimeType
            }
            let response: HTTPResponse
            do {
                response = try await sendProviderRequest(.init(
                    url: uploadURL, method: "PUT", headers: headers, body: data,
                    timeoutInterval: 10 * 60
                ))
            } catch {
                throw MediaGenerationClientError.mediaUploadFailed(error.localizedDescription)
            }
            guard (200..<300).contains(response.statusCode) else {
                throw MediaGenerationClientError.mediaUploadFailed(
                    "“\(asset.name)”上传失败（HTTP \(response.statusCode)）。"
                )
            }
            guard let publicURL = await client.resolvePublicURL(target.url),
                  ["https", "http"].contains(publicURL.scheme?.lowercased() ?? ""),
                  publicURL.host != nil, publicURL.user == nil, publicURL.password == nil else {
                throw MediaGenerationClientError.mediaUploadFailed("对象存储没有返回公网读取地址。")
            }
            switch asset.role {
            case .firstFrame: media.firstFrameURL = publicURL
            case .lastFrame: media.lastFrameURL = publicURL
            case .referenceVideo: media.referenceVideoURL = publicURL
            case .referenceAudio: media.referenceAudioURL = publicURL
            }
        }
        return media
    }

    private static func makeVideoCreateRequest(
        runtime: RuntimeModelConfig,
        request: VideoGenerationRequest,
        unifiedMedia: UnifiedVideoMedia? = nil
    ) throws -> HTTPRequest {
        let profile = VideoGenerationProfile(modelName: runtime.model)
        let endpoint = try videoCreateEndpoint(baseURL: runtime.baseURL ?? "")
        var headers = providerHeaders(runtime: runtime)
        headers["Content-Type"] = "application/json"
        let payload = try unifiedVideoPayload(
            runtime: runtime, request: request, profile: profile, media: unifiedMedia
        )
        return HTTPRequest(
            url: endpoint,
            method: "POST",
            headers: headers,
            body: try JSONSerialization.data(withJSONObject: payload),
            timeoutInterval: 120
        )
    }

    private static func unifiedVideoPayload(
        runtime: RuntimeModelConfig,
        request: VideoGenerationRequest,
        profile: VideoGenerationProfile,
        media: UnifiedVideoMedia?
    ) throws -> [String: Any] {
        try validateUnifiedVideoRequest(request, profile: profile)
        if request.inputImage != nil, media?.firstFrameURL == nil {
            throw MediaGenerationClientError.mediaUploadFailed("首帧没有可用的公网地址。")
        }
        if request.lastFrameImage != nil, media?.lastFrameURL == nil {
            throw MediaGenerationClientError.mediaUploadFailed("尾帧没有可用的公网地址。")
        }
        if request.referenceVideo != nil, media?.referenceVideoURL == nil {
            throw MediaGenerationClientError.mediaUploadFailed("参考视频没有可用的公网地址。")
        }
        if request.referenceAudio != nil, media?.referenceAudioURL == nil {
            throw MediaGenerationClientError.mediaUploadFailed("参考音频没有可用的公网地址。")
        }

        let duration = profile.isSeedance && request.referencePurpose == .edit
            ? -1 : request.seconds
        var metadata: [String: Any] = [
            "ratio": request.referencePurpose == .reference
                ? (request.inputImage == nil ? request.ratio : "adaptive") : "adaptive",
        ]
        if let value = media?.firstFrameURL { metadata["first_frame_image"] = value.absoluteString }
        if let value = media?.lastFrameURL { metadata["last_frame_image"] = value.absoluteString }
        if let value = media?.referenceVideoURL { metadata["video_url"] = value.absoluteString }
        if let value = media?.referenceAudioURL { metadata["audio_url"] = value.absoluteString }
        if profile.isSeedance {
            metadata["generate_audio"] = true
            if request.referenceVideo != nil {
                metadata["omni_reference_task_type"] = switch request.referencePurpose {
                case .reference: "reference"
                case .edit: "edit"
                case .extend: "extend"
                }
            }
            if profile == .seedance25 { metadata["output_format"] = "mp4" }
        }
        return [
            "model": runtime.model,
            "prompt": profile.isSeedance
                ? seedancePrompt(request.prompt, purpose: request.referencePurpose)
                : request.prompt.trimmingCharacters(in: .whitespacesAndNewlines),
            "duration": duration,
            "size": request.size.lowercased(),
            "metadata": metadata,
        ]
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
            if profile.isMiniMax { try validateMiniMaxImage(image) }
            else if profile.isSeedance { try validateSeedanceImage(image) }
            else { try validateGenericVideoImage(image) }
        }
        if let image = request.lastFrameImage {
            guard request.inputImage != nil, profile.supportsLastFrame else {
                throw MediaGenerationClientError.unsupportedLastFrameProtocol
            }
            if profile.isMiniMax { try validateMiniMaxImage(image) }
            else if profile.isSeedance { try validateSeedanceImage(image) }
            else { try validateGenericVideoImage(image) }
        }
        if let video = request.referenceVideo {
            guard profile.supportsReferenceVideo else {
                throw MediaGenerationClientError.unsupportedReferenceVideoProtocol
            }
            if profile.isMiniMax { try validateMiniMaxReferenceVideo(video) }
            else { try validateSeedanceReferenceVideo(video) }
        }
        if let audio = request.referenceAudio {
            guard profile.supportsReferenceVideo else {
                throw MediaGenerationClientError.unsupportedReferenceVideoProtocol
            }
            try validateReferenceAudio(audio)
        }
        if profile.isMiniMax {
            let prompt = request.prompt.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !prompt.isEmpty, prompt.unicodeScalars.count <= 7_000 else {
                throw MediaGenerationClientError.invalidMiniMaxPrompt
            }
            guard request.inputImage != nil
                    || VideoGenerationProfile.miniMaxRatios.contains(request.ratio) else {
                throw MediaGenerationClientError.invalidVideoOptions
            }
        }
    }

    private static func seedancePrompt(
        _ prompt: String, purpose: VideoGenerationReferencePurpose
    ) -> String {
        let value = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        switch purpose {
        case .reference:
            return value
        case .edit:
            return "视频编辑：以@视频1为原视频，严格执行以下修改，未提及的主体、场景与镜头尽量保持不变。\n\(value)"
        case .extend:
            return "延长@视频1：从原视频结尾自然继续，保持人物、场景、动作方向、光线与运镜连贯，不要重复原视频已有内容。\n\(value)"
        }
    }

    private static func validateSeedanceImage(_ image: ImageGenerationInputImage) throws {
        let mimeType = image.mimeType.lowercased()
        guard ["image/png", "image/jpeg", "image/webp"].contains(mimeType),
              let data = Data(base64Encoded: image.base64Data), !data.isEmpty,
              data.count <= 30 * 1024 * 1024 else {
            throw MediaGenerationClientError.invalidInputImage
        }
    }

    private static func validateGenericVideoImage(_ image: ImageGenerationInputImage) throws {
        let mimeType = image.mimeType.lowercased()
        guard ["image/png", "image/jpeg", "image/webp"].contains(mimeType),
              let data = Data(base64Encoded: image.base64Data), !data.isEmpty,
              data.count <= 20 * 1024 * 1024 else {
            throw MediaGenerationClientError.invalidInputImage
        }
    }

    private static func validateSeedanceReferenceVideo(_ video: VideoGenerationInputVideo) throws {
        guard ["video/mp4", "video/quicktime"].contains(video.mimeType.lowercased()),
              let data = Data(base64Encoded: video.base64Data), !data.isEmpty,
              data.count <= 50 * 1024 * 1024 else {
            throw MediaGenerationClientError.invalidReferenceVideo
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
        let status = rawStatus.lowercased()
        let progress: Double?
        if let value = root["progress"] as? Double {
            progress = value
        } else if let value = root["progress"] as? Int {
            progress = Double(value)
        } else {
            progress = nil
        }
        let errorMessage: String?
        if let error = root["error"] as? [String: Any] {
            errorMessage = error["message"] as? String
        } else {
            errorMessage = nil
        }
        let metadata = root["metadata"] as? [String: Any]
        return ProviderVideoJob(
            id: id,
            status: status,
            progress: progress,
            model: root["model"] as? String,
            errorMessage: errorMessage,
            contentURL: (metadata?["url"] as? String).flatMap(URL.init(string:))
        )
    }

}

private struct UnifiedVideoMedia {
    var firstFrameURL: URL?
    var lastFrameURL: URL?
    var referenceVideoURL: URL?
    var referenceAudioURL: URL?
}

private struct UnifiedVideoUploadAsset {
    enum Role { case firstFrame, lastFrame, referenceVideo, referenceAudio }

    var role: Role
    var name: String
    var mimeType: String
    var base64Data: String
}

private struct UnifiedVideoUploadsRequest: Encodable {
    var assets: [Asset]

    struct Asset: Encodable {
        var name: String
        var mimeType: String
        var size: Int
    }
}

private struct UnifiedVideoUploadsResponse: Decodable, Sendable {
    var uploads: [Upload]

    struct Upload: Decodable, Sendable {
        var uploadURL: String
        var uploadHeaders: [String: String]?
        var url: String

        enum CodingKeys: String, CodingKey {
            case url, uploadHeaders
            case uploadURL = "uploadUrl"
        }
    }
}
