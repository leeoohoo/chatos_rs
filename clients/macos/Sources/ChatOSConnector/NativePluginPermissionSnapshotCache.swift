import ChatOSCore
import Foundation

actor NativePluginPermissionSnapshotCache {
    private struct Key: Hashable, Sendable {
        let pluginID: String
        let releaseID: String
        let artifactSHA256: String
        let installationPath: String

        init(record: NativeInstalledPluginRecord) {
            pluginID = record.pluginID
            releaseID = record.releaseID
            artifactSHA256 = record.artifactSHA256
            installationPath = record.installationPath
        }
    }

    private struct Entry: Sendable {
        let generation: Int
        let permissions: [LocalConnectorPluginPermission]
        let expiresAt: Date
    }

    private struct Refresh: Sendable {
        let generation: Int
        let task: Task<[LocalConnectorPluginPermission], Never>
    }

    private static let timeToLive: TimeInterval = 5
    private var generations: [Key: Int] = [:]
    private var entries: [Key: Entry] = [:]
    private var refreshes: [Key: Refresh] = [:]

    func permissions(
        record: NativeInstalledPluginRecord,
        manifest: NativePluginManifest,
        forceRefresh: Bool = false
    ) async throws -> [LocalConnectorPluginPermission] {
        try Task.checkCancellation()
        guard manifest.name == "open-computer-use" else {
            return await NativePluginPermissionInspector.permissions(
                record: record,
                manifest: manifest
            )
        }

        let key = Key(record: record)
        if forceRefresh {
            invalidate(key: key)
        }
        let generation = generations[key, default: 0]
        let now = Date()
        if let entry = entries[key],
           Self.snapshotIsUsable(
               snapshotGeneration: entry.generation,
               currentGeneration: generation,
               expiresAt: entry.expiresAt,
               now: now
           ) {
            return entry.permissions
        }

        if let refresh = refreshes[key], refresh.generation == generation {
            let value = await refresh.task.value
            return try finalize(
                value,
                key: key,
                generation: refresh.generation
            )
        }

        let task = Task {
            await NativePluginPermissionInspector.permissions(
                record: record,
                manifest: manifest
            )
        }
        refreshes[key] = .init(generation: generation, task: task)
        let value = await task.value
        return try finalize(value, key: key, generation: generation)
    }

    func invalidate(record: NativeInstalledPluginRecord) {
        invalidate(key: Key(record: record))
    }

    func invalidateAll() {
        let knownKeys = Set(generations.keys)
            .union(entries.keys)
            .union(refreshes.keys)
        for key in knownKeys {
            generations[key, default: 0] &+= 1
        }
        refreshes.values.forEach { $0.task.cancel() }
        entries.removeAll(keepingCapacity: false)
        refreshes.removeAll(keepingCapacity: false)
    }

    private func invalidate(key: Key) {
        generations[key, default: 0] &+= 1
        entries[key] = nil
        refreshes.removeValue(forKey: key)?.task.cancel()
    }

    private func finalize(
        _ value: [LocalConnectorPluginPermission],
        key: Key,
        generation: Int
    ) throws -> [LocalConnectorPluginPermission] {
        let currentGeneration = generations[key, default: 0]
        guard Self.refreshIsCurrent(
            expectedGeneration: generation,
            currentGeneration: currentGeneration,
            isCancelled: Task.isCancelled
        ) else {
            throw CancellationError()
        }
        let now = Date()
        if let entry = entries[key],
           Self.snapshotIsUsable(
               snapshotGeneration: entry.generation,
               currentGeneration: generation,
               expiresAt: entry.expiresAt,
               now: now
           ) {
            if refreshes[key]?.generation == generation {
                refreshes[key] = nil
            }
            return entry.permissions
        }
        entries[key] = .init(
            generation: generation,
            permissions: value,
            expiresAt: now.addingTimeInterval(Self.timeToLive)
        )
        if refreshes[key]?.generation == generation {
            refreshes[key] = nil
        }
        return value
    }

    static func snapshotIsUsable(
        snapshotGeneration: Int,
        currentGeneration: Int,
        expiresAt: Date,
        now: Date
    ) -> Bool {
        snapshotGeneration == currentGeneration && expiresAt > now
    }

    static func refreshIsCurrent(
        expectedGeneration: Int,
        currentGeneration: Int,
        isCancelled: Bool
    ) -> Bool {
        !isCancelled && expectedGeneration == currentGeneration
    }
}
