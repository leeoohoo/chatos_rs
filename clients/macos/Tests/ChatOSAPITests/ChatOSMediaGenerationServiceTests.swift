import ChatOSCore
import Foundation
import AppKit
import XCTest
@testable import ChatOSAPI

final class ChatOSMediaGenerationServiceTests: XCTestCase {
    func testResumeVideoOnlyQueriesExistingNewAPITask() async throws {
        let transport = NewAPIVideoTransport()
        let client = ChatOSAPIClient(configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!), accessToken: "token", transport: transport)
        let service = ChatOSMediaGenerationService(client: client, providerTransport: transport, videoPollIntervalNanoseconds: 0)
        let progress = VideoProgressRecorder()
        let result = try await service.resumeVideo(.init(modelConfigID: "h3", prompt: "original prompt", size: "768P", seconds: 15), jobID: "video-h3") {
            await progress.append($0)
        }
        let requests = await transport.allRequests()
        XCTAssertTrue(requests.allSatisfy { $0.method == "GET" })
        XCTAssertEqual(requests.map(\.url.path), ["/api/chatos/ai-model-configs/h3", "/v1/videos/video-h3", "/v1/videos/video-h3/content"])
        XCTAssertEqual(result.id, "video-h3")
        let updates = await progress.values()
        XCTAssertEqual(updates.first?.jobID, "video-h3")
    }

    func testAllStoryReferencesAreAttachedToImageEdits() async throws {
        let transport = MediaGenerationTransport()
        let client = ChatOSAPIClient(configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!), accessToken: "token", transport: transport)
        let service = ChatOSMediaGenerationService(client: client, providerTransport: transport)
        _ = try await service.generateImage(.init(modelConfigID: "image-model", prompt: "Compose the scene", size: nil, count: 1,
            referenceImages: [.init(name: "character.png", mimeType: "image/png", base64Data: Data("character-bytes".utf8).base64EncodedString()),
                              .init(name: "scene.png", mimeType: "image/png", base64Data: Data("scene-bytes".utf8).base64EncodedString())]))
        let requests = await transport.allRequests()
        let request = try XCTUnwrap(requests.last)
        XCTAssertEqual(request.url.path, "/v1/images/edits")
        let body = String(decoding: try XCTUnwrap(request.body), as: UTF8.self)
        XCTAssertEqual(body.components(separatedBy: "name=\"image[]\"").count - 1, 2)
        XCTAssertTrue(body.contains("character-bytes")); XCTAssertTrue(body.contains("scene-bytes"))
    }

    func testLegacyImageRequestDecodesWithoutMultiReferenceField() throws {
        let data = Data(#"{"model_config_id":"image-model","prompt":"test","count":1}"#.utf8)
        let request = try JSONDecoder().decode(ImageGenerationRequest.self, from: data)
        XCTAssertTrue(request.referenceImages.isEmpty)
    }

    func testLoadsUsableModelsFromExistingCatalogWithoutRequiringTaskUsage() async throws {
        let transport = MediaGenerationTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        let models = try await ChatOSMediaGenerationService(client: client).fetchModels()

        XCTAssertEqual(models.map(\.id), ["image-model", "task-model", "video-model"])
        XCTAssertEqual(models.first?.taskEnabled, false)
        XCTAssertEqual(models.filter(\.isLikelyVideoModel).map(\.id), ["video-model"])
        let request = await transport.firstRequest()
        XCTAssertEqual(request?.url.path, "/api/chatos/ai-model-configs")
        XCTAssertNil(request?.url.query)
    }

    func testGenerateImageFetchesSecretThenCallsConfiguredProviderDirectly() async throws {
        let transport = MediaGenerationTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        let result = try await ChatOSMediaGenerationService(
            client: client,
            providerTransport: transport
        ).generateImage(
            .init(modelConfigID: "image-model", prompt: "orange fox", size: "1024x1024", count: 1)
        )

        XCTAssertEqual(result.images.first?.base64Data, "aW1hZ2U=")
        XCTAssertEqual(result.modelName, "gpt-image-1")
        let requests = await transport.allRequests()
        XCTAssertEqual(requests.count, 2)
        XCTAssertEqual(requests[0].url.path, "/api/chatos/ai-model-configs/image-model")
        XCTAssertEqual(requests[0].url.query, "include_secret=true")

        let request = requests[1]
        XCTAssertEqual(request.method, "POST")
        XCTAssertEqual(request.url.host, "provider.example")
        XCTAssertEqual(request.url.path, "/v1/images/generations")
        XCTAssertEqual(request.headers["Authorization"], "Bearer secret")
        XCTAssertEqual(request.headers["Content-Type"], "application/json")
        XCTAssertEqual(request.timeoutInterval, 600)

        let body = try XCTUnwrap(request.body)
        let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        XCTAssertEqual(payload["model"] as? String, "gpt-image-1")
        XCTAssertEqual(payload["prompt"] as? String, "orange fox")
        XCTAssertEqual(payload["size"] as? String, "1024x1024")
        XCTAssertEqual(payload["n"] as? Int, 1)
    }

    func testGenerateImageEncodesOptionalReferenceImage() async throws {
        let transport = MediaGenerationTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        _ = try await ChatOSMediaGenerationService(
            client: client,
            providerTransport: transport
        ).generateImage(
            .init(
                modelConfigID: "image-model",
                prompt: "turn it into watercolor",
                size: nil,
                count: 1,
                inputImage: .init(
                    name: "reference.png",
                    mimeType: "image/png",
                    base64Data: "aW1hZ2U="
                )
            )
        )

        let requests = await transport.allRequests()
        XCTAssertEqual(requests.count, 2)
        let request = requests[1]
        XCTAssertEqual(request.url.host, "provider.example")
        XCTAssertEqual(request.url.path, "/v1/images/edits")
        XCTAssertEqual(request.headers["Authorization"], "Bearer secret")
        XCTAssertTrue(request.headers["Content-Type"]?.hasPrefix("multipart/form-data; boundary=") == true)
        XCTAssertEqual(request.timeoutInterval, 600)

        let body = try XCTUnwrap(request.body)
        XCTAssertNotNil(body.range(of: Data("name=\"model\"\r\n\r\ngpt-image-1".utf8)))
        XCTAssertNotNil(body.range(of: Data("name=\"prompt\"\r\n\r\nturn it into watercolor".utf8)))
        XCTAssertNotNil(body.range(of: Data("name=\"image\"; filename=\"reference.png\"".utf8)))
        XCTAssertNotNil(body.range(of: Data("Content-Type: image/png\r\n\r\nimage".utf8)))
    }

    func testGenerateVideoCreatesPollsAndDownloadsDirectlyFromProvider() async throws {
        let transport = MediaGenerationTransport()
        let progress = VideoProgressRecorder()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        let result = try await ChatOSMediaGenerationService(
            client: client,
            providerTransport: transport,
            videoPollIntervalNanoseconds: 0
        ).generateVideo(
            .init(
                modelConfigID: "video-model",
                prompt: "camera circles a paper city",
                size: "1280x720",
                seconds: 4
            )
        ) { value in
            await progress.append(value)
        }

        XCTAssertEqual(result.id, "video-1")
        XCTAssertEqual(result.modelName, "sora-2")
        XCTAssertEqual(result.videoData, Data("mp4-data".utf8))
        XCTAssertEqual(result.mimeType, "video/mp4")
        let progressValues = await progress.values()
        XCTAssertEqual(progressValues.map(\.status), ["queued", "completed", "downloading"])

        let requests = await transport.allRequests()
        XCTAssertEqual(requests.map(\.url.path), [
            "/api/chatos/ai-model-configs/video-model",
            "/v1/videos",
            "/v1/videos/video-1",
            "/v1/videos/video-1/content",
        ])
        XCTAssertEqual(requests[1].method, "POST")
        XCTAssertEqual(requests[2].method, "GET")
        XCTAssertEqual(requests[3].headers["Accept"], "video/mp4")
        XCTAssertTrue(requests.dropFirst().allSatisfy {
            $0.headers["Authorization"] == "Bearer video-secret"
        })

        let body = try XCTUnwrap(requests[1].body)
        let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        XCTAssertEqual(payload["model"] as? String, "sora-2")
        XCTAssertEqual(payload["size"] as? String, "1280x720")
        XCTAssertEqual(payload["seconds"] as? String, "4")
    }

    func testMiniMaxFirstFrameUsesV2JSONAndPollsNestedTaskThenDownloadsWithoutAPIKey() async throws {
        let transport = MiniMaxTransport()
        let progress = VideoProgressRecorder()
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 256, pixelsHigh: 256,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let result = try await miniMaxService(transport).generateVideo(.init(
            modelConfigID: "h3", prompt: "The dog runs", size: "768P", seconds: 4,
            inputImage: .init(name: "dog.png", mimeType: "image/png", base64Data: png.base64EncodedString())
        )) { await progress.append($0) }
        XCTAssertEqual(result.videoData, Data("mp4-data".utf8))
        let requests = await transport.allRequests()
        XCTAssertEqual(requests.map(\.url.path), [
            "/api/chatos/ai-model-configs/h3", "/v2/video_generation",
            "/v2/query/video_generation/h3-task", "/v2/query/video_generation/h3-task", "/result.mp4",
        ])
        XCTAssertEqual(requests[1].url.host, "relay.example")
        XCTAssertEqual(requests[1].headers["Content-Type"], "application/json")
        XCTAssertEqual(requests[1].headers["Authorization"], "Bearer h3-secret")
        XCTAssertEqual(requests[2].headers["Authorization"], "Bearer h3-secret")
        XCTAssertNil(requests.last?.headers["Authorization"])
        let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(requests[1].body)) as? [String: Any])
        XCTAssertEqual(payload["model"] as? String, "MiniMax-H3")
        XCTAssertEqual(payload["resolution"] as? String, "768P")
        XCTAssertEqual(payload["duration"] as? Int, 4)
        XCTAssertEqual(payload["ratio"] as? String, "adaptive")
        XCTAssertNil(payload["size"])
        XCTAssertNil(payload["seconds"])
        let content = try XCTUnwrap(payload["content"] as? [[String: Any]])
        XCTAssertEqual(content.first?["text"] as? String, "The dog runs")
        XCTAssertEqual(content.last?["role"] as? String, "first_frame")
        XCTAssertEqual((content.last?["image_url"] as? [String: String])?["url"], "data:image/png;base64,\(png.base64EncodedString())")
        let values = await progress.values()
        XCTAssertEqual(values.map(\.status), ["queued", "in_progress", "completed", "downloading"])
    }

    func testMiniMaxTextGenerationUsesExplicitRatioAndPreservesRoutingPrefix() async throws {
        for base in ["https://relay.example/minimax/v1/", "https://relay.example/minimax/v2/video_generation"] {
            let transport = MiniMaxTransport(model: "MiniMax-H3-Max", base: base)
            _ = try await miniMaxService(transport).generateVideo(.init(
                modelConfigID: "h3", prompt: "A running dog", size: "480P", seconds: 5, ratio: "9:16"
            )) { _ in }
            let requests = await transport.allRequests()
            XCTAssertEqual(requests[1].url.path, "/minimax/v2/video_generation")
            let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(requests[1].body)) as? [String: Any])
            XCTAssertEqual(payload["ratio"] as? String, "9:16")
            XCTAssertEqual((payload["content"] as? [Any])?.count, 1)
        }
    }

    func testMiniMaxRejectsInvalidOptionsBeforeCreatingTask() async throws {
        for (size, seconds) in [("2K", 5), ("768P", 4), ("1280x720", 5)] {
            let transport = MiniMaxTransport(model: "MiniMax-H3-Max")
            do {
                _ = try await miniMaxService(transport).generateVideo(.init(
                    modelConfigID: "h3", prompt: "dog", size: size, seconds: seconds
                )) { _ in }
                XCTFail("Invalid H3 Max options should be rejected")
            } catch {
                let requests = await transport.allRequests()
                XCTAssertEqual(requests.count, 1)
            }
        }
    }

    func testMiniMaxFailureAndCancellationDoNotDownload() async throws {
        for status in ["failed", "cancelled"] {
            let transport = MiniMaxTransport(terminalStatus: status)
            do {
                _ = try await miniMaxService(transport).generateVideo(.init(
                    modelConfigID: "h3", prompt: "dog", size: "768P", seconds: 4
                )) { _ in }
                XCTFail("Terminal failure should throw")
            } catch {
                XCTAssertTrue(error.localizedDescription.contains("fixture failure"))
                let requests = await transport.allRequests()
                XCTAssertEqual(requests.count, 4)
            }
        }
    }

    private func miniMaxService(_ transport: MiniMaxTransport) -> ChatOSMediaGenerationService {
        ChatOSMediaGenerationService(
            client: ChatOSAPIClient(configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!), transport: transport),
            providerTransport: transport, videoPollIntervalNanoseconds: 0
        )
    }

    func testMiniMaxHTMLResponseReportsProtocolMismatchWithoutBlamingProvider() async throws {
        let transport = MiniMaxTransport(htmlResponse: true)
        do {
            _ = try await miniMaxService(transport).generateVideo(.init(
                modelConfigID: "h3", prompt: "dog", size: "768P", seconds: 4
            )) { _ in }
            XCTFail("A relay web page is not a generation task")
        } catch {
            XCTAssertTrue(error.localizedDescription.contains("relay.example"))
            XCTAssertTrue(error.localizedDescription.contains("返回了网页而非任务数据"))
            XCTAssertFalse(error.localizedDescription.contains("未提供 MiniMax"))
        }
    }

    func testMiniMaxInvalidReferenceIsRejectedBeforeSubmission() async throws {
        let transport = MiniMaxTransport()
        do {
            _ = try await miniMaxService(transport).generateVideo(.init(
                modelConfigID: "h3", prompt: "dog", size: "768P", seconds: 4,
                inputImage: .init(name: "invalid.png", mimeType: "image/png", base64Data: "aW1hZ2U=")
            )) { _ in }
            XCTFail("Undecodable images should be rejected")
        } catch {
            let requests = await transport.allRequests()
            XCTAssertEqual(requests.count, 1)
        }
    }

    func testGenerateVideoEncodesFirstFrameAsMultipartInputReference() async throws {
        let transport = MediaGenerationTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        _ = try await ChatOSMediaGenerationService(
            client: client,
            providerTransport: transport,
            videoPollIntervalNanoseconds: 0
        ).generateVideo(
            .init(
                modelConfigID: "video-model",
                prompt: "the paper boat starts moving",
                size: "1280x720",
                seconds: 4,
                inputImage: .init(
                    name: "first-frame.png",
                    mimeType: "image/png",
                    base64Data: "aW1hZ2U="
                )
            )
        ) { _ in }

        let requests = await transport.allRequests()
        let create = requests[1]
        XCTAssertTrue(create.headers["Content-Type"]?.hasPrefix("multipart/form-data; boundary=") == true)
        let body = try XCTUnwrap(create.body)
        XCTAssertNotNil(body.range(of: Data("name=\"input_reference\"; filename=\"first-frame.png\"".utf8)))
        XCTAssertNotNil(body.range(of: Data("Content-Type: image/png\r\n\r\nimage".utf8)))
    }

    func testNewAPIH3UsesConfiguredCompatibleProtocolForCreatePollAndContent() async throws {
        for provider in ["gpt", "openai", " GPT ", ""] {
            let transport = NewAPIVideoTransport(provider: provider)
            let progress = VideoProgressRecorder()
            let service = compatibleVideoService(transport)
            let result = try await service.generateVideo(.init(
                modelConfigID: "h3", prompt: "A running dog", size: "768P", seconds: 4, ratio: "9:16"
            )) { await progress.append($0) }
            XCTAssertEqual(result.videoData, Data("mp4-data".utf8))
            let requests = await transport.allRequests()
            XCTAssertEqual(requests.map(\.url.path), [
                "/api/chatos/ai-model-configs/h3", "/v1/videos",
                "/v1/videos/video-h3", "/v1/videos/video-h3/content",
            ])
            XCTAssertEqual(requests.map(\.method), ["GET", "POST", "GET", "GET"])
            XCTAssertTrue(requests.dropFirst().allSatisfy { $0.headers["Authorization"] == "Bearer new-api-token" })
            XCTAssertEqual(requests[1].headers["Content-Type"], "application/json")
            let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(requests[1].body)) as? [String: Any])
            XCTAssertEqual(payload["model"] as? String, "MiniMax-H3")
            XCTAssertEqual(payload["prompt"] as? String, "A running dog")
            XCTAssertEqual(payload["duration"] as? Int, 4)
            XCTAssertEqual(payload["size"] as? String, "768P")
            XCTAssertEqual((payload["metadata"] as? [String: String])?["ratio"], "9:16")
            XCTAssertNil(payload["content"])
            XCTAssertNil(payload["resolution"])
            XCTAssertNil(payload["seconds"])
            let values = await progress.values()
            XCTAssertEqual(values.map(\.status), ["queued", "completed", "downloading"])
        }
    }

    func testCompatibleH3ReferenceImageUsesInputReferenceAndAdaptiveRatio() async throws {
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 256, pixelsHigh: 256,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let transport = NewAPIVideoTransport()
        _ = try await compatibleVideoService(transport).generateVideo(.init(
            modelConfigID: "h3", prompt: "Animate this", size: "2K", seconds: 15,
            inputImage: .init(name: "first.png", mimeType: "image/png", base64Data: png.base64EncodedString())
        )) { _ in }
        let requests = await transport.allRequests()
        let payload = try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(requests[1].body)) as? [String: Any])
        XCTAssertEqual(payload["input_reference"] as? String, "data:image/png;base64,\(png.base64EncodedString())")
        XCTAssertEqual((payload["metadata"] as? [String: String])?["ratio"], "adaptive")
        XCTAssertEqual(payload["duration"] as? Int, 15)
        XCTAssertEqual(payload["size"] as? String, "2K")
    }

    func testExplicitCompatibleProviderWinsOverOfficialHostAndKeepsRoutingPrefix() async throws {
        for base in ["https://api.minimax.io/v1", "https://relay.example/route/v1/"] {
            let transport = NewAPIVideoTransport(base: base, model: "MiniMax-H3-Max")
            _ = try await compatibleVideoService(transport).generateVideo(.init(
                modelConfigID: "h3", prompt: "dog", size: "480P", seconds: 5
            )) { _ in }
            let requests = await transport.allRequests()
            XCTAssertEqual(requests[1].url.absoluteString, base.trimmingCharacters(in: CharacterSet(charactersIn: "/")) + "/videos")
        }
    }

    func testCompatibleH3RejectsInvalidOptionsBeforeSubmission() async throws {
        for (model, size, seconds) in [("MiniMax-H3", "1280x720", 4), ("MiniMax-H3-Max", "768P", 4), ("MiniMax-H3-Max", "2K", 5)] {
            let transport = NewAPIVideoTransport(model: model)
            do {
                _ = try await compatibleVideoService(transport).generateVideo(.init(
                    modelConfigID: "h3", prompt: "dog", size: size, seconds: seconds
                )) { _ in }
                XCTFail("Invalid options must not be submitted")
            } catch {
                let requests = await transport.allRequests()
                XCTAssertEqual(requests.count, 1)
            }
        }
    }

    func testCompatibleVideoHTTPFailuresKeepStageAndStatusAndDoNotRetryCreation() async throws {
        for failAt in ["create", "poll", "content"] {
            for html in [false, true] {
                let transport = NewAPIVideoTransport(failAt: failAt, htmlError: html)
                do {
                    _ = try await compatibleVideoService(transport).generateVideo(.init(
                        modelConfigID: "h3", prompt: "dog", size: "768P", seconds: 4
                    )) { _ in }
                    XCTFail("HTTP failure must throw")
                } catch {
                    let message = error.localizedDescription
                    XCTAssertTrue(message.contains("HTTP 502"))
                    let stage = ["create": "创建视频任务", "poll": "查询视频任务", "content": "下载视频内容"][failAt]!
                    XCTAssertTrue(message.contains(stage))
                    XCTAssertTrue(message.contains("relay.example/v1/videos"))
                    XCTAssertFalse(message.contains("未提供 MiniMax"))
                    XCTAssertFalse(message.contains("new-api-token"))
                    if !html { XCTAssertTrue(message.contains("响应正文为空")) }
                    let requests = await transport.allRequests()
                    XCTAssertEqual(requests.filter { $0.method == "POST" }.count, 1)
                    XCTAssertEqual(requests.contains { $0.url.path.hasSuffix("/content") }, failAt == "content")
                }
            }
        }
    }

    private func compatibleVideoService(_ transport: NewAPIVideoTransport) -> ChatOSMediaGenerationService {
        ChatOSMediaGenerationService(
            client: ChatOSAPIClient(configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!), transport: transport),
            providerTransport: transport, videoPollIntervalNanoseconds: 0
        )
    }
}

