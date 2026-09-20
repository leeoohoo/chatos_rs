import ChatOSCore
import Foundation

extension ChatOSMediaGenerationService {
    static func makeProviderRequest(
        runtime: RuntimeModelConfig,
        request: ImageGenerationRequest
    ) throws -> HTTPRequest {
        guard let apiKey = runtime.apiKey?.trimmingCharacters(in: .whitespacesAndNewlines),
              let baseURLText = runtime.baseURL?.trimmingCharacters(in: .whitespacesAndNewlines) else {
            throw MediaGenerationClientError.invalidModelConfiguration
        }
        let endpoint = try providerEndpoint(
            baseURL: baseURLText,
            operation: request.inputImage == nil && request.referenceImages.isEmpty
                ? .generation : .edit
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

    static func multipartBody(
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
                  ["image/png", "image/jpeg", "image/webp"]
                    .contains(reference.mimeType.lowercased()) else {
                throw MediaGenerationClientError.invalidInputImage
            }
            let fileName = sanitizedFileName(reference.name, mimeType: reference.mimeType)
            let field = images.count > 1 ? "image[]" : "image"
            body.append(Data("--\(boundary)\r\n".utf8))
            body.append(Data(
                "Content-Disposition: form-data; name=\"\(field)\"; filename=\"\(fileName)\"\r\n".utf8
            ))
            body.append(Data("Content-Type: \(reference.mimeType.lowercased())\r\n\r\n".utf8))
            body.append(data)
            body.append(Data("\r\n".utf8))
        }
        body.append(Data("--\(boundary)--\r\n".utf8))
        return (boundary, body)
    }

    static func appendMultipartField(
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

    static func sanitizedFileName(_ name: String, mimeType: String) -> String {
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

    static func providerErrorDetail(_ body: Data) -> String {
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

    static func imageModelRank(_ model: MediaGenerationModel) -> Int {
        let searchable = "\(model.name) \(model.modelName)".lowercased()
        return searchable.contains("image") ? 0 : 1
    }
}
