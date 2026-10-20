import Foundation

public enum ProjectFilesystemCreationKind: Sendable, Equatable {
    case file
    case directory
}

public struct ProjectFilesystemSaveResult: Sendable, Equatable {
    public let refreshedFile: ProjectFileContent?

    public init(refreshedFile: ProjectFileContent?) {
        self.refreshedFile = refreshedFile
    }
}

public struct ProjectFilesystemDirectoryMutationResult: Sendable, Equatable {
    public let refreshedListing: ProjectDirectoryListing?

    public init(refreshedListing: ProjectDirectoryListing?) {
        self.refreshedListing = refreshedListing
    }
}

public struct ProjectFilesystemMutationUseCase: Sendable {
    private let service: any ProjectFilesystemServicing

    public init(service: any ProjectFilesystemServicing) {
        self.service = service
    }

    public func save(path: String, content: String) async throws -> ProjectFilesystemSaveResult {
        try await service.writeFile(path: path, content: content)
        let file = try? await service.readFile(path: path)
        return .init(refreshedFile: file)
    }

    public func create(
        parentPath: String,
        name: String,
        kind: ProjectFilesystemCreationKind
    ) async throws -> ProjectFilesystemDirectoryMutationResult {
        switch kind {
        case .file:
            try await service.createFile(parentPath: parentPath, name: name)
        case .directory:
            try await service.createDirectory(parentPath: parentPath, name: name)
        }
        let listing = try? await service.listEntries(path: parentPath, forceRefresh: true)
        return .init(refreshedListing: listing)
    }

    public func delete(
        path: String,
        parentPath: String?,
        isDirectory: Bool
    ) async throws -> ProjectFilesystemDirectoryMutationResult {
        try await service.deleteEntry(path: path, recursive: isDirectory)
        guard let parentPath else { return .init(refreshedListing: nil) }
        let listing = try? await service.listEntries(path: parentPath, forceRefresh: true)
        return .init(refreshedListing: listing)
    }
}
