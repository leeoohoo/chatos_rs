import ChatOSCore
import CryptoKit
import Foundation

actor StoryProjectStore {
    struct Snapshot: Sendable { var projects: [StoryProject]; var unreadableCount: Int }
    nonisolated let root: URL
    init(root: URL? = nil) {
        self.root = (root ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("ChatOSSwift/StoryStudio", isDirectory: true)).resolvingSymlinksInPath()
    }

    func load(owner: String) throws -> Snapshot {
        let directory = accountDirectory(owner)
        guard FileManager.default.fileExists(atPath: directory.path) else { return .init(projects: [], unreadableCount: 0) }
        var projects: [StoryProject] = []
        var unreadable = 0
        for folder in try FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles]) {
            guard UUID(uuidString: folder.lastPathComponent) != nil else { continue }
            let manifest = folder.appendingPathComponent("project.json")
            guard FileManager.default.fileExists(atPath: manifest.path) else { continue }
            do {
                guard (try manifest.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0) <= 16 * 1024 * 1024 else { throw StoryError.invalidProject }
                let project = try JSONDecoder().decode(StoryProject.self, from: Data(contentsOf: manifest))
                guard project.id.uuidString == folder.lastPathComponent else { throw StoryError.invalidProject }
                try project.validate()
                projects.append(project)
            } catch { unreadable += 1 } // Never replace or delete an unreadable manifest.
        }
        return .init(projects: projects.sorted { $0.updatedAt > $1.updatedAt }, unreadableCount: unreadable)
    }

    func save(_ project: StoryProject, owner: String) throws {
        try project.validate()
        let folder = directory(owner: owner, projectID: project.id)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try JSONEncoder().encode(project).write(to: folder.appendingPathComponent("project.json"), options: .atomic)
    }

    func saveRun(_ run: StoryAgentRun, owner: String) throws {
        try run.validate(owner: owner, projectID: run.projectID)
        let folder = directory(owner: owner, projectID: run.projectID).appendingPathComponent("runs", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let data = try JSONEncoder().encode(run)
        guard data.count <= 64 * 1024 * 1024 else { throw StoryAgentError.invalidRun }
        try data.write(to: folder.appendingPathComponent("\(run.id).json"), options: .atomic)
    }

    func loadRuns(owner: String, projectID: UUID) throws -> (runs: [StoryAgentRun], unreadable: Int) {
        let folder = directory(owner: owner, projectID: projectID).appendingPathComponent("runs", isDirectory: true)
        guard FileManager.default.fileExists(atPath: folder.path) else { return ([], 0) }
        var runs: [StoryAgentRun] = []; var unreadable = 0
        for url in try FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: [.fileSizeKey]) {
            guard url.pathExtension == "json", let id = UUID(uuidString: url.deletingPathExtension().lastPathComponent) else { continue }
            do {
                guard (try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0) <= 64 * 1024 * 1024 else { throw StoryAgentError.invalidRun }
                let run = try JSONDecoder().decode(StoryAgentRun.self, from: Data(contentsOf: url))
                guard run.id == id else { throw StoryAgentError.invalidRun }
                try run.validate(owner: owner, projectID: projectID)
                runs.append(run)
            } catch { unreadable += 1 }
        }
        return (runs.sorted { $0.updatedAt > $1.updatedAt }, unreadable)
    }

    /// Canonical project replacement is guarded by the original digest. A crash after project save
    /// but before the applied marker is recoverable by comparing with the completed draft digest.
    func applyRun(_ input: StoryAgentRun, owner: String) throws -> (StoryAgentRun, StoryProject) {
        var run = input
        try run.validate(owner: owner, projectID: run.projectID)
        guard run.checkpoint.status == .completed else { throw StoryAgentError.incompletePlan }
        try StoryAgentTools.validateCompletion(run)
        let url = directory(owner: owner, projectID: run.projectID).appendingPathComponent("project.json")
        let current = try JSONDecoder().decode(StoryProject.self, from: Data(contentsOf: url))
        let digest = try StoryAgentRun.digest(current)
        let draftDigest = try StoryAgentRun.digest(run.draft)
        guard digest == run.baseDigest || digest == draftDigest else { throw StoryAgentError.projectChanged }
        var next = run.draft; next.updatedAt = Date()
        if digest != (try StoryAgentRun.digest(next)) { try save(next, owner: owner) }
        else { next = current }
        run.draft = next; run.applied = true; run.updatedAt = Date()
        try saveRun(run, owner: owner)
        return (run, next)
    }

    func saveImage(_ data: Data, mimeType: String, projectID: UUID, owner: String) throws -> StoryImage {
        guard !data.isEmpty, data.count <= 20 * 1024 * 1024,
              ["image/png", "image/jpeg", "image/webp"].contains(mimeType) else { throw StoryError.unsafeFile }
        let ext = mimeType == "image/jpeg" ? "jpg" : mimeType == "image/webp" ? "webp" : "png"
        let image = StoryImage(filename: "\(UUID().uuidString).\(ext)", mimeType: mimeType)
        try saveFile(data, filename: image.filename, projectID: projectID, owner: owner)
        return image
    }

    func saveVideo(_ result: VideoGenerationResult, projectID: UUID, owner: String) throws -> StoryVideo {
        guard !result.videoData.isEmpty, result.videoData.count <= 512 * 1024 * 1024 else { throw StoryError.unsafeFile }
        let video = StoryVideo(filename: "\(UUID().uuidString).mp4", jobID: result.id, modelName: result.modelName)
        try saveFile(result.videoData, filename: video.filename, projectID: projectID, owner: owner)
        return video
    }

    private func saveFile(_ data: Data, filename: String, projectID: UUID, owner: String) throws {
        let folder = directory(owner: owner, projectID: projectID)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let url = try fileURL(filename, projectID: projectID, owner: owner)
        try data.write(to: url, options: .atomic)
    }

    nonisolated func fileURL(_ filename: String, projectID: UUID, owner: String) throws -> URL {
        guard filename == URL(fileURLWithPath: filename).lastPathComponent,
              !filename.hasPrefix("."), !filename.isEmpty, !filename.contains("/") else { throw StoryError.unsafeFile }
        let folder = directory(owner: owner, projectID: projectID).resolvingSymlinksInPath()
        let file = folder.appendingPathComponent(filename).resolvingSymlinksInPath()
        guard file.deletingLastPathComponent() == folder else { throw StoryError.unsafeFile }
        return file
    }

    private nonisolated func accountDirectory(_ owner: String) -> URL {
        let key = SHA256.hash(data: Data(owner.utf8)).map { String(format: "%02x", $0) }.joined()
        return root.appendingPathComponent(key, isDirectory: true)
    }
    private nonisolated func directory(owner: String, projectID: UUID) -> URL {
        accountDirectory(owner).appendingPathComponent(projectID.uuidString, isDirectory: true)
    }
}
