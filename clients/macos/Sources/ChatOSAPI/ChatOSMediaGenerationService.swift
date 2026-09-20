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
        let wireProtocol = runtime.videoProtocol
        var job: ProviderVideoJob
        if let existingJobID {
            let url = try Self.videoStatusEndpoint(
                baseURL: runtime.baseURL ?? "", jobID: existingJobID, wireProtocol: wireProtocol
            )
            job = try await sendVideoJobRequest(.init(url: url, method: "GET", headers: Self.providerHeaders(runtime: runtime)), wireProtocol: wireProtocol)
        } else {
            let createRequest = try Self.makeVideoCreateRequest(runtime: runtime, request: request)
            job = try await sendVideoJobRequest(createRequest, wireProtocol: wireProtocol, isCreation: true)
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
            let statusURL = try Self.videoStatusEndpoint(
                baseURL: runtime.baseURL ?? "", jobID: job.id, wireProtocol: wireProtocol
            )
            job = try await sendVideoJobRequest(
                HTTPRequest(
                    url: statusURL,
                    method: "GET",
                    headers: Self.providerHeaders(runtime: runtime),
                    timeoutInterval: 60
                ),
                wireProtocol: wireProtocol
            )
            await progress(.init(status: job.status, percent: job.progress, jobID: job.id))
        }

        guard job.status == "completed" else {
            throw MediaGenerationClientError.videoFailed(job.errorMessage)
        }
        try Task.checkCancellation()
        let contentURL: URL
        let downloadHeaders: [String: String]
        if wireProtocol == .miniMaxNative || wireProtocol == .volcengineArk {
            guard let url = job.contentURL, url.scheme?.lowercased() == "https",
                  url.host != nil, url.user == nil, url.password == nil else {
                throw MediaGenerationClientError.invalidProviderResponse
            }
            contentURL = url
            // Signed CDN URLs authorize themselves; never forward the provider key.
            downloadHeaders = ["Accept": "video/mp4"]
        } else {
            contentURL = try Self.videoContentEndpoint(baseURL: runtime.baseURL ?? "", jobID: job.id)
            downloadHeaders = Self.providerHeaders(runtime: runtime, accept: "video/mp4")
        }
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
        wireProtocol: VideoWireProtocol,
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
        return try Self.decodeVideoJob(response.body, wireProtocol: wireProtocol, isCreation: isCreation)
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
        if runtime.videoProtocol == .miniMaxNative {
            return try makeMiniMaxCreateRequest(runtime: runtime, request: request, profile: profile)
        }
        if runtime.videoProtocol == .volcengineArk {
            return try makeSeedanceCreateRequest(runtime: runtime, request: request, profile: profile)
        }
        guard profile.durations.contains(request.seconds), profile.sizes.contains(request.size) else {
            throw MediaGenerationClientError.invalidVideoOptions
        }
        if request.referenceVideo != nil, !profile.supportsReferenceVideo {
            throw MediaGenerationClientError.unsupportedReferenceVideoProtocol
        }
        if profile.isSeedance {
            if request.referenceVideo != nil {
                throw MediaGenerationClientError.unsupportedReferenceVideoProtocol
            }
            if request.lastFrameImage != nil {
                throw MediaGenerationClientError.unsupportedLastFrameProtocol
            }
        }
        let endpoint = try videoCreateEndpoint(baseURL: runtime.baseURL ?? "")
        var headers = providerHeaders(runtime: runtime)
        let body: Data
        if profile.isMiniMax {
            // NewAPI's /v1/videos compatibility layer accepts MiniMax V2's top-level
            // content array. Preserve the official first_frame/last_frame roles instead
            // of flattening the first image into input_reference and dropping the tail.
            let validated = try miniMaxPayload(runtime: runtime, request: request, profile: profile)
            let payload: [String: Any] = [
                "model": runtime.model,
                "content": validated["content"] as Any,
                "duration": request.seconds,
                "size": request.size,
                "metadata": ["ratio": request.inputImage == nil ? request.ratio : "adaptive"],
            ]
            headers["Content-Type"] = "application/json"
            body = try JSONSerialization.data(withJSONObject: payload)
        } else if let inputImage = request.inputImage {
            let multipart = try videoMultipartBody(
                runtime: runtime,
                request: request,
                image: inputImage
            )
            headers["Content-Type"] = "multipart/form-data; boundary=\(multipart.boundary)"
            body = multipart.body
        } else {
            headers["Content-Type"] = "application/json"
            body = try JSONSerialization.data(withJSONObject: [
                "model": runtime.model,
                "prompt": request.prompt,
                "size": request.size,
                "seconds": String(request.seconds),
            ])
        }
        return HTTPRequest(
            url: endpoint,
            method: "POST",
            headers: headers,
            body: body,
            timeoutInterval: 120
        )
    }

    // https://platform.minimax.io/docs/api-reference/video-generation-v2-create
    private static func makeMiniMaxCreateRequest(
        runtime: RuntimeModelConfig,
        request: VideoGenerationRequest,
        profile: VideoGenerationProfile
    ) throws -> HTTPRequest {
        let payload = try miniMaxPayload(runtime: runtime, request: request, profile: profile)
        var headers = providerHeaders(runtime: runtime)
        headers["Content-Type"] = "application/json"
        return HTTPRequest(
            url: try miniMaxEndpoint(baseURL: runtime.baseURL ?? ""),
            method: "POST", headers: headers,
            body: try JSONSerialization.data(withJSONObject: payload), timeoutInterval: 120
        )
    }

    private static func miniMaxPayload(
        runtime: RuntimeModelConfig, request: VideoGenerationRequest, profile: VideoGenerationProfile
    ) throws -> [String: Any] {
        guard request.referenceVideo == nil
                || (request.inputImage == nil && request.lastFrameImage == nil) else {
            throw MediaGenerationClientError.mixedFrameAndReferenceVideoInputs
        }
        guard profile.sizes.contains(request.size), profile.durations.contains(request.seconds),
              (request.inputImage != nil || VideoGenerationProfile.miniMaxRatios.contains(request.ratio)) else {
            throw MediaGenerationClientError.invalidVideoOptions
        }
        let prompt = request.prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !prompt.isEmpty, prompt.unicodeScalars.count <= 7_000 else {
            throw MediaGenerationClientError.invalidMiniMaxPrompt
        }
        var content: [[String: Any]] = [["type": "text", "text": prompt]]
        if let image = request.inputImage {
            try validateMiniMaxImage(image)
            content.append([
                "type": "image_url",
                "image_url": ["url": "data:\(image.mimeType.lowercased());base64,\(image.base64Data)"],
                "role": "first_frame",
            ])
        }
        if let image = request.lastFrameImage {
            guard request.inputImage != nil, profile.supportsLastFrame else {
                throw MediaGenerationClientError.unsupportedLastFrameProtocol
            }
            try validateMiniMaxImage(image)
            content.append([
                "type": "image_url",
                "image_url": ["url": "data:\(image.mimeType.lowercased());base64,\(image.base64Data)"],
                "role": "last_frame",
            ])
        }
        if let video = request.referenceVideo {
            guard profile.supportsReferenceVideo else {
                throw MediaGenerationClientError.unsupportedReferenceVideoProtocol
            }
            try validateMiniMaxReferenceVideo(video)
            content.append([
                "type": "video_url",
                "video_url": ["url": "data:\(video.mimeType.lowercased());base64,\(video.base64Data)"],
                "role": "reference_video",
            ])
        }
        return [
            "model": runtime.model,
            "content": content,
            "resolution": request.size,
            "duration": request.seconds,
            "ratio": request.inputImage == nil && request.lastFrameImage == nil ? request.ratio : "adaptive",
        ]
    }

    // https://ark.volcengine.com/region:cn-beijing/docs/82379/1520757?lang=zh
    private static func makeSeedanceCreateRequest(
        runtime: RuntimeModelConfig,
        request: VideoGenerationRequest,
        profile: VideoGenerationProfile
    ) throws -> HTTPRequest {
        guard profile.isSeedance else {
            throw MediaGenerationClientError.invalidModelConfiguration
        }
        let payload = try seedancePayload(runtime: runtime, request: request, profile: profile)
        var headers = providerHeaders(runtime: runtime)
        headers["Content-Type"] = "application/json"
        return HTTPRequest(
            url: try arkVideoEndpoint(baseURL: runtime.baseURL ?? ""),
            method: "POST", headers: headers,
            body: try JSONSerialization.data(withJSONObject: payload), timeoutInterval: 120
        )
    }

    private static func seedancePayload(
        runtime: RuntimeModelConfig,
        request: VideoGenerationRequest,
        profile: VideoGenerationProfile
    ) throws -> [String: Any] {
        guard profile.sizes.contains(request.size) else {
            throw MediaGenerationClientError.invalidVideoOptions
        }
        guard request.referenceVideo == nil
                || (request.inputImage == nil && request.lastFrameImage == nil) else {
            throw MediaGenerationClientError.mixedFrameAndReferenceVideoInputs
        }
        if request.referencePurpose != .reference {
            guard request.referenceVideo != nil else {
                throw MediaGenerationClientError.missingReferenceVideo
            }
        }
        let duration = request.referencePurpose == .edit ? -1 : request.seconds
        guard duration == -1 || profile.durations.contains(duration) else {
            throw MediaGenerationClientError.invalidVideoOptions
        }
        var content: [[String: Any]] = [[
            "type": "text",
            "text": seedancePrompt(request.prompt, purpose: request.referencePurpose),
        ]]
        if let image = request.inputImage {
            try validateSeedanceImage(image)
            content.append([
                "type": "image_url",
                "image_url": ["url": "data:\(image.mimeType.lowercased());base64,\(image.base64Data)"],
                "role": "first_frame",
            ])
        }
        if let image = request.lastFrameImage {
            guard request.inputImage != nil else {
                throw MediaGenerationClientError.unsupportedLastFrameProtocol
            }
            try validateSeedanceImage(image)
            content.append([
                "type": "image_url",
                "image_url": ["url": "data:\(image.mimeType.lowercased());base64,\(image.base64Data)"],
                "role": "last_frame",
            ])
        }
        if let video = request.referenceVideo {
            try validateSeedanceReferenceVideo(video)
            content.append([
                "type": "video_url",
                "video_url": ["url": "data:\(video.mimeType.lowercased());base64,\(video.base64Data)"],
                "role": "reference_video",
            ])
        }
        var payload: [String: Any] = [
            "model": runtime.model,
            "content": content,
            "resolution": request.size,
            "ratio": request.referencePurpose == .reference
                ? (request.inputImage == nil ? request.ratio : "adaptive") : "adaptive",
            "duration": duration,
            "generate_audio": true,
        ]
        if request.referenceVideo != nil {
            switch request.referencePurpose {
            case .reference: payload["omni_reference_task_type"] = "reference"
            case .edit: payload["omni_reference_task_type"] = "edit"
            case .extend: payload["omni_reference_task_type"] = "extend"
            }
        }
        if profile == .seedance25 {
            payload["output_format"] = "mp4"
        }
        return payload
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

    private static func miniMaxEndpoint(baseURL: String, jobID: String? = nil) throws -> URL {
        // Preserve the configured relay host and any routing prefix, replacing the API version.
        let base = normalizedProviderBaseURL(baseURL)
        guard var components = URLComponents(string: base),
              ["https", "http"].contains(components.scheme?.lowercased() ?? ""),
              components.host != nil else {
            throw MediaGenerationClientError.invalidModelConfiguration
        }
        var path = components.path
        if path.hasSuffix("/video_generation") { path.removeLast("/video_generation".count) }
        if path.hasSuffix("/v1") || path.hasSuffix("/v2") { path.removeLast(3) }
        components.path = path + "/v2"
        components.query = nil
        components.fragment = nil
        guard let root = components.url else {
            throw MediaGenerationClientError.invalidModelConfiguration
        }
        if let jobID {
            return root.appendingPathComponent("query/video_generation").appendingPathComponent(jobID)
        }
        return root.appendingPathComponent("video_generation")
    }

    private static func arkVideoEndpoint(baseURL: String, jobID: String? = nil) throws -> URL {
        var value = baseURL.trimmingCharacters(in: .whitespacesAndNewlines)
        while value.hasSuffix("/") { value.removeLast() }
        if let range = value.range(of: "/contents/generations/tasks", options: .backwards) {
            value = String(value[..<range.lowerBound])
        }
        guard var components = URLComponents(string: value),
              ["https", "http"].contains(components.scheme?.lowercased() ?? ""),
              components.host != nil else {
            throw MediaGenerationClientError.invalidModelConfiguration
        }
        if components.path.isEmpty || components.path == "/" {
            components.path = "/api/v3"
        }
        components.query = nil
        components.fragment = nil
        guard let root = components.url else {
            throw MediaGenerationClientError.invalidModelConfiguration
        }
        var endpoint = root.appendingPathComponent("contents/generations/tasks")
        if let jobID { endpoint.appendPathComponent(jobID) }
        return endpoint
    }

    private static func videoStatusEndpoint(
        baseURL: String, jobID: String, wireProtocol: VideoWireProtocol
    ) throws -> URL {
        switch wireProtocol {
        case .miniMaxNative:
            try miniMaxEndpoint(baseURL: baseURL, jobID: jobID)
        case .volcengineArk:
            try arkVideoEndpoint(baseURL: baseURL, jobID: jobID)
        case .openAICompatible:
            try videoJobEndpoint(baseURL: baseURL, jobID: jobID)
        }
    }

    private static func videoMultipartBody(
        runtime: RuntimeModelConfig,
        request: VideoGenerationRequest,
        image: ImageGenerationInputImage
    ) throws -> (boundary: String, body: Data) {
        guard let imageData = Data(base64Encoded: image.base64Data),
              !imageData.isEmpty,
              imageData.count <= 20 * 1024 * 1024 else {
            throw MediaGenerationClientError.invalidInputImage
        }
        let mimeType = image.mimeType.lowercased()
        guard ["image/png", "image/jpeg", "image/webp"].contains(mimeType) else {
            throw MediaGenerationClientError.invalidInputImage
        }
        let boundary = "ChatOSVideoBoundary\(UUID().uuidString.replacingOccurrences(of: "-", with: ""))"
        var body = Data()
        appendMultipartField(name: "model", value: runtime.model, boundary: boundary, to: &body)
        appendMultipartField(name: "prompt", value: request.prompt, boundary: boundary, to: &body)
        appendMultipartField(name: "size", value: request.size, boundary: boundary, to: &body)
        appendMultipartField(name: "seconds", value: String(request.seconds), boundary: boundary, to: &body)
        let fileName = sanitizedFileName(image.name, mimeType: mimeType)
        body.append(Data("--\(boundary)\r\n".utf8))
        body.append(Data("Content-Disposition: form-data; name=\"input_reference\"; filename=\"\(fileName)\"\r\n".utf8))
        body.append(Data("Content-Type: \(mimeType)\r\n\r\n".utf8))
        body.append(imageData)
        body.append(Data("\r\n--\(boundary)--\r\n".utf8))
        return (boundary, body)
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

    private static func decodeVideoJob(
        _ body: Data, wireProtocol: VideoWireProtocol, isCreation: Bool
    ) throws -> ProviderVideoJob {
        guard let root = try? JSONSerialization.jsonObject(with: body) as? [String: Any] else {
            throw MediaGenerationClientError.invalidProviderResponse
        }
        if (wireProtocol == .miniMaxNative || wireProtocol == .volcengineArk) && isCreation {
            let rawID = wireProtocol == .miniMaxNative ? root["task_id"] : root["id"]
            guard let id = (rawID as? String)?.trimmingCharacters(in: .whitespacesAndNewlines),
                  !id.isEmpty else {
                throw MediaGenerationClientError.invalidProviderResponse
            }
            return ProviderVideoJob(id: id, status: "queued")
        }
        let object = wireProtocol == .miniMaxNative ? (root["task"] as? [String: Any] ?? [:]) : root
        guard
              let id = (object["id"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines),
              !id.isEmpty,
              let rawStatus = object["status"] as? String else {
            throw MediaGenerationClientError.invalidProviderResponse
        }
        let status: String
        switch rawStatus.lowercased() {
        case "running": status = "in_progress"
        case "succeeded": status = "completed"
        default: status = rawStatus.lowercased()
        }
        let progress: Double?
        if let value = object["progress"] as? Double {
            progress = value
        } else if let value = object["progress"] as? Int {
            progress = Double(value)
        } else {
            progress = nil
        }
        let errorMessage: String?
        if let error = object["error"] as? [String: Any] {
            errorMessage = error["message"] as? String
        } else {
            errorMessage = object["error"] as? String
        }
        return ProviderVideoJob(
            id: id,
            status: status,
            progress: progress,
            model: object["model"] as? String,
            errorMessage: errorMessage,
            contentURL: {
                let content = object["content"] as? [String: Any]
                return ((content?["url"] ?? content?["video_url"]) as? String).flatMap(URL.init(string:))
            }()
        )
    }

}
