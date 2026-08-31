import Foundation

struct MetadataFileSearchRecord: Sendable, Hashable {
    let url: URL
    let displayName: String
    let contentType: String?
}

@MainActor
final class MetadataFileSearchProvider {
    private var activeSession: MetadataFileQuerySession?

    func search(_ text: String) async -> [MetadataFileSearchRecord] {
        activeSession?.cancel()
        let session = MetadataFileQuerySession(text: text)
        activeSession = session
        let results = await session.results()
        if activeSession === session {
            activeSession = nil
        }
        return results
    }

    func cancel() {
        activeSession?.cancel()
        activeSession = nil
    }
}

@MainActor
private final class MetadataFileQuerySession: NSObject {
    private let query = NSMetadataQuery()
    private let text: String
    private var continuation: CheckedContinuation<[MetadataFileSearchRecord], Never>?
    private var observers: [NSObjectProtocol] = []
    private var hasFinished = false

    init(text: String) {
        self.text = text
        super.init()
    }

    func results() async -> [MetadataFileSearchRecord] {
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return [] }
        return await withCheckedContinuation { continuation in
            self.continuation = continuation
            query.searchScopes = [NSMetadataQueryLocalComputerScope]
            query.predicate = NSPredicate(
                format: "((%K CONTAINS[cd] %@) OR (%K CONTAINS[cd] %@)) AND %K != 1",
                NSMetadataItemFSNameKey,
                text,
                NSMetadataItemDisplayNameKey,
                text,
                "kMDItemFSInvisible"
            )
            query.sortDescriptors = [
                NSSortDescriptor(key: NSMetadataItemFSContentChangeDateKey, ascending: false),
            ]
            let center = NotificationCenter.default
            observers.append(center.addObserver(
                forName: .NSMetadataQueryDidFinishGathering,
                object: query,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.finishWithCurrentResults() }
            })
            observers.append(center.addObserver(
                forName: .NSMetadataQueryDidUpdate,
                object: query,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self, self.query.resultCount >= 50 else { return }
                    self.finishWithCurrentResults()
                }
            })
            if !query.start() {
                finish([])
            }
        }
    }

    func cancel() {
        finish([])
    }

    private func finishWithCurrentResults() {
        query.disableUpdates()
        let count = min(50, query.resultCount)
        var records: [MetadataFileSearchRecord] = []
        records.reserveCapacity(count)
        for index in 0..<count {
            guard let item = query.result(at: index) as? NSMetadataItem,
                  let url = item.value(forAttribute: NSMetadataItemURLKey) as? URL else {
                continue
            }
            let displayName = (item.value(forAttribute: NSMetadataItemDisplayNameKey) as? String)
                ?? url.lastPathComponent
            records.append(MetadataFileSearchRecord(
                url: url,
                displayName: displayName,
                contentType: item.value(forAttribute: NSMetadataItemContentTypeKey) as? String
            ))
        }
        finish(records)
    }

    private func finish(_ records: [MetadataFileSearchRecord]) {
        guard !hasFinished else { return }
        hasFinished = true
        query.stop()
        let center = NotificationCenter.default
        observers.forEach(center.removeObserver)
        observers.removeAll()
        continuation?.resume(returning: records)
        continuation = nil
    }
}
