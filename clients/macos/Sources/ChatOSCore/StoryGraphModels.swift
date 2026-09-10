import Foundation

/// A normalized read model for the project, three primary entity tables and association table.
public struct StoryGraphNode: Codable, Equatable, Identifiable, Sendable {
    public enum Kind: String, Codable, Sendable { case project, character, scene, segment, prop }
    public let id: String
    public let entityID: String
    public let kind: Kind
    public let name: String
    public let summary: String
    public let hasWrittenProfile: Bool
    public let hasConfirmedImage: Bool
    public let hasVideo: Bool
}

public struct StoryGraphEdge: Codable, Equatable, Identifiable, Sendable {
    public enum Kind: String, Codable, Sendable { case projectContains, segmentReferences, characterInScene }
    public let id: String
    public let kind: Kind
    public let from: String
    public let to: String
    public let segmentID: String?
    public let action: String?
    public let position: String?
    public let startSecond: Int?
    public let endSecond: Int?
}

public struct StoryGraphPage: Codable, Equatable, Sendable {
    public let nodeCount: Int
    public let edgeCount: Int
    public let nodes: [StoryGraphNode]
    public let edges: [StoryGraphEdge]
    public let nextNodeOffset: Int?
    public let nextEdgeOffset: Int?
}

public extension StoryProject {
    func graphPage(nodeOffset: Int, edgeOffset: Int, limit: Int) throws -> StoryGraphPage {
        try validate()
        guard nodeOffset >= 0, edgeOffset >= 0, (1...50).contains(limit) else { throw StoryError.invalidPlan }
        let projectNode = StoryGraphNode(id: "project:\(id)", entityID: id.uuidString, kind: .project,
            name: title, summary: String(summary.prefix(300)), hasWrittenProfile: !summary.isEmpty,
            hasConfirmedImage: false, hasVideo: false)
        let characterNodes = characters.map {
            StoryGraphNode(id: "character:\($0.id)", entityID: $0.id, kind: .character, name: $0.name,
                summary: String($0.imagePrompt.prefix(300)), hasWrittenProfile: true,
                hasConfirmedImage: $0.media.confirmedImage != nil, hasVideo: false)
        }
        let sceneNodes = scenes.map {
            StoryGraphNode(id: "scene:\($0.id)", entityID: $0.id, kind: .scene, name: $0.name,
                summary: String($0.imagePrompt.prefix(300)), hasWrittenProfile: true,
                hasConfirmedImage: $0.media.confirmedImage != nil, hasVideo: false)
        }
        let propNodes = props.map {
            StoryGraphNode(id: "prop:\($0.id)", entityID: $0.id, kind: .prop, name: $0.name,
                summary: String($0.imagePrompt.prefix(300)), hasWrittenProfile: true,
                hasConfirmedImage: $0.media.confirmedImage != nil, hasVideo: false)
        }
        let segmentNodes = segments.map {
            StoryGraphNode(id: "segment:\($0.id)", entityID: $0.id, kind: .segment, name: $0.title,
                summary: String($0.synopsis.prefix(300)), hasWrittenProfile: $0.detail != nil,
                hasConfirmedImage: $0.firstFrame != nil, hasVideo: $0.video != nil)
        }
        let nodes = [projectNode] + characterNodes + sceneNodes + propNodes + segmentNodes
        var edges: [StoryGraphEdge] = []
        for node in nodes.dropFirst() {
            edges.append(.init(id: "contains:\(node.id)", kind: .projectContains, from: projectNode.id,
                to: node.id, segmentID: nil, action: nil, position: nil, startSecond: nil, endSecond: nil))
        }
        for segment in segments {
            for id in segment.characterIDs {
                edges.append(.init(id: "reference:\(segment.id):character:\(id)", kind: .segmentReferences,
                    from: "segment:\(segment.id)", to: "character:\(id)", segmentID: segment.id,
                    action: nil, position: nil, startSecond: nil, endSecond: nil))
            }
            for id in segment.sceneIDs {
                edges.append(.init(id: "reference:\(segment.id):scene:\(id)", kind: .segmentReferences,
                    from: "segment:\(segment.id)", to: "scene:\(id)", segmentID: segment.id,
                    action: nil, position: nil, startSecond: nil, endSecond: nil))
            }
            for id in segment.propIDs {
                edges.append(.init(id: "reference:\(segment.id):prop:\(id)", kind: .segmentReferences,
                    from: "segment:\(segment.id)", to: "prop:\(id)", segmentID: segment.id,
                    action: nil, position: nil, startSecond: nil, endSecond: nil))
            }
        }
        edges += relations.map {
            .init(id: "appearance:\($0.id)", kind: .characterInScene,
                from: "character:\($0.characterID)", to: "scene:\($0.sceneID)", segmentID: $0.segmentID,
                action: $0.action, position: $0.position, startSecond: $0.startSecond, endSecond: $0.endSecond)
        }
        let nodePage = Array(nodes.dropFirst(nodeOffset).prefix(limit))
        let edgePage = Array(edges.dropFirst(edgeOffset).prefix(limit))
        return .init(nodeCount: nodes.count, edgeCount: edges.count, nodes: nodePage, edges: edgePage,
            nextNodeOffset: nodeOffset + nodePage.count < nodes.count ? nodeOffset + nodePage.count : nil,
            nextEdgeOffset: edgeOffset + edgePage.count < edges.count ? edgeOffset + edgePage.count : nil)
    }
}
