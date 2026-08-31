import Foundation

enum MessageTaskGraphAlgorithms {
    static func transitiveReduction(
        _ edges: [MessageTaskGraphEdge]
    ) -> [MessageTaskGraphEdge] {
        let adjacency = Dictionary(grouping: edges, by: \.sourceID)
            .mapValues { $0.map(\.targetID) }
        guard isAcyclic(edges) else { return edges }
        return edges.filter { edge in
            !pathExists(
                from: edge.sourceID,
                to: edge.targetID,
                adjacency: adjacency,
                excluding: edge
            )
        }
    }

    static func isReviewTitle(_ title: String) -> Bool {
        title.range(of: "review", options: .caseInsensitive) != nil
            || title.contains("复核") || title.contains("审查") || title.contains("验收")
    }

    static func aggregateStatus(_ statuses: [String?]) -> String? {
        let values = statuses.compactMap { $0?.lowercased() }
        func contains(_ candidates: [String]) -> Bool {
            !values.filter(candidates.contains).isEmpty
        }
        if contains(["failed", "error"]) { return "failed" }
        if contains(["running", "processing", "in_progress", "doing"]) { return "running" }
        if contains(["blocked"]) { return "blocked" }
        if contains(["queued", "ready", "todo", "pending"]) { return "ready" }
        if contains(["cancelled", "canceled"]) { return "cancelled" }
        let completed = ["succeeded", "success", "completed", "done"]
        if !values.isEmpty && values.allSatisfy(completed.contains) { return "completed" }
        return values.first
    }

    private static func isAcyclic(_ edges: [MessageTaskGraphEdge]) -> Bool {
        let ids = Set(edges.flatMap { [$0.sourceID, $0.targetID] })
        var indegree = Dictionary(uniqueKeysWithValues: ids.map { ($0, 0) })
        var adjacency: [String: [String]] = [:]
        for edge in edges {
            adjacency[edge.sourceID, default: []].append(edge.targetID)
            indegree[edge.targetID, default: 0] += 1
        }
        var queue = Array(ids.filter { indegree[$0] == 0 })
        var visited = 0
        while let current = queue.popLast() {
            visited += 1
            for next in adjacency[current] ?? [] {
                indegree[next, default: 0] -= 1
                if indegree[next] == 0 { queue.append(next) }
            }
        }
        return visited == ids.count
    }

    private static func pathExists(
        from source: String,
        to target: String,
        adjacency: [String: [String]],
        excluding edge: MessageTaskGraphEdge
    ) -> Bool {
        var pending = adjacency[source]?.filter { $0 != edge.targetID } ?? []
        var visited: Set<String> = []
        while let current = pending.popLast() {
            if current == target { return true }
            guard visited.insert(current).inserted else { continue }
            pending.append(contentsOf: adjacency[current] ?? [])
        }
        return false
    }
}
