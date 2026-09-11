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
            let url = try wireProtocol == .miniMaxNative
                ? Self.miniMaxEndpoint(baseURL: runtime.baseURL ?? "", jobID: existingJobID)
                : Self.videoJobEndpoint(baseURL: runtime.baseURL ?? "", jobID: existingJobID)
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
            let statusURL = try wireProtocol == .miniMaxNative ? Self.miniMaxEndpoint(
                baseURL: runtime.baseURL ?? "", jobID: job.id
            ) : Self.videoJobEndpoint(
                baseURL: runtime.baseURL ?? "",
                jobID: job.id
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
        if wireProtocol == .miniMaxNative {
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

    private static func makeProviderRequest(
        runtime: RuntimeModelConfig,
        request: ImageGenerationRequest
    ) throws -> HTTPRequest {
        guard let apiKey = runtime.apiKey?.trimmingCharacters(in: .whitespacesAndNewlines),
              let baseURLText = runtime.baseURL?.trimmingCharacters(in: .whitespacesAndNewlines) else {
            throw MediaGenerationClientError.invalidModelConfiguration
        }
        let endpoint = try providerEndpoint(
            baseURL: baseURLText,
            operation: request.inputImage == nil && request.referenceImages.isEmpty ? .generation : .edit
        )
        var headers = [
            "Accept": "application/json",
            "Authorization": "Bearer \(apiKey)",
        ]
        let body: Data
        if let inputImage = request.inputImage ?? request.referenceImages.first {
            let multipart = try multipartBody(runtime: runtime, request: request, image: inputImage)
            headers["Content-Type"] = "multipart/form-data; boundary=\(multipart.boundary)"
            body = multipart.body
        } else {
            headers["Content-Type"] = "application/json"
            var payload: [String: Any] = [
                "model": runtime.model,
                "prompt": request.prompt,
                "n": request.count,
            ]
            if let size = request.size {
                payload["size"] = size
            }
            body = try JSONSerialization.data(withJSONObject: payload)
        }
        return HTTPRequest(
            url: endpoint,
            method: "POST",
            headers: headers,
            body: body,
            timeoutInterval: 10 * 60
        )
    }

    private static func makeVideoCreateRequest(
        runtime: RuntimeModelConfig,
        request: VideoGenerationRequest
    ) throws -> HTTPRequest {
        let profile = VideoGenerationProfile(modelName: runtime.model)
        if runtime.videoProtocol == .miniMaxNative {
            return try makeMiniMaxCreateRequest(runtime: runtime, request: request, profile: profile)
        }
        guard profile.durations.contains(request.seconds), profile.sizes.contains(request.size) else {
            throw MediaGenerationClientError.invalidVideoOptions
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
        return [
            "model": runtime.model,
            "content": content,
            "resolution": request.size,
            "duration": request.seconds,
            "ratio": request.inputImage == nil && request.lastFrameImage == nil ? request.ratio : "adaptive",
        ]
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

    private static func providerEndpoint(
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
        if wireProtocol == .miniMaxNative && isCreation {
            guard let id = (root["task_id"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines),
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
            contentURL: ((object["content"] as? [String: Any])?["url"] as? String).flatMap(URL.init(string:))
        )
    }

    private static func multipartBody(
        runtime: RuntimeModelConfig,
        request: ImageGenerationRequest,
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

        let boundary = "ChatOSMediaBoundary\(UUID().uuidString.replacingOccurrences(of: "-", with: ""))"
        var body = Data()
        appendMultipartField(name: "model", value: runtime.model, boundary: boundary, to: &body)
        appendMultipartField(name: "prompt", value: request.prompt, boundary: boundary, to: &body)
        appendMultipartField(name: "n", value: String(request.count), boundary: boundary, to: &body)
        if let size = request.size {
            appendMultipartField(name: "size", value: size, boundary: boundary, to: &body)
        }
        let images = request.referenceImages.isEmpty ? [image]
            : (request.inputImage.map { [$0] } ?? []) + request.referenceImages
        guard images.count <= 8 else { throw MediaGenerationClientError.invalidInputImage }
        for reference in images {
            guard let data = Data(base64Encoded: reference.base64Data), !data.isEmpty,
                  data.count <= 20 * 1024 * 1024,
                  ["image/png", "image/jpeg", "image/webp"].contains(reference.mimeType.lowercased()) else {
                throw MediaGenerationClientError.invalidInputImage
            }
            let fileName = sanitizedFileName(reference.name, mimeType: reference.mimeType)
            let field = images.count > 1 ? "image[]" : "image"
            body.append(Data("--\(boundary)\r\n".utf8))
            body.append(Data("Content-Disposition: form-data; name=\"\(field)\"; filename=\"\(fileName)\"\r\n".utf8))
            body.append(Data("Content-Type: \(reference.mimeType.lowercased())\r\n\r\n".utf8))
            body.append(data)
            body.append(Data("\r\n".utf8))
        }
        body.append(Data("--\(boundary)--\r\n".utf8))
        return (boundary, body)
    }

    private static func appendMultipartField(
        name: String,
        value: String,
        boundary: String,
        to body: inout Data
    ) {
        body.append(Data("--\(boundary)\r\n".utf8))
        body.append(Data("Content-Disposition: form-data; name=\"\(name)\"\r\n\r\n".utf8))
        body.append(Data(value.utf8))
        body.append(Data("\r\n".utf8))
    }

    private static func sanitizedFileName(_ name: String, mimeType: String) -> String {
        let fileExtension: String
        switch mimeType {
        case "image/jpeg": fileExtension = "jpg"
        case "image/webp": fileExtension = "webp"
        default: fileExtension = "png"
        }
        let stem = name
            .split(separator: ".")
            .dropLast()
            .joined(separator: ".")
            .filter { $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-" || $0 == "_") }
        return "\(stem.isEmpty ? "input" : stem).\(fileExtension)"
    }

    private static func providerErrorDetail(_ body: Data) -> String {
        if let object = try? JSONSerialization.jsonObject(with: body) as? [String: Any] {
            if let error = object["error"] as? [String: Any],
               let message = error["message"] as? String {
                return message
            }
            if let message = object["message"] as? String {
                return message
            }
        }
        let raw = String(decoding: body.prefix(2_000), as: UTF8.self)
        return raw.isEmpty ? "响应正文为空，未提供具体错误原因。" : raw
    }

    private static func imageModelRank(_ model: MediaGenerationModel) -> Int {
        let searchable = "\(model.name) \(model.modelName)".lowercased()
        return searchable.contains("image") ? 0 : 1
    }
}

private enum VideoWireProtocol { case openAICompatible, miniMaxNative }

private struct RuntimeModelConfig: Decodable, Sendable {
    var provider: String?
    var model: String
    var apiKey: String?
    var baseURL: String?
    var enabled: Bool

    enum CodingKeys: String, CodingKey {
        case provider, model, enabled
        case apiKey = "api_key"
        case baseURL = "base_url"
    }

    var videoProtocol: VideoWireProtocol {
        // Explicit OpenAI-compatible configuration wins, regardless of model or host.
        switch provider?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
        case "gpt", "openai": return .openAICompatible
        case "minimax": return .miniMaxNative
        default:
            let host = URL(string: baseURL ?? "")?.host?.lowercased()
            return ["api.minimax.io", "api.minimaxi.com"].contains(host ?? "")
                ? .miniMaxNative : .openAICompatible
        }
    }
}

private struct ProviderImageResponse: Decodable, Sendable {
    var id: String?
    var model: String?
    var data: [ProviderImageItem]
}

private struct ProviderImageItem: Decodable, Sendable {
    var id: String?
    var b64JSON: String?
    var base64Data: String?
    var url: URL?
    var mimeType: String?
    var revisedPrompt: String?

    enum CodingKeys: String, CodingKey {
        case id
        case b64JSON = "b64_json"
        case base64Data = "base64"
        case url
        case mimeType = "mime_type"
        case revisedPrompt = "revised_prompt"
    }
}

private enum ProviderImageOperation: String {
    case generation = "generations"
    case edit = "edits"
}

private struct ProviderVideoJob: Sendable {
    var id: String
    var status: String
    var progress: Double?
    var model: String?
    var errorMessage: String?
    var contentURL: URL?

    var isPending: Bool {
        status == "queued" || status == "in_progress" || status == "processing"
    }
}

private enum MediaGenerationClientError: LocalizedError, MediaGenerationSubmissionFailure {
    case preflightFailed(String)
    case invalidModelConfiguration
    case invalidInputImage
    case invalidProviderResponse
    case responseTooLarge
    case providerRejected(statusCode: Int, detail: String)
    case invalidVideoOptions
    case invalidVideoContent
    case videoTimedOut
    case videoFailed(String?)
    case invalidMiniMaxPrompt
    case invalidMiniMaxImageDimensions
    case unsupportedLastFrameProtocol
    case videoEndpointReturnedHTML(String)

    var errorDescription: String? {
        switch self {
        case let .preflightFailed(detail):
            detail
        case .invalidModelConfiguration:
            "模型缺少可用的 API 地址或密钥，请检查模型配置。"
        case .invalidInputImage:
            "参考图无效或超过 20 MB。"
        case .invalidProviderResponse:
            "模型返回的数据格式无法识别。"
        case .responseTooLarge:
            "模型返回的数据过大。"
        case let .providerRejected(statusCode, detail):
            "模型请求失败（HTTP \(statusCode)）：\(detail)"
        case .invalidVideoOptions:
            "视频尺寸或时长不受当前协议支持。"
        case .invalidMiniMaxPrompt:
            "MiniMax 视频提示词不能为空，且不能超过 7000 字符。"
        case .invalidMiniMaxImageDimensions:
            "MiniMax 参考图宽高须为 256–5760 像素，宽高比须为 0.4–2.5。"
        case .unsupportedLastFrameProtocol:
            "尾帧约束需要同时提供首帧，且当前视频模型必须支持首尾帧生成。"
        case let .videoEndpointReturnedHTML(endpoint):
            "视频接口 \(endpoint) 返回了网页而非任务数据，请检查客户端协议与接口路径是否匹配。"
        case .invalidVideoContent:
            "模型没有返回可播放的视频文件。"
        case .videoTimedOut:
            "视频生成等待超时，可以稍后重新尝试。"
        case let .videoFailed(detail):
            "视频生成失败：\(detail?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty ?? "模型未提供失败原因")"
        }
    }

    var requestMayHaveBeenSubmitted: Bool {
        switch self {
        case .preflightFailed, .invalidModelConfiguration, .invalidInputImage,
             .invalidVideoOptions, .invalidMiniMaxPrompt, .invalidMiniMaxImageDimensions,
             .unsupportedLastFrameProtocol:
            false
        case .invalidProviderResponse, .responseTooLarge, .providerRejected,
             .invalidVideoContent, .videoTimedOut, .videoFailed, .videoEndpointReturnedHTML:
            true
        }
    }
}

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}
