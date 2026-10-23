import ChatOSCore
import Darwin
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
        var request = request
        request.transferOwnerSessionID = authenticationSessionID?.uuidString.lowercased()
        let disposableBodyFile = request.bodyFile.flatMap { $0.deleteWhenDone ? $0 : nil }
        defer {
            disposableBodyFile?.removeIfNeeded()
        }
        try Task.checkCancellation()
        if let authenticationSessionID {
            guard try await client.currentAuthenticationSessionID() == authenticationSessionID else { throw ChatOSAPIError.unauthorized }
        }
        let response = if request.bodyFile != nil {
            try await providerTransport.sendFileBacked(request)
        } else {
            try await providerTransport.send(request)
        }
        // A completed submission still belongs to the original account. Never make any
        // further provider request after a switch; the caller retains its durable intent.
        if let authenticationSessionID {
            guard try await client.currentAuthenticationSessionID() == authenticationSessionID else { throw ChatOSAPIError.unauthorized }
        }
        return response
    }

    private func streamProviderRequest(_ request: HTTPRequest) async throws -> HTTPStreamResponse {
        var request = request
        request.transferOwnerSessionID = authenticationSessionID?.uuidString.lowercased()
        try Task.checkCancellation()
        if let authenticationSessionID {
            guard try await client.currentAuthenticationSessionID() == authenticationSessionID else {
                throw ChatOSAPIError.unauthorized
            }
        }
        let response = try await providerTransport.stream(request)
        if let authenticationSessionID {
            guard try await client.currentAuthenticationSessionID() == authenticationSessionID else {
                throw ChatOSAPIError.unauthorized
            }
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
        let models: [MediaGenerationModel] = try await client.request(
            "/model-configs",
            service: .userService,
            expectedAuthenticationSessionID: authenticationSessionID
        )
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
        var images: [GeneratedMediaAsset] = []
        images.reserveCapacity(payload.data.count)
        for (index, item) in payload.data.enumerated() {
            try Task.checkCancellation()
            let assetID = item.id ?? "\(resultID):image:\(index)"
            if let base64Data = item.b64JSON ?? item.base64Data {
                let mediaFile = try await ChatOSBase64PayloadFileWriter.write(
                    base64Data,
                    maximumBytes: 20 * 1024 * 1024
                )
                images.append(GeneratedMediaAsset(
                    id: assetID,
                    mimeType: item.mimeType ?? "image/png",
                    revisedPrompt: item.revisedPrompt,
                    mediaFile: mediaFile
                ))
            } else if let url = item.url, url.scheme?.lowercased() == "https" {
                images.append(GeneratedMediaAsset(
                    id: assetID,
                    mimeType: item.mimeType ?? "image/png",
                    url: url,
                    revisedPrompt: item.revisedPrompt
                ))
            } else {
                throw MediaGenerationClientError.invalidProviderResponse
            }
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
                    timeoutInterval: 60,
                    maximumResponseBytes: 2 * 1_024 * 1_024
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
                timeoutInterval: 10 * 60,
                maximumResponseBytes: 512 * 1_024 * 1_024,
                transferTaskID: "video:\(job.id):content"
            )
        } else {
            contentRequest = HTTPRequest(
                url: try Self.videoContentEndpoint(
                    baseURL: runtime.baseURL ?? "", jobID: job.id
                ),
                method: "GET",
                headers: Self.providerHeaders(runtime: runtime, accept: "video/mp4"),
                timeoutInterval: 10 * 60,
                maximumResponseBytes: 512 * 1_024 * 1_024,
                transferTaskID: "video:\(job.id):content"
            )
        }
        await progress(.init(status: "downloading"))
        let contentResponse = try await streamProviderRequest(contentRequest)
        guard (200..<300).contains(contentResponse.statusCode) else {
            let errorBody = try await Self.collect(
                contentResponse.body,
                maximumBytes: 2 * 1_024 * 1_024
            )
            throw MediaGenerationClientError.providerRejected(
                statusCode: contentResponse.statusCode,
                detail: "下载视频内容（\(contentRequest.url.host ?? "")\(contentRequest.url.path)）：\(Self.providerErrorDetail(errorBody))"
            )
        }
        let maximumVideoBytes = 512 * 1_024 * 1_024
        let contentLength = contentResponse.headers["content-length"].flatMap(Int.init)
        if let contentLength,
           contentLength <= 0 || contentLength > maximumVideoBytes {
            throw MediaGenerationClientError.invalidVideoContent
        }
        let mimeType = contentResponse.headers["content-type"]?
            .split(separator: ";")
            .first
            .map(String.init) ?? "video/mp4"
        let videoFile = try await ChatOSResponseBodyFileWriter.write(
            contentResponse.body,
            maximumBytes: maximumVideoBytes,
            expectedBytes: contentLength
        )
        do {
            try Task.checkCancellation()
            if let authenticationSessionID {
                guard try await client.currentAuthenticationSessionID() == authenticationSessionID else {
                    throw ChatOSAPIError.unauthorized
                }
            }
        } catch {
            videoFile.removeIfNeeded()
            throw error
        }
        return VideoGenerationResult(
            id: job.id,
            modelConfigID: request.modelConfigID,
            modelName: job.model ?? runtime.model,
            createdAt: ISO8601DateFormatter().string(from: Date()),
            mimeType: mimeType,
            videoFile: videoFile
        )
    }

    private static func collect(
        _ stream: AsyncThrowingStream<Data, Error>,
        maximumBytes: Int
    ) async throws -> Data {
        var body = Data()
        for try await chunk in stream {
            try Task.checkCancellation()
            guard chunk.count <= maximumBytes,
                  body.count <= maximumBytes - chunk.count else {
                throw MediaGenerationClientError.responseTooLarge
            }
            body.append(chunk)
        }
        return body
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
            "/model-configs/\(encodedID)?include_secret=true",
            service: .userService,
            expectedAuthenticationSessionID: authenticationSessionID
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
        if let clientRequestID = request.clientRequestID?
            .trimmingCharacters(in: .whitespacesAndNewlines),
           !clientRequestID.isEmpty {
            headers["Idempotency-Key"] = clientRequestID
        }
        let inputImagePlaceholder = request.inputImage?.fileURL.map { _ in
            "__CHATOS_FIRST_FRAME_DATA_URL_\(UUID().uuidString)__"
        }
        let lastFramePlaceholder = request.lastFrameImage?.fileURL.map { _ in
            "__CHATOS_LAST_FRAME_DATA_URL_\(UUID().uuidString)__"
        }
        let referenceVideoPlaceholder = request.referenceVideo?.fileURL.map { _ in
            "__CHATOS_VIDEO_DATA_URL_\(UUID().uuidString)__"
        }
        let referenceAudioPlaceholder = request.referenceAudio?.fileURL.map { _ in
            "__CHATOS_AUDIO_DATA_URL_\(UUID().uuidString)__"
        }
        let payload = try unifiedVideoPayload(
            runtime: runtime,
            request: request,
            profile: profile,
            inputImagePlaceholder: inputImagePlaceholder,
            lastFramePlaceholder: lastFramePlaceholder,
            referenceVideoPlaceholder: referenceVideoPlaceholder,
            referenceAudioPlaceholder: referenceAudioPlaceholder
        )
        var fileBackedSources: [ChatOSVideoRequestBodyFileWriter.Source] = []
        if let input = request.inputImage, let fileURL = input.fileURL,
           let inputImagePlaceholder {
            fileBackedSources.append(.init(
                placeholder: inputImagePlaceholder,
                url: fileURL,
                mimeType: input.mimeType,
                maximumBytes: ChatOSVideoRequestBodyFileWriter.maximumImageSourceBytes
            ))
        }
        if let input = request.lastFrameImage, let fileURL = input.fileURL,
           let lastFramePlaceholder {
            fileBackedSources.append(.init(
                placeholder: lastFramePlaceholder,
                url: fileURL,
                mimeType: input.mimeType,
                maximumBytes: ChatOSVideoRequestBodyFileWriter.maximumImageSourceBytes
            ))
        }
        if let input = request.referenceVideo, let fileURL = input.fileURL,
           let referenceVideoPlaceholder {
            fileBackedSources.append(.init(
                placeholder: referenceVideoPlaceholder,
                url: fileURL,
                mimeType: input.mimeType,
                maximumBytes: ChatOSVideoRequestBodyFileWriter.maximumSourceBytes
            ))
        }
        if let input = request.referenceAudio, let fileURL = input.fileURL,
           let referenceAudioPlaceholder {
            fileBackedSources.append(.init(
                placeholder: referenceAudioPlaceholder,
                url: fileURL,
                mimeType: input.mimeType,
                maximumBytes: ChatOSVideoRequestBodyFileWriter.maximumAudioSourceBytes
            ))
        }
        let bodyFile: HTTPRequestBodyFile?
        let body: Data?
        if !fileBackedSources.isEmpty {
            bodyFile = try ChatOSVideoRequestBodyFileWriter.write(
                payload: payload,
                sources: fileBackedSources
            )
            body = nil
        } else {
            bodyFile = nil
            body = try JSONSerialization.data(withJSONObject: payload)
        }
        return HTTPRequest(
            url: endpoint,
            method: "POST",
            headers: headers,
            body: body,
            bodyFile: bodyFile,
            timeoutInterval: 10 * 60,
            maximumResponseBytes: 2 * 1_024 * 1_024
        )
    }

    private static func unifiedVideoPayload(
        runtime: RuntimeModelConfig,
        request: VideoGenerationRequest,
        profile: VideoGenerationProfile,
        inputImagePlaceholder: String? = nil,
        lastFramePlaceholder: String? = nil,
        referenceVideoPlaceholder: String? = nil,
        referenceAudioPlaceholder: String? = nil
    ) throws -> [String: Any] {
        try validateUnifiedVideoRequest(request, profile: profile)
        var metadata: [String: Any] = [
            "ratio": request.referencePurpose == .reference
                ? (request.inputImage == nil ? request.ratio : "adaptive") : "adaptive",
        ]
        if let value = request.inputImage {
            metadata["first_frame_image"] = inputImagePlaceholder ?? dataURL(
                mimeType: value.mimeType, base64Data: value.base64Data
            )
        }
        if let value = request.lastFrameImage {
            metadata["last_frame_image"] = lastFramePlaceholder ?? dataURL(
                mimeType: value.mimeType, base64Data: value.base64Data
            )
        }
        if let value = request.referenceVideo {
            metadata["video_url"] = referenceVideoPlaceholder ?? dataURL(
                mimeType: value.mimeType,
                base64Data: value.base64Data
            )
        }
        if let value = request.referenceAudio {
            metadata["audio_url"] = referenceAudioPlaceholder ?? dataURL(
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
        guard ["image/png", "image/jpeg", "image/webp"].contains(mimeType) else {
            throw MediaGenerationClientError.invalidInputImage
        }
        let properties: [CFString: Any]
        if let fileURL = image.fileURL {
            guard image.base64Data.isEmpty, fileURL.isFileURL else {
                throw MediaGenerationClientError.invalidInputImage
            }
            properties = try imageProperties(for: fileURL)
        } else {
            guard let data = Data(base64Encoded: image.base64Data), !data.isEmpty,
                  data.count <= ChatOSVideoRequestBodyFileWriter.maximumImageSourceBytes,
                  let source = CGImageSourceCreateWithData(data as CFData, nil),
                  let decodedProperties = CGImageSourceCopyPropertiesAtIndex(
                    source, 0, nil
                  ) as? [CFString: Any] else {
                throw MediaGenerationClientError.invalidInputImage
            }
            properties = decodedProperties
        }
        guard let width = properties[kCGImagePropertyPixelWidth] as? Int,
              let height = properties[kCGImagePropertyPixelHeight] as? Int,
              (256...5760).contains(width), (256...5760).contains(height),
              (0.4...2.5).contains(Double(width) / Double(height)) else {
            throw MediaGenerationClientError.invalidMiniMaxImageDimensions
        }
    }

    private static func imageProperties(for url: URL) throws -> [CFString: Any] {
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw MediaGenerationClientError.invalidInputImage }
        defer { Darwin.close(descriptor) }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_size > 0,
              status.st_size <= ChatOSVideoRequestBodyFileWriter.maximumImageSourceBytes else {
            throw MediaGenerationClientError.invalidInputImage
        }
        let descriptorURL = URL(fileURLWithPath: "/dev/fd/\(descriptor)")
        guard let source = CGImageSourceCreateWithURL(descriptorURL as CFURL, nil) else {
            throw MediaGenerationClientError.invalidInputImage
        }
        guard CGImageSourceGetCount(source) > 0,
              let properties = CGImageSourceCopyPropertiesAtIndex(
                source, 0, nil
              ) as? [CFString: Any] else {
            throw MediaGenerationClientError.invalidInputImage
        }
        return properties
    }

    /// Base64 adds roughly one third to the request size. Keep the local source below
    /// 47 MiB so the complete JSON body remains under MiniMax's 64 MiB request limit.
    private static func validateMiniMaxReferenceVideo(_ video: VideoGenerationInputVideo) throws {
        guard ["video/mp4", "video/quicktime"].contains(video.mimeType.lowercased()) else {
            throw MediaGenerationClientError.invalidReferenceVideo
        }
        if let fileURL = video.fileURL {
            guard video.base64Data.isEmpty, fileURL.isFileURL else {
                throw MediaGenerationClientError.invalidReferenceVideo
            }
            // The writer opens with O_NOFOLLOW and validates the descriptor with fstat,
            // closing the check/use race before any bytes are encoded.
            return
        }
        guard let data = Data(base64Encoded: video.base64Data), !data.isEmpty,
              data.count <= ChatOSVideoRequestBodyFileWriter.maximumSourceBytes else {
            throw MediaGenerationClientError.invalidReferenceVideo
        }
    }

    private static func validateReferenceAudio(_ audio: VideoGenerationInputAudio) throws {
        guard [
            "audio/mpeg", "audio/mp3", "audio/wav", "audio/x-wav", "audio/vnd.wave",
            "audio/mp4", "audio/x-m4a", "audio/aac",
        ].contains(audio.mimeType.lowercased()) else {
            throw MediaGenerationClientError.invalidReferenceAudio
        }
        if let fileURL = audio.fileURL {
            guard audio.base64Data.isEmpty, fileURL.isFileURL else {
                throw MediaGenerationClientError.invalidReferenceAudio
            }
            // The request writer opens the file with O_NOFOLLOW and verifies the
            // descriptor immediately before encoding it.
            return
        }
        guard let data = Data(base64Encoded: audio.base64Data), !data.isEmpty,
              data.count <= ChatOSVideoRequestBodyFileWriter.maximumAudioSourceBytes else {
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
