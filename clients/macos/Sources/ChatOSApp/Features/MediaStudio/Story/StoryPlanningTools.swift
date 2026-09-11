import ChatOSCore
import Foundation

/// Typed, validated fallback planning boundary used by tests and non-agent callers.
/// Model output is data: it cannot choose accounts/projects or invoke generation tools.
enum StoryPlanningTools {
    enum OptimizationTarget: String, Sendable { case source, style }
    struct OptimizationSuggestion: Codable, Equatable, Sendable {
        var optimizedText: String
        var rationale: String
    }
    struct Outline: Codable, Sendable {
        struct Prop: Codable, Sendable { var id: String; var name: String; var description: String }
        struct Segment: Codable, Sendable {
            var id: String
            var title: String
            var synopsis: String
            var kind: StorySegmentKind
            var seconds: Int
            var propIDs: [String]
        }
        var summary: String
        var props: [Prop]
        var segments: [Segment]
    }

    static func outlineRequest(_ project: StoryProject) throws -> StoryPlanningRequest {
        let string: [String: Any] = ["type": "string", "minLength": 1]
        let prop = object(["id": string, "name": string, "description": string])
        let segment = object(["id": string, "title": string, "synopsis": string,
                              "kind": ["type": "string", "enum": ["story", "transition"]],
                              "seconds": ["type": "integer", "minimum": 2, "maximum": 15],
                              "propIDs": ["type": "array", "items": string, "maxItems": 8]])
        let schema = object(["summary": string,
            "props": ["type": "array", "items": prop, "maxItems": 100],
            "segments": ["type": "array", "items": segment, "minItems": 1, "maxItems": 200]])
        return .init(modelConfigID: project.models.textModelID,
                     systemPrompt: StoryPromptRegistry.render(.fallbackOutline),
                     context: try contextJSON(["title": project.title, "description": project.description, "style": project.style, "story": project.source]),
                     toolName: "story_save_outline", schema: try JSONSerialization.data(withJSONObject: schema))
    }

    static func optimizationRequest(_ project: StoryProject, source: String, style: String,
                                    target: OptimizationTarget) throws -> StoryPlanningRequest {
        let limit = target == .source ? 80_000 : 2_000
        let current = target == .source ? source : style
        guard !current.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, current.count <= limit else {
            throw StoryError.invalidProject
        }
        let string: [String: Any] = ["type": "string", "minLength": 1]
        let schema = object(["optimizedText": string, "rationale": string])
        let key: StoryPromptRegistry.Key = target == .source ? .optimizeSource : .optimizeStyle
        return .init(modelConfigID: project.models.textModelID,
                     systemPrompt: StoryPromptRegistry.render(key),
                     context: try contextJSON(["title": project.title, "description": project.description,
                                          "target": target.rawValue, "story": source, "visualStyle": style]),
                     toolName: "story_suggest_optimized_text", schema: try JSONSerialization.data(withJSONObject: schema))
    }

    static func decodeOptimization(_ data: Data, target: OptimizationTarget) throws -> OptimizationSuggestion {
        let suggestion = try JSONDecoder().decode(OptimizationSuggestion.self, from: data)
        let limit = target == .source ? 80_000 : 2_000
        guard !suggestion.optimizedText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              suggestion.optimizedText.count <= limit, !suggestion.rationale.isEmpty,
              suggestion.rationale.count <= 2_000 else { throw StoryError.invalidPlan }
        return suggestion
    }

