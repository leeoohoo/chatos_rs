import Foundation

enum AgentAttachmentDataLoader {
    static let maximumBytes = 20 * 1_024 * 1_024

    static func load(_ url: URL) throws -> Data {
        guard url.isFileURL,
              let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey]),
              values.isRegularFile == true,
              let fileSize = values.fileSize,
              fileSize > 0,
              fileSize <= maximumBytes,
              let handle = try? FileHandle(forReadingFrom: url) else {
            throw AgentRuntimeError.invalidAttachment
        }
        defer { try? handle.close() }
        guard let data = try? handle.read(upToCount: maximumBytes + 1),
              !data.isEmpty,
              data.count <= maximumBytes else {
            throw AgentRuntimeError.invalidAttachment
        }
        return data
    }
}
