import ChatOSCore
import CryptoKit
import Foundation
import ImageIO
import UniformTypeIdentifiers

actor MediaStudioWorkflowStore {
    private static let maximumManifestBytes = 2 * 1_024 * 1_024
    private let root: URL

    init(root: URL? = nil) {
        self.root = (root ?? FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        )[0].appendingPathComponent(
            "ChatOSSwift/MediaStudio/Workflows",
            isDirectory: true
        )).resolvingSymlinksInPath()
    }

    func list(owner ownerInput: String) throws -> [MediaStudioWorkflow] {
        let owner = try normalizedOwner(ownerInput)
        let directory = ownerDirectory(owner)
        guard FileManager.default.fileExists(atPath: directory.path) else { return [] }
        let children = try FileManager.default.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey]
        )
        var workflows: [MediaStudioWorkflow] = []
        for folder in children {
            try Task.checkCancellation()
            guard UUID(uuidString: folder.lastPathComponent) != nil else { continue }
            let values = try folder.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            guard values.isDirectory == true, values.isSymbolicLink != true else { continue }
            workflows.append(try load(folder: folder, owner: owner))
        }
        return workflows.sorted { $0.updatedAt > $1.updatedAt }
    }

    func save(_ workflow: MediaStudioWorkflow, owner ownerInput: String) throws {
        let owner = try normalizedOwner(ownerInput)
        try MediaStudioWorkflowValidator.validate(workflow)
        let folder = workflowFolder(workflow.id, owner: owner)
        let assets = folder.appendingPathComponent("assets", isDirectory: true)
        try createPrivateDirectory(folder)
        try createPrivateDirectory(assets)
        let data = try JSONEncoder().encode(workflow)
        guard data.count <= Self.maximumManifestBytes else {
            throw MediaStudioWorkflowStoreError.invalidManifest
        }
        try data.write(to: folder.appendingPathComponent("workflow.json"), options: .atomic)
        try cleanupAssets(in: assets, referencedBy: workflow)
    }

    func delete(id: UUID, owner ownerInput: String) throws {
        let owner = try normalizedOwner(ownerInput)
        let folder = workflowFolder(id, owner: owner)
        guard FileManager.default.fileExists(atPath: folder.path) else { return }
        let values = try folder.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        guard values.isDirectory == true, values.isSymbolicLink != true else {
            throw MediaStudioWorkflowStoreError.invalidManifest
        }
        try FileManager.default.removeItem(at: folder)
    }

    func importImage(
        _ image: ImageGenerationInputImage,
        workflowID: UUID,
        owner ownerInput: String
    ) async throws -> MediaStudioWorkflowImageReference {
        let owner = try normalizedOwner(ownerInput)
        guard let contentType = UTType(mimeType: image.mimeType),
              contentType.conforms(to: .image),
              let source = image.fileURL else {
            throw MediaStudioWorkflowStoreError.invalidImage
        }
        let securityAccess = source.startAccessingSecurityScopedResource()
        defer { if securityAccess { source.stopAccessingSecurityScopedResource() } }
        let values = try source.resourceValues(
            forKeys: [.fileSizeKey, .isRegularFileKey, .isSymbolicLinkKey]
        )
        guard source.isFileURL,
              values.isRegularFile == true,
              values.isSymbolicLink != true,
              CGImageSourceCreateWithURL(source as CFURL, nil) != nil,
              let byteCount = values.fileSize,
              (1...MediaStudioWorkflowValidator.maximumImageBytes).contains(byteCount) else {
            throw MediaStudioWorkflowStoreError.invalidImage
        }
        let folder = workflowFolder(workflowID, owner: owner)
        guard FileManager.default.fileExists(atPath: folder.appendingPathComponent("workflow.json").path) else {
            throw MediaStudioWorkflowStoreError.missingWorkflow
        }
        let assets = folder.appendingPathComponent("assets", isDirectory: true)
        try createPrivateDirectory(assets)
        let filename = "\(UUID().uuidString.lowercased()).\(fileExtension(image.mimeType))"
        let destination = assets.appendingPathComponent(filename)
        try await AppCancellableDetachedWork.run {
            try AppBoundedFileReader.copy(
                source,
                to: destination,
                expectedBytes: byteCount,
                maximumBytes: MediaStudioWorkflowValidator.maximumImageBytes
            )
        }
        return .init(
            name: String(image.name.prefix(255)),
            mimeType: image.mimeType,
            filename: filename,
            byteCount: byteCount
        )
    }

    func resolveImages(
        _ references: [MediaStudioWorkflowImageReference],
        workflowID: UUID,
        owner ownerInput: String
    ) throws -> [ImageGenerationInputImage] {
        let owner = try normalizedOwner(ownerInput)
        let assets = workflowFolder(workflowID, owner: owner)
            .appendingPathComponent("assets", isDirectory: true)
        return try references.map { reference in
            let url = assets.appendingPathComponent(reference.filename)
            let values = try url.resourceValues(
                forKeys: [.fileSizeKey, .isRegularFileKey, .isSymbolicLinkKey]
            )
            guard reference.filename == URL(fileURLWithPath: reference.filename).lastPathComponent,
                  values.isRegularFile == true,
                  values.isSymbolicLink != true,
                  values.fileSize == reference.byteCount else {
                throw MediaStudioWorkflowStoreError.invalidImage
            }
            return ImageGenerationInputImage(
                name: reference.name,
                mimeType: reference.mimeType,
                fileURL: url.resolvingSymlinksInPath()
            )
        }
    }

    private func load(folder: URL, owner: String) throws -> MediaStudioWorkflow {
        let workflow = try JSONDecoder().decode(
            MediaStudioWorkflow.self,
            from: AppBoundedFileReader.read(
                folder.appendingPathComponent("workflow.json"),
                maximumBytes: Self.maximumManifestBytes
            )
        )
        guard workflow.id.uuidString.caseInsensitiveCompare(folder.lastPathComponent) == .orderedSame else {
            throw MediaStudioWorkflowStoreError.invalidManifest
        }
        try MediaStudioWorkflowValidator.validate(workflow)
        _ = try resolveImages(
            workflow.inputImages + workflow.nodes.flatMap(\.referenceImages),
            workflowID: workflow.id,
            owner: owner
        )
        return workflow
    }

    private func cleanupAssets(in directory: URL, referencedBy workflow: MediaStudioWorkflow) throws {
        let referenced = Set(
            (workflow.inputImages + workflow.nodes.flatMap(\.referenceImages)).map(\.filename)
        )
        for url in try FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil) {
            guard !referenced.contains(url.lastPathComponent) else { continue }
            let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
            guard values.isRegularFile == true, values.isSymbolicLink != true else {
                throw MediaStudioWorkflowStoreError.invalidImage
            }
            try FileManager.default.removeItem(at: url)
        }
    }

    private func normalizedOwner(_ owner: String) throws -> String {
        let value = owner.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty else { throw MediaStudioWorkflowStoreError.invalidOwner }
        return value
    }

    private func ownerDirectory(_ owner: String) -> URL {
        let key = SHA256.hash(data: Data(owner.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
        return root.appendingPathComponent("Accounts/\(key)", isDirectory: true)
    }

    private func workflowFolder(_ id: UUID, owner: String) -> URL {
        ownerDirectory(owner).appendingPathComponent(id.uuidString.lowercased(), isDirectory: true)
    }

    private func createPrivateDirectory(_ url: URL) throws {
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        try FileManager.default.setAttributes(
            [.posixPermissions: NSNumber(value: Int16(0o700))],
            ofItemAtPath: url.path
        )
    }

    private func fileExtension(_ mimeType: String) -> String {
        UTType(mimeType: mimeType)?.preferredFilenameExtension ?? "png"
    }
}

private enum MediaStudioWorkflowStoreError: LocalizedError {
    case invalidOwner
    case invalidManifest
    case invalidImage
    case missingWorkflow

    var errorDescription: String? {
        switch self {
        case .invalidOwner: "工作流缺少有效的账号上下文。"
        case .invalidManifest: "工作流文件无效或已损坏。"
        case .invalidImage: "工作流参考图无效、丢失或超过 20 MB。"
        case .missingWorkflow: "工作流已不存在，请返回列表后重试。"
        }
    }
}
