import Foundation

public enum MessageTaskGraphDisplayMode: String, Sendable, CaseIterable {
    case reduced
    case full
}

public enum MessageTaskGraphNormalizer {
    public static func normalize(
        _ graph: MessageTaskGraphSnapshot,
        mode: MessageTaskGraphDisplayMode
    ) -> MessageTaskGraphSnapshot {
        let validNodes = graph.nodes.filter { !$0.task.id.isEmpty }
        let nodeIDs = Set(validNodes.map(\.id))
        var edges = inferredEdges(nodes: validNodes, fallback: graph.edges, nodeIDs: nodeIDs)

        if mode == .full {
            edges = addingContextEdges(nodes: validNodes, edges: edges, nodeIDs: nodeIDs)
        }

        var snapshot = MessageTaskGraphSnapshot(
            rootTaskIDs: graph.rootTaskIDs,
            nodes: validNodes,
            edges: mode == .reduced ? MessageTaskGraphAlgorithms.transitiveReduction(edges) : edges,
            sourceSessionID: graph.sourceSessionID,
            sourceTurnID: graph.sourceTurnID,
            sourceUserMessageID: graph.sourceUserMessageID
        )
        if mode == .reduced {
            snapshot = collapseProjectTaskStages(snapshot)
        }
        return recomputeDepth(snapshot)
    }

    private static func inferredEdges(
        nodes: [MessageTaskGraphNode],
        fallback: [MessageTaskGraphEdge],
        nodeIDs: Set<String>
    ) -> [MessageTaskGraphEdge] {
        var byKey: [String: MessageTaskGraphEdge] = [:]
        for node in nodes {
            for prerequisiteID in node.task.prerequisiteTaskIDs where nodeIDs.contains(prerequisiteID) {
                let key = "\(prerequisiteID)->\(node.id)"
                byKey[key] = MessageTaskGraphEdge(
                    id: key,
                    sourceID: prerequisiteID,
                    targetID: node.id
                )
            }
        }
        if byKey.isEmpty {
            for edge in fallback
            where nodeIDs.contains(edge.sourceID) && nodeIDs.contains(edge.targetID) && edge.sourceID != edge.targetID {
                let key = "\(edge.sourceID)->\(edge.targetID)"
                byKey[key] = MessageTaskGraphEdge(
                    id: key,
                    sourceID: edge.sourceID,
                    targetID: edge.targetID,
                    kind: edge.kind
                )
            }
        }
        return Array(byKey.values)
    }

    private static func addingContextEdges(
        nodes: [MessageTaskGraphNode],
        edges: [MessageTaskGraphEdge],
        nodeIDs: Set<String>
    ) -> [MessageTaskGraphEdge] {
        var result = Dictionary(uniqueKeysWithValues: edges.map { ($0.id, $0) })
        let taskIDByClientRef = Dictionary(
            uniqueKeysWithValues: nodes.compactMap { node in
                node.task.executionClientRef.map { ($0, node.id) }
            }
        )
        for node in nodes {
            for contextRef in node.task.dependencyContextRefs {
                guard let sourceID = taskIDByClientRef[contextRef],
                      nodeIDs.contains(sourceID), sourceID != node.id else { continue }
                let key = "\(sourceID)->\(node.id)"
                result[key] = MessageTaskGraphEdge(
                    id: key,
                    sourceID: sourceID,
                    targetID: node.id,
                    kind: "context"
                )
            }
        }
        return Array(result.values)
    }

    private static func collapseProjectTaskStages(
        _ graph: MessageTaskGraphSnapshot
    ) -> MessageTaskGraphSnapshot {
        let groups = Dictionary(grouping: graph.nodes) { node in
            node.task.projectTaskID.map { "project:\($0)" } ?? "task:\(node.id)"
        }
        var representativeByTaskID: [String: String] = [:]
        var nodes: [MessageTaskGraphNode] = []

        for group in groups.values {
            guard var representative = group.first(where: {
                !MessageTaskGraphAlgorithms.isReviewTitle($0.task.title)
            }) ?? group.first else {
                continue
            }
            let tasks = group.map(\.task)
            for node in group { representativeByTaskID[node.id] = representative.id }
            representative.task.status = MessageTaskGraphAlgorithms.aggregateStatus(tasks.map(\.status))
            representative.isCurrentMessage = group.contains(where: \.isCurrentMessage)
            representative.isRoot = group.contains(where: \.isRoot)
            representative.groupedTasks = tasks
            nodes.append(representative)
        }

        var edgeByKey: [String: MessageTaskGraphEdge] = [:]
        for edge in graph.edges {
            let source = representativeByTaskID[edge.sourceID] ?? edge.sourceID
            let target = representativeByTaskID[edge.targetID] ?? edge.targetID
            guard source != target else { continue }
            let key = "\(source)->\(target)"
            edgeByKey[key] = MessageTaskGraphEdge(
                id: key,
                sourceID: source,
                targetID: target,
                kind: edge.kind
            )
        }
        let edges = MessageTaskGraphAlgorithms.transitiveReduction(Array(edgeByKey.values))
        let prerequisites = Dictionary(grouping: edges, by: \.targetID)
            .mapValues { $0.map(\.sourceID) }
        nodes = nodes.map { node in
            var node = node
            node.task.prerequisiteTaskIDs = prerequisites[node.id] ?? []
            return node
        }
        return MessageTaskGraphSnapshot(
            rootTaskIDs: graph.rootTaskIDs.compactMap { representativeByTaskID[$0] },
            nodes: nodes,
            edges: edges,
            sourceSessionID: graph.sourceSessionID,
            sourceTurnID: graph.sourceTurnID,
            sourceUserMessageID: graph.sourceUserMessageID
        )
    }

    private static func recomputeDepth(_ graph: MessageTaskGraphSnapshot) -> MessageTaskGraphSnapshot {
        var depth = Dictionary(uniqueKeysWithValues: graph.nodes.map { ($0.id, 0) })
        for _ in graph.nodes.indices {
            var changed = false
            for edge in graph.edges {
                let candidate = (depth[edge.sourceID] ?? 0) + 1
                if candidate > (depth[edge.targetID] ?? 0) {
                    depth[edge.targetID] = candidate
                    changed = true
                }
            }
            if !changed { break }
        }
        var result = graph
        result.nodes = graph.nodes.map { node in
            var node = node
            node.depth = depth[node.id] ?? node.depth
            return node
        }
        return result
    }

}
