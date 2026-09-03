import Foundation

enum NativeBrowserExtensionPairingStatus {
    private struct PersistedPairing: Decodable {
        var protocolVersion: String
        var allowedExtensionOrigin: String

        enum CodingKeys: String, CodingKey {
            case protocolVersion = "protocol_version"
            case allowedExtensionOrigin = "allowed_extension_origin"
        }
    }

    static func isPaired(at url: URL) -> Bool {
        guard let values = try? url.resourceValues(forKeys: [
            .isRegularFileKey,
            .isSymbolicLinkKey,
            .fileSizeKey,
        ]),
              values.isRegularFile == true,
              values.isSymbolicLink != true,
              (values.fileSize ?? (16 * 1024 + 1)) <= 16 * 1024,
              let data = try? Data(contentsOf: url, options: .mappedIfSafe),
              let pairing = try? JSONDecoder().decode(PersistedPairing.self, from: data) else {
            return false
        }
        return pairing.protocolVersion == "1.0"
            && pairing.allowedExtensionOrigin
                == "chrome-extension://jooaepjckiofmpldinopgdgddcoaofil/"
    }
}

actor NativeBrowserExtensionPairingRuntime {
    private var client: NativePluginStdioClient?
    private var expirationTask: Task<Void, Never>?
    private let lifetime: Duration

    init(lifetime: Duration = .seconds(600)) {
        self.lifetime = lifetime
    }

    func start(launch: NativePreparedPluginLaunch) async throws {
        await stop()
        let nextClient = NativePluginStdioClient(launch: launch)
        do {
            try await nextClient.start()
            _ = try await nextClient.initialize()
        } catch {
            await nextClient.terminate()
            throw error
        }
        client = nextClient
        let lifetime = lifetime
        expirationTask = Task { [weak self] in
            try? await Task.sleep(for: lifetime)
            guard !Task.isCancelled else { return }
            await self?.stop()
        }
    }

    func stop() async {
        expirationTask?.cancel()
        expirationTask = nil
        guard let client else { return }
        self.client = nil
        await client.terminate()
    }

    func isRunning() -> Bool {
        client != nil
    }
}