    static func applyOutline(_ data: Data, to project: StoryProject) throws -> StoryProject {
        guard project.segments.isEmpty else { throw StoryError.invalidPlan }
        let outline = try JSONDecoder().decode(Outline.self, from: data)
        guard !outline.summary.isEmpty, !outline.segments.isEmpty,
              outline.props.allSatisfy({ !$0.id.isEmpty && !$0.name.isEmpty && !$0.description.isEmpty }),
              outline.segments.allSatisfy({ value in
                  !value.id.isEmpty && !value.synopsis.isEmpty && value.propIDs.count <= 8
                      && (2...15).contains(value.seconds) && (value.kind != .transition || value.seconds <= 3)
              }) else {
            throw StoryError.invalidPlan
        }
        var result = project
        result.summary = outline.summary
        result.props = outline.props.map { .init(id: $0.id, name: $0.name, description: $0.description) }
        let sourceCount = max(1, result.source.count)
        let kinds = outline.segments.map(\.kind)
        guard kinds.first == .story, kinds.last == .story,
              !zip(kinds, kinds.dropFirst()).contains(where: { pair in
                  pair.0 == .transition && pair.1 == .transition
              }) else {
            throw StoryError.invalidPlan
        }
        let storyCount = kinds.filter { $0 == .story }.count
        guard storyCount > 0 else { throw StoryError.invalidPlan }
        var storyIndex = 0
        result.segments = outline.segments.map { value in
            let kind = value.kind
            let seconds = value.seconds
            let start = storyIndex * sourceCount / storyCount
            let end: Int
            if kind == .story {
                storyIndex += 1
                end = storyIndex * sourceCount / storyCount
            } else {
                end = start
            }
            return .init(id: value.id, title: value.title, synopsis: value.synopsis,
                         sourceRange: .init(start: start, end: end), kind: kind,
                         seconds: seconds, propIDs: value.propIDs)
        }
        try result.validate()
        return result
    }

    static func detailRequest(_ project: StoryProject, segmentID: String) throws -> StoryPlanningRequest {
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }) else { throw StoryError.invalidPlan }
        let segment = project.segments[index]
        let string: [String: Any] = ["type": "string"]
        let shot = object(["start": ["type": "integer", "minimum": 0, "maximum": max(0, segment.seconds - 1)],
                           "end": ["type": "integer", "minimum": 1, "maximum": segment.seconds], "prompt": string])
        let schema = object(["firstFramePrompt": string, "lastFramePrompt": string,
            "shots": ["type": "array", "minItems": 1, "maxItems": 8, "items": shot],
            "continuityIn": string, "continuityOut": string, "audio": string, "constraints": string])
        let context: [String: Any] = [
            "summary": project.summary, "style": project.style, "ratio": project.ratio,
            "outline": project.segments.map { ["id": $0.id, "synopsis": $0.synopsis] },
            "current": ["id": segment.id, "title": segment.title, "synopsis": segment.synopsis,
                        "kind": segment.kind.rawValue, "seconds": segment.seconds],
            "sourceExcerpt": try StoryContinuityContext.sourceExcerpt(project, segmentID: segmentID),
            "adjacentContinuity": try StoryContinuityContext.context(project, segmentID: segmentID),
            "characters": project.characters.filter { segment.characterIDs.contains($0.id) }.map { ["id": $0.id, "name": $0.name, "profile": $0.imagePrompt] },
            "scenes": project.scenes.filter { segment.sceneIDs.contains($0.id) }.map { ["id": $0.id, "name": $0.name, "profile": $0.imagePrompt] },
            "props": project.props.filter { segment.propIDs.contains($0.id) }.map { ["id": $0.id, "name": $0.name, "profile": $0.imagePrompt] },
            "relations": project.relations(for: segment.id).map { ["characterID": $0.characterID, "sceneID": $0.sceneID, "action": $0.action, "position": $0.position] },
        ]
        return .init(modelConfigID: project.models.textModelID,
                     systemPrompt: StoryPromptRegistry.render(.segmentDetail),
                     context: try contextJSON(context), toolName: "story_update_segment",
                     schema: try JSONSerialization.data(withJSONObject: schema))
    }

    static func decodeDetail(_ data: Data, duration: Int) throws -> StorySegmentDetail {
        let detail = try JSONDecoder().decode(StorySegmentDetail.self, from: data)
        try detail.validate(duration: duration)
        return detail
    }
    private static func object(_ properties: [String: Any]) -> [String: Any] {
        ["type": "object", "properties": properties, "required": Array(properties.keys).sorted(), "additionalProperties": false]
    }
    private static func contextJSON(_ value: [String: Any]) throws -> String {
        String(decoding: try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]), as: UTF8.self)
    }
}
