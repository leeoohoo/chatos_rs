import ChatOSCore
import Foundation

/// One source of truth for both individual clicks and deterministic batch generation.
enum StoryGenerationContext {
    static func assetPrompt(_ project: StoryProject, resource: StoryResource) -> String {
        StoryPromptRegistry.render(.assetImage, values: ["style": project.style, "resourcePrompt": resource.prompt])
    }

    static func text(_ project: StoryProject, segment: StorySegment) throws -> String {
        try text(project, segment: segment, imageReferenceIDs: segment.resourceIDs,
                 previousTailReferenceIndex: nil, currentFirstFrameReferenceIndex: nil)
    }

    private static func text(_ project: StoryProject, segment: StorySegment,
                             imageReferenceIDs: [String],
                             previousTailReferenceIndex: Int?,
                             currentFirstFrameReferenceIndex: Int?) throws -> String {
        try project.validate()
        guard Set(imageReferenceIDs).count == imageReferenceIDs.count,
              imageReferenceIDs.allSatisfy(segment.resourceIDs.contains) else { throw StoryError.invalidPlan }
        if let previousTailReferenceIndex {
            guard previousTailReferenceIndex == imageReferenceIDs.count + 1,
                  StoryContinuityContext.previousTail(project, segmentID: segment.id) != nil else {
                throw StoryError.invalidPlan
            }
        }
        if let currentFirstFrameReferenceIndex {
            guard previousTailReferenceIndex == nil,
                  currentFirstFrameReferenceIndex == imageReferenceIDs.count + 1,
                  segment.firstFrame != nil else { throw StoryError.invalidPlan }
        }
        let resources = try segment.resourceIDs.map { id -> [String: Any] in
            guard let resource = project.resource(id: id) else { throw StoryError.invalidPlan }
            var value: [String: Any] = [
                "id": id, "name": resource.name, "kind": resource.kind.rawValue,
                "visualDescription": resource.prompt,
                "hasReferenceImage": imageReferenceIDs.contains(id),
            ]
            if let index = imageReferenceIDs.firstIndex(of: id) { value["referenceIndex"] = index + 1 }
            if let profile = resource.characterProfile { value["characterProfile"] = try JSONSerialization.jsonObject(with: JSONEncoder().encode(profile)) }
            if let profile = resource.sceneProfile { value["sceneProfile"] = try JSONSerialization.jsonObject(with: JSONEncoder().encode(profile)) }
            return value
        }
        let relations = try JSONSerialization.jsonObject(with: JSONEncoder().encode(project.relations(for: segment.id)))
        var context: [String: Any] = ["segment": segment.title, "storyBeat": segment.synopsis, "style": project.style, "ratio": project.ratio,
            "resources": resources, "characterSceneRelations": relations,
            "adjacentContinuity": try StoryContinuityContext.context(project, segmentID: segment.id),
            "relationshipSource": StoryPromptRegistry.render(.frameRelationshipRule)]
        if let previousTailReferenceIndex {
            context["previousSegmentTailReferenceIndex"] = previousTailReferenceIndex
            context["continuityReferenceRule"] = StoryPromptRegistry.render(.previousTailRule)
        }
        if let currentFirstFrameReferenceIndex {
            context["currentSegmentFirstFrameReferenceIndex"] = currentFirstFrameReferenceIndex
            context["continuityReferenceRule"] = StoryPromptRegistry.render(.currentFirstRule, values: [
                "seconds": "\(segment.seconds)",
            ])
        }
        return StoryPromptRegistry.render(.frameContext)
            + String(decoding: try JSONSerialization.data(withJSONObject: context, options: [.sortedKeys]), as: UTF8.self)
    }
    static func firstFramePrompt(_ project: StoryProject, segment: StorySegment) throws -> String {
        try firstFramePrompt(project, segment: segment, referenceResourceIDs: segment.resourceIDs)
    }
    static func firstFramePrompt(_ project: StoryProject, segment: StorySegment,
                                 referenceResourceIDs: [String],
                                 previousTailReferenceIndex: Int? = nil) throws -> String {
        guard let detail = segment.detail else { throw StoryError.invalidPlan }
        guard !referenceResourceIDs.isEmpty else { throw StoryError.invalidPlan }
        return try text(project, segment: segment, imageReferenceIDs: referenceResourceIDs,
                        previousTailReferenceIndex: previousTailReferenceIndex,
                        currentFirstFrameReferenceIndex: nil)
            + StoryPromptRegistry.render(.firstFrameImage, values: [
                "seconds": "\(segment.seconds)", "videoPrompt": detail.videoPrompt,
                "framePrompt": detail.firstFramePrompt,
            ])
    }
    static func lastFramePrompt(_ project: StoryProject, segment: StorySegment) throws -> String {
        try lastFramePrompt(project, segment: segment, referenceResourceIDs: segment.resourceIDs)
    }
    static func lastFramePrompt(_ project: StoryProject, segment: StorySegment,
                                referenceResourceIDs: [String],
                                currentFirstFrameReferenceIndex: Int? = nil) throws -> String {
        guard let detail = segment.detail else { throw StoryError.invalidPlan }
        guard !referenceResourceIDs.isEmpty else { throw StoryError.invalidPlan }
        return try text(project, segment: segment, imageReferenceIDs: referenceResourceIDs,
                        previousTailReferenceIndex: nil,
                        currentFirstFrameReferenceIndex: currentFirstFrameReferenceIndex)
            + StoryPromptRegistry.render(.lastFrameImage, values: [
                "seconds": "\(segment.seconds)", "videoPrompt": detail.videoPrompt,
                "framePrompt": detail.effectiveLastFramePrompt,
            ])
    }
    static func framePrompt(_ project: StoryProject, segment: StorySegment, role: StoryFrameRole,
                            referenceResourceIDs: [String],
                            previousTailReferenceIndex: Int? = nil,
                            currentFirstFrameReferenceIndex: Int? = nil) throws -> String {
        switch role {
        case .first: try firstFramePrompt(project, segment: segment, referenceResourceIDs: referenceResourceIDs,
                                          previousTailReferenceIndex: previousTailReferenceIndex)
        case .last: try lastFramePrompt(project, segment: segment, referenceResourceIDs: referenceResourceIDs,
                                        currentFirstFrameReferenceIndex: currentFirstFrameReferenceIndex)
        }
    }
    static func videoPrompt(_ project: StoryProject, segment: StorySegment) throws -> String {
        guard let detail = segment.detail else { throw StoryError.invalidPlan }
        try project.validate()

        let shotBudget = max(240, 2_400 / max(1, detail.shots.count))
        let shots = detail.shots.map {
            "\($0.start)–\($0.end)秒：" + bounded($0.prompt, limit: shotBudget)
        }.joined(separator: "\n")
        let resourceBudget = max(100, 900 / max(1, segment.resourceIDs.count))
        let resources = segment.resourceIDs.compactMap { id -> String? in
            guard let resource = project.resource(id: id) else { return nil }
            return "- \(resource.name)[\(resource.kind.rawValue)]：" + bounded(resource.prompt, limit: resourceBudget)
        }.joined(separator: "\n")
        let relationsForSegment = project.relations(for: segment.id)
        let relationBudget = max(100, 500 / max(1, relationsForSegment.count))
        let relations = relationsForSegment.map {
            "- \($0.characterID) @ \($0.sceneID) \($0.startSecond)–\($0.endSecond)秒："
                + bounded("\($0.action)；位置 \($0.position)", limit: relationBudget)
        }.joined(separator: "\n")

        var adjacent: [String] = []
        if let index = project.segments.firstIndex(where: { $0.id == segment.id }) {
            if index > 0, let previous = project.segments[index - 1].detail {
                adjacent.append("上一段结束：" + bounded(previous.effectiveLastFramePrompt + "；" + previous.continuityOut, limit: 200))
            }
            if index + 1 < project.segments.count, let next = project.segments[index + 1].detail {
                adjacent.append("下一段开始：" + bounded(next.firstFramePrompt + "；" + next.continuityIn, limit: 200))
            }
        }

        let key: StoryPromptRegistry.Key = segment.kind == .transition ? .transitionVideo : .storyVideo
        let prompt = StoryPromptRegistry.render(key, values: [
            "seconds": "\(segment.seconds)",
            "segment": bounded(segment.title + "；" + segment.synopsis, limit: 250),
            "style": bounded(project.style, limit: 350), "shots": shots,
            "continuityIn": bounded(detail.continuityIn, limit: 250),
            "continuityOut": bounded(detail.continuityOut, limit: 250),
            "firstFrame": bounded(detail.firstFramePrompt, limit: 250),
            "lastFrame": bounded(detail.effectiveLastFramePrompt, limit: 250),
            "resources": resources, "relations": relations,
            "adjacent": adjacent.joined(separator: "\n"),
            "audio": bounded(detail.audio, limit: 150),
            "constraints": bounded(detail.constraints, limit: 250),
        ])
        // MiniMax H3 accepts at most 7,000 characters. Leave room for gateways that add
        // small protocol annotations while preserving every timed shot above.
        return bounded(prompt, limit: 6_800)
    }

    private static func bounded(_ value: String, limit: Int) -> String {
        guard value.count > limit else { return value }
        return String(value.prefix(max(0, limit - 1))) + "…"
    }
}