/// Independent fixture for the New API client-facing contract; native /v2 requests fail.
private actor NewAPIVideoTransport: HTTPTransport {
    private var requests: [HTTPRequest] = []
    let provider: String
    let base: String
    let model: String
    let failAt: String?
    let htmlError: Bool

    init(provider: String = "gpt", base: String = "https://relay.example/v1", model: String = "MiniMax-H3", failAt: String? = nil, htmlError: Bool = false) {
        self.provider = provider
        self.base = base
        self.model = model
        self.failAt = failAt
        self.htmlError = htmlError
    }

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requests.append(request)
        let root = base.trimmingCharacters(in: CharacterSet(charactersIn: "/")) + "/videos"
        let payload: [String: Any]
        if request.url.path == "/api/chatos/ai-model-configs/h3" {
            var config: [String: Any] = ["model": model, "base_url": base, "api_key": "new-api-token", "enabled": true]
            if !provider.isEmpty { config["provider"] = provider }
            payload = config
        } else if request.url.absoluteString == root && request.method == "POST" {
            if failAt == "create" { return failure() }
            payload = ["id": "video-h3", "status": "queued", "model": model]
        } else if request.url.absoluteString == root + "/video-h3" && request.method == "GET" {
            if failAt == "poll" { return failure() }
            payload = ["id": "video-h3", "status": "completed", "model": model]
        } else if request.url.absoluteString == root + "/video-h3/content" && request.method == "GET" {
            if failAt == "content" { return failure() }
            return .init(statusCode: 200, headers: ["content-type": "video/mp4"], body: Data("mp4-data".utf8))
        } else {
            throw URLError(.badURL)
        }
        return .init(statusCode: 200, headers: ["content-type": "application/json"], body: try JSONSerialization.data(withJSONObject: payload))
    }

    private func failure() -> HTTPResponse {
        .init(statusCode: 502, headers: htmlError ? ["content-type": "text/html"] : [:],
              body: htmlError ? Data("<html>Bad Gateway</html>".utf8) : Data())
    }

    func allRequests() -> [HTTPRequest] { requests }
}

