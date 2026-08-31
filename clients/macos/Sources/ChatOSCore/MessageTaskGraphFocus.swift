import Foundation

public struct MessageTaskGraphFocusContext: Sendable, Equatable {
    public var relatedTaskIDs: Set<String>
    public var directTaskIDs: Set<String>

    public init(relatedTaskIDs: Set<String>, directTaskIDs: Set<String>) {
        self.relatedTaskIDs = relatedTaskIDs
        self.directTaskIDs = directTaskIDs
    }
}

public enum MessageTaskGraphFocus {
    public static func context(
        selectedTaskID: String?,
        edges: [MessageTaskGraphEdge]
    ) -> MessageTaskGraphFocusContext? {
        guard let selectedTaskID, !selectedTaskID.isEmpty else { return nil }

        let parents = Dictionary(grouping: edges, by: \.targetID)
            .mapValues { $0.map(\.sourceID) }
        let children = Dictionary(grouping: edges, by: \.sourceID)
            .mapValues { $0.map(\.targetID) }
        let upstream = reachable(from: selectedTaskID, adjacency: parents)
        let downstream = reachable(from: selectedTaskID, adjacency: children)
        let direct = Set([selectedTaskID]
            + (parents[selectedTaskID] ?? [])
            + (children[selectedTaskID] ?? []))

        return MessageTaskGraphFocusContext(
            relatedTaskIDs: upstream.union(downstream).union([selectedTaskID]),
            directTaskIDs: direct
        )
    }

    private static func reachable(
        from startID: String,
        adjacency: [String: [String]]
    ) -> Set<String> {
        var result = Set<String>()
        var pending = adjacency[startID] ?? []
        while let current = pending.popLast() {
            guard result.insert(current).inserted else { continue }
            pending.append(contentsOf: adjacency[current] ?? [])
        }
        return result
    }
}