private actor MiniMaxTransport: HTTPTransport {
    var requests: [HTTPRequest] = []
    let model: String
    let base: String
    let terminalStatus: String
    let htmlResponse: Bool
    var polls = 0

    init(model: String = "MiniMax-H3", base: String = "https://relay.example/v1", terminalStatus: String = "succeeded", htmlResponse: Bool = false) {
        self.model = model
        self.base = base
        self.terminalStatus = terminalStatus
        self.htmlResponse = htmlResponse
    }

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requests.append(request)
        let payload: [String: Any]
        if request.url.path.contains("ai-model-configs") {
            payload = ["provider": "minimax", "model": model, "base_url": base, "api_key": "h3-secret", "enabled": true]
        } else if request.method == "POST" {
            if htmlResponse {
                return .init(statusCode: 200, headers: ["content-type": "text/html"], body: Data("<!doctype html><html>New API</html>".utf8))
            }
            payload = ["task_id": "h3-task"]
        } else if request.url.path.contains("query/video_generation") {
            polls += 1
            payload = ["task": [
                "id": "h3-task", "model": model, "status": polls == 1 ? "running" : terminalStatus,
                "content": ["url": "https://cdn.example/result.mp4"],
                "error": ["message": "fixture failure"],
            ]]
        } else if request.url.host == "cdn.example" {
            return .init(statusCode: 200, headers: ["content-type": "video/mp4"], body: Data("mp4-data".utf8))
        } else {
            throw URLError(.badURL)
        }
        return .init(statusCode: 200, headers: [:], body: try JSONSerialization.data(withJSONObject: payload))
    }

    func allRequests() -> [HTTPRequest] { requests }
}

private actor MediaGenerationTransport: HTTPTransport {
    private var requests: [HTTPRequest] = []

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requests.append(request)
        let body: Data
        let headers: [String: String]
        if request.url.path.hasSuffix("/ai-model-configs") {
            body = Data(#"[{"id":"task-model","name":"Chat","provider":"gpt","model":"gpt-5.6-sol","enabled":true,"task_enabled":true,"has_api_key":true},{"id":"image-model","name":"Image","provider":"gpt","model":"gpt-image-1","enabled":true,"task_enabled":false,"has_api_key":true},{"id":"video-model","name":"Sora Video","provider":"gpt","model":"sora-2","enabled":true,"task_enabled":false,"has_api_key":true},{"id":"disabled","name":"Disabled","provider":"gpt","model":"gpt-image-disabled","enabled":false,"task_enabled":false,"has_api_key":true},{"id":"no-key","name":"No Key","provider":"gpt","model":"gpt-image-no-key","enabled":true,"task_enabled":false,"has_api_key":false}]"#.utf8)
            headers = [:]
        } else if request.url.path.hasSuffix("/ai-model-configs/image-model") {
            body = Data(#"{"id":"image-model","name":"Image","provider":"gpt","model":"gpt-image-1","api_key":"secret","base_url":"https://provider.example/v1","enabled":true}"#.utf8)
            headers = [:]
        } else if request.url.path.hasSuffix("/ai-model-configs/video-model") {
            body = Data(#"{"id":"video-model","name":"Sora Video","provider":"gpt","model":"sora-2","api_key":"video-secret","base_url":"https://provider.example/v1","enabled":true}"#.utf8)
            headers = [:]
        } else if request.url.path.hasSuffix("/videos/video-1/content") {
            body = Data("mp4-data".utf8)
            headers = ["content-type": "video/mp4"]
        } else if request.url.path.hasSuffix("/videos/video-1") {
            body = Data(#"{"id":"video-1","status":"completed","progress":100,"model":"sora-2"}"#.utf8)
            headers = [:]
        } else if request.url.path.hasSuffix("/videos") {
            body = Data(#"{"id":"video-1","status":"queued","progress":0,"model":"sora-2"}"#.utf8)
            headers = [:]
        } else {
            body = Data(#"{"data":[{"b64_json":"aW1hZ2U=","revised_prompt":"orange fox in warm light"}]}"#.utf8)
            headers = [:]
        }
        return HTTPResponse(statusCode: 200, headers: headers, body: body)
    }

    func firstRequest() -> HTTPRequest? { requests.first }
    func allRequests() -> [HTTPRequest] { requests }
}

private actor VideoProgressRecorder {
    private var recorded: [VideoGenerationProgress] = []

    func append(_ value: VideoGenerationProgress) {
        recorded.append(value)
    }

    func values() -> [VideoGenerationProgress] {
        recorded
    }
}
