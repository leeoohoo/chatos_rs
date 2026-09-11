import ChatOSAgentRuntime
import ChatOSCore
import Foundation

enum StoryAgentTools {
    static var systemPrompt: String { StoryPromptRegistry.render(.agentSystem) }

    static func goalPrompt(stage: StoryAgentRun.Stage, sourceLength: Int, targetCount: Int) -> String {
        StoryPromptRegistry.render(stage == .outline ? .agentGoalOutline : .agentGoalRefine, values: [
            "stage": stage.rawValue, "sourceLength": "\(sourceLength)", "targetCount": "\(targetCount)",
        ])
    }

    static func definitions(stage: StoryAgentRun.Stage) throws -> [AgentToolDefinition] {
        var definitions: [AgentToolDefinition] = []
        func tool(_ name: String, _ description: String, _ fields: [String: Any], _ effect: AgentToolDefinition.Effect = .readOnly) throws {
            definitions.append(.init(name: name, description: description, schema: try JSONSerialization.data(withJSONObject: object(fields)), effect: effect))
        }
        try tool("story_read_state", "分页读取计划索引、授权目标、摘要和进度；readyToFinish=true 时不要再读取，立即单独调用 story_finish。", ["offset": integer(0, 200)])
        try tool("story_read_graph", "分页读取完整关系图谱：人物、场景、剧情分段节点，以及分段引用和人物在场景中的动作边。", [
            "nodeOffset": integer(0, 400), "edgeOffset": integer(0, 4_000), "limit": integer(1, 50),
        ])
        try tool("story_read_source", "分页读原文，使用返回的 nextOffset 继续；可重读已读位置。", ["offset": integer(0, 80_000), "limit": integer(1, 1_200)])
        try tool("story_read_text", "分页读取完整项目描述、风格或摘要，补全状态中的预览。", ["field": ["type": "string", "enum": ["description", "style", "summary"]], "offset": integer(0, 16_000), "limit": integer(1, 1_200)])
        try tool("story_read_segment", "细化前必须调用：读取单段原文、引用ID和关系，以及相邻段的完整镜头计划、首尾帧提示词、连续性状态和确认帧/视频状态。", ["segmentID": text(128, min: 1)])
        try tool("story_read_asset", "读取指定素材的外观提示词。", ["assetID": text(128, min: 1)])
        try tool("story_read_asset_prompt", "分页读取素材当前完整图片提示词，包括用户手动修改。", ["assetID": text(128, min: 1), "offset": integer(0, 4_000), "limit": integer(1, 1_200)])
        try tool("story_save_segment_relations", "原子保存当前分段的人物、至少一个场景、道具外键及人物—场景关系；没有场景的分段不能完成规划。", [
            "segmentID": text(128, min: 1),
            "characterIDs": ["type": "array", "items": text(128, min: 1), "maxItems": 8],
            "sceneIDs": ["type": "array", "items": text(128, min: 1), "minItems": 1, "maxItems": 8],
            "propIDs": ["type": "array", "items": text(128, min: 1), "maxItems": 8],
            "relations": ["type": "array", "maxItems": 8, "items": object([
                "relationID": text(128, min: 1), "characterID": text(128, min: 1), "sceneID": text(128, min: 1), "action": text(200, min: 1), "position": text(200, min: 1),
                "startSecond": integer(0, 14), "endSecond": integer(1, 15),
            ])],
        ], .write)
        if stage == .outline {
            try tool("story_save_summary", "保存全剧摘要，不生成媒体。", ["summary": text(2_000, min: 1)], .write)
            try tool("story_upsert_asset", "保存一个道具定义；角色和场景必须使用各自文字画像工具。", [
                "id": text(128, min: 1), "kind": ["type": "string", "enum": ["prop"]],
                "name": text(120, min: 1), "prompt": text(1_500, min: 1),
            ], .write)
            try tool("story_save_scene_profile", "根据已读剧情生成并保存场景文字画像，不生成图片；不同镜头复用固定空间设定。", [
                "id": text(128, min: 1), "name": text(120, min: 1), "profile": object([
                    "roleInStory": text(400, min: 1), "setting": text(400, min: 1), "spatialLayout": text(400, min: 1),
                    "lightingAndPalette": text(400, min: 1), "keyElements": text(400, min: 1),
                    "atmosphere": text(400, min: 1), "consistencyNotes": text(400, min: 1),
                ]),
            ], .write)
            try tool("story_save_character_profile", "根据已读剧情生成并保存主角/人物文字画像，不生成图片。", [
                "id": text(128, min: 1), "name": text(120, min: 1), "profile": object([
                    "isProtagonist": ["type": "boolean"], "roleInStory": text(400, min: 1),
                    "appearance": text(400, min: 1), "personality": text(400, min: 1), "motivation": text(400, min: 1),
                    "relationships": text(400, min: 1), "costume": text(400, min: 1), "consistencyNotes": text(400, min: 1),
                ]),
            ], .write)
            try tool("story_append_segments", "追加最多5个明确类型和时长的正式分段。大多数 story 边界直接衔接；仅有明显时空或叙事跳变时，才在同批 story 之间插入一个2–3秒 transition，严禁每段都加，也不能留给用户手动补。transition 不消耗原文。", [
                "segments": ["type": "array", "minItems": 1, "maxItems": 5, "items": object([
                    "id": text(128, min: 1), "title": text(120, min: 1), "synopsis": text(250, min: 1),
                    "kind": ["type": "string", "enum": ["story", "transition"]], "seconds": integer(2, 15),
                    "sourceStart": integer(0, 80_000), "sourceEnd": integer(0, 80_000),
                ])],
            ], .write)
        } else {
            try tool("story_update_segment", "按 story_read_segment 返回的 kind 和 seconds 保存完整镜头语言及首尾帧提示词；转场只连接前后画面状态，不推进剧情。", [
                "segmentID": text(128, min: 1), "detail": object([
                    "firstFramePrompt": text(500, min: 1),
                    "lastFramePrompt": text(500, min: 1),
                    "shots": ["type": "array", "minItems": 1, "maxItems": 8, "items": object([
                        "start": integer(0, 14), "end": integer(1, 15), "prompt": text(200, min: 1),
                    ])],
                    "continuityIn": text(200), "continuityOut": text(200), "audio": text(200), "constraints": text(300),
                ]),
            ], .write)
        }
        try tool("story_finish", "当 story_read_state 返回 readyToFinish=true 时，单独调用此工具校验并结束；不生成媒体。", [:], .terminal)
        return definitions
    }

    static func execute(_ call: AgentToolCall, run: inout StoryAgentRun) throws -> AgentToolOutcome {
        let data = Data(call.arguments.utf8)
        switch call.name {
        case "story_read_state":
            struct Args: Decodable { let offset: Int }
            let offset = try JSONDecoder().decode(Args.self, from: data).offset
            let project = run.draft
            var state: [String: Any] = [:]
            state["stage"] = run.stage.rawValue
            state["title"] = project.title
            state["description"] = String(project.description.prefix(500))
            state["style"] = String(project.style.prefix(500))
            state["ratio"] = project.ratio
            state["summary"] = String(project.summary.prefix(500))
            state["descriptionLength"] = project.description.count
            state["styleLength"] = project.style.count
            state["summaryLength"] = project.summary.count
            state["sourceLength"] = project.source.count
            state["readThrough"] = run.readThrough
            state["coveredThrough"] = project.segments.last?.sourceRange.end ?? 0
            state["characterCount"] = project.characters.count
            state["sceneCount"] = project.scenes.count
            state["propCount"] = project.props.count
            state["segmentCount"] = project.segments.count
            state["targetCount"] = run.targetIDs.count
            state["targetIDs"] = Array(run.targetIDs.dropFirst(offset).prefix(5))
            let remainingTargetIDs = run.targetIDs.filter { id in
                guard let segment = project.segments.first(where: { $0.id == id }), segment.detail != nil else { return true }
                return (try? validateRelations(project, segment: segment)) == nil
            }
            let readyToFinish = (try? validateCompletion(run)) != nil
            state["readyToFinish"] = readyToFinish
            state["remainingTargetCount"] = remainingTargetIDs.count
            state["remainingTargetIDs"] = Array(remainingTargetIDs.prefix(20))
            state["nextAction"] = readyToFinish
                ? "单独调用 story_finish；不要再读取状态、原文或图谱。"
                : "仅补全尚未满足的阶段要求，继续最新进度，不要从头重复读取。"
            state["resources"] = project.resources.dropFirst(offset).prefix(5).map {
                ["id": $0.id, "name": $0.name, "kind": $0.kind.rawValue]
            }
            state["segments"] = project.segments.dropFirst(offset).prefix(5).map {
                ["id": $0.id, "title": $0.title, "kind": $0.kind.rawValue,
                 "seconds": $0.seconds, "detailSaved": $0.detail != nil] as [String: Any]
            }
            state["nextOffset"] = offset + 5
            return try output(state, progress: true)
        case "story_read_graph":
            struct Args: Decodable { let nodeOffset: Int; let edgeOffset: Int; let limit: Int }
            let args = try JSONDecoder().decode(Args.self, from: data)
            let page = try run.draft.graphPage(nodeOffset: args.nodeOffset, edgeOffset: args.edgeOffset, limit: args.limit)
            return .init(String(decoding: try JSONEncoder().encode(page), as: UTF8.self), madeProgress: true)
        case "story_read_text":
            struct Args: Decodable { let field: String; let offset: Int; let limit: Int }
            let args = try JSONDecoder().decode(Args.self, from: data)
            let value: String
            switch args.field {
            case "description": value = run.draft.description
            case "style": value = run.draft.style
            case "summary": value = run.draft.summary
            default: throw StoryAgentError.forbiddenTarget
            }
            guard (0...value.count).contains(args.offset), (1...1_200).contains(args.limit) else { throw StoryAgentError.forbiddenTarget }
            let part = String(value.dropFirst(args.offset).prefix(args.limit))
            return try output(["field": args.field, "offset": args.offset, "nextOffset": args.offset + part.count, "length": value.count, "text": part], progress: true)
        case "story_read_source":
            struct Args: Decodable { let offset: Int; let limit: Int }
            let args = try JSONDecoder().decode(Args.self, from: data)
            guard args.offset >= 0, args.offset < run.draft.source.count, (1...1_200).contains(args.limit),
                  run.stage == .refine || args.offset <= run.readThrough else { throw StoryAgentError.incompletePlan }
            let part = String(run.draft.source.dropFirst(args.offset).prefix(args.limit))
            let next = args.offset + part.count
            let progressed = next > run.readThrough
            if args.offset <= run.readThrough { run.readThrough = max(run.readThrough, next) }
            return try output(["offset": args.offset, "nextOffset": next, "sourceLength": run.draft.source.count, "text": part], progress: progressed)
        case "story_read_asset":
            struct Args: Decodable { let assetID: String }
            let id = try JSONDecoder().decode(Args.self, from: data).assetID
            guard let asset = run.draft.resources.first(where: { $0.id == id }) else { throw StoryAgentError.forbiddenTarget }
            var info: [String: Any] = ["id": asset.id, "name": asset.name, "kind": asset.kind.rawValue]
            if let profile = asset.characterProfile { info["characterProfile"] = try JSONSerialization.jsonObject(with: JSONEncoder().encode(profile)) }
            else if let profile = asset.sceneProfile { info["sceneProfile"] = try JSONSerialization.jsonObject(with: JSONEncoder().encode(profile)) }
            if asset.prompt != asset.characterProfile?.imagePrompt && asset.prompt != asset.sceneProfile?.imagePrompt {
                info["prompt"] = String(asset.prompt.prefix(500)); info["promptLength"] = asset.prompt.count
            }
            return try output(info, progress: true)
        case "story_read_asset_prompt":
            struct Args: Decodable { let assetID: String; let offset: Int; let limit: Int }
            let args = try JSONDecoder().decode(Args.self, from: data)
            guard let asset = run.draft.resources.first(where: { $0.id == args.assetID }),
                  (0...asset.prompt.count).contains(args.offset), (1...1_200).contains(args.limit) else { throw StoryAgentError.forbiddenTarget }
            let part = String(asset.prompt.dropFirst(args.offset).prefix(args.limit))
            return try output(["assetID": asset.id, "offset": args.offset, "nextOffset": args.offset + part.count, "length": asset.prompt.count, "text": part], progress: true)
        case "story_read_segment":
            struct Args: Decodable { let segmentID: String }
            let id = try JSONDecoder().decode(Args.self, from: data).segmentID
            guard let index = run.draft.segments.firstIndex(where: { $0.id == id }) else { throw StoryAgentError.forbiddenTarget }
            let segment = run.draft.segments[index]
            var info: [String: Any] = ["id": id, "title": segment.title, "synopsis": segment.synopsis,
                "kind": segment.kind.rawValue, "seconds": segment.seconds,
                "characterIDs": segment.characterIDs, "sceneIDs": segment.sceneIDs, "propIDs": segment.propIDs, "detailSaved": segment.detail != nil,
                "sourceExcerpt": try StoryContinuityContext.sourceExcerpt(run.draft, segmentID: id),
                "adjacentContinuity": try StoryContinuityContext.context(run.draft, segmentID: id)]
            info["sourceStart"] = segment.sourceRange.start; info["sourceEnd"] = segment.sourceRange.end
            info["relations"] = try JSONSerialization.jsonObject(with: JSONEncoder().encode(run.draft.relations(for: id)))
            return try output(info, progress: true)
        case "story_save_segment_relations":
            struct Args: Decodable {
                struct Relation: Decodable { let relationID: String; let characterID: String; let sceneID: String; let action: String; let position: String; let startSecond: Int; let endSecond: Int }
                let segmentID: String; let characterIDs: [String]; let sceneIDs: [String]; let propIDs: [String]; let relations: [Relation]
            }
            let args = try JSONDecoder().decode(Args.self, from: data)
            guard run.stage == .outline || run.targetIDs.contains(args.segmentID),
                  let index = run.draft.segments.firstIndex(where: { $0.id == args.segmentID }),
                  run.draft.segments[index].attempt == nil, run.draft.segments[index].video == nil else { throw StoryAgentError.forbiddenTarget }
            let relations = args.relations.map { StorySegmentRelation(id: $0.relationID, segmentID: args.segmentID, characterID: $0.characterID, sceneID: $0.sceneID, action: $0.action, position: $0.position, startSecond: $0.startSecond, endSecond: $0.endSecond) }
            if run.draft.segments[index].characterIDs == args.characterIDs,
               run.draft.segments[index].sceneIDs == args.sceneIDs,
               run.draft.segments[index].propIDs == args.propIDs,
               run.draft.relations(for: args.segmentID) == relations { return .init("关联关系已保存", madeProgress: false) }
            guard run.draft.segments[index].detail == nil else { throw StoryAgentError.forbiddenTarget }
            run.draft.segments[index].characterIDs = args.characterIDs
            run.draft.segments[index].sceneIDs = args.sceneIDs
            run.draft.segments[index].propIDs = args.propIDs
            run.draft.relations.removeAll { $0.segmentID == args.segmentID }
            run.draft.relations.append(contentsOf: relations)
            try run.draft.validate()
            try validateRelations(run.draft, segment: run.draft.segments[index])
            return .init("本段人物、场景与动作关系已保存")
        case "story_save_summary":
            guard run.stage == .outline else { throw StoryAgentError.wrongStage }
            struct Args: Decodable { let summary: String }
            let summary = try JSONDecoder().decode(Args.self, from: data).summary
            guard !summary.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, summary.count <= 4_000 else { throw StoryError.invalidPlan }
            let changed = run.draft.summary != summary
            run.draft.summary = summary
            return .init("全剧摘要已保存", madeProgress: changed)
        case "story_upsert_asset":
            guard run.stage == .outline else { throw StoryAgentError.wrongStage }
            struct Args: Decodable { let id: String; let kind: StoryResource.Kind; let name: String; let prompt: String }
            let asset = try JSONDecoder().decode(Args.self, from: data)
            guard asset.kind == .prop else { throw StoryAgentError.wrongStage }
            let value = StoryProp(id: asset.id, name: asset.name, description: asset.prompt)
            if let index = run.draft.props.firstIndex(where: { $0.id == asset.id }) {
                if run.draft.props[index] == value {
                    return .init("素材定义已存在，无需重复创建", madeProgress: false)
                }
                guard run.draft.props[index].media.images.isEmpty else { throw StoryAgentError.forbiddenTarget }
                run.draft.props[index] = value
            } else {
                try guardNoResourceCollision(run.draft, id: asset.id, kind: .prop)
                run.draft.props.append(value)
            }
            try run.draft.validate()
            return .init("素材已保存：\(asset.id)")
        case "story_save_character_profile":
            guard run.stage == .outline, run.readThrough > 0 else { throw StoryAgentError.wrongStage }
            struct Args: Decodable { let id: String; let name: String; let profile: StoryCharacterProfile }
            let args = try JSONDecoder().decode(Args.self, from: data)
            try args.profile.validate()
            let character = StoryCharacter(id: args.id, name: args.name, profile: args.profile)
            if let index = run.draft.characters.firstIndex(where: { $0.id == args.id }) {
                if run.draft.characters[index] == character { return .init("人物文字画像已保存", madeProgress: false) }
                guard run.draft.characters[index].media.images.isEmpty else { throw StoryAgentError.forbiddenTarget }
                run.draft.characters[index] = character
            } else {
                try guardNoResourceCollision(run.draft, id: args.id, kind: .character)
                run.draft.characters.append(character)
            }
            try run.draft.validate()
            return .init("人物文字画像已保存：\(args.name)。没有生成图片。")
        case "story_save_scene_profile":
            guard run.stage == .outline, run.readThrough > 0 else { throw StoryAgentError.wrongStage }
            struct Args: Decodable { let id: String; let name: String; let profile: StorySceneProfile }
            let args = try JSONDecoder().decode(Args.self, from: data)
            try args.profile.validate()
            let scene = StoryScene(id: args.id, name: args.name, profile: args.profile)
            if let index = run.draft.scenes.firstIndex(where: { $0.id == args.id }) {
                if run.draft.scenes[index] == scene { return .init("场景文字画像已保存", madeProgress: false) }
                guard run.draft.scenes[index].media.images.isEmpty else { throw StoryAgentError.forbiddenTarget }
                run.draft.scenes[index] = scene
            } else {
                try guardNoResourceCollision(run.draft, id: args.id, kind: .scene)
                run.draft.scenes.append(scene)
            }
            try run.draft.validate()
            return .init("场景文字画像已保存：\(args.name)。没有生成图片。")
        case "story_append_segments":
            guard run.stage == .outline else { throw StoryAgentError.wrongStage }
            struct Args: Decodable {
                struct Segment: Decodable {
                    let id: String; let title: String; let synopsis: String
                    let kind: StorySegmentKind; let seconds: Int
                    let sourceStart: Int; let sourceEnd: Int
                }
                let segments: [Segment]
            }
            let args = try JSONDecoder().decode(Args.self, from: data)
            guard (1...5).contains(args.segments.count) else { throw StoryError.invalidPlan }
            var cursor = run.draft.segments.last?.sourceRange.end ?? 0
            for value in args.segments {
                let kind = value.kind
                let seconds = value.seconds
                let previousKind = run.draft.segments.last?.kind
                let validSourceRange = kind == .story
                    ? value.sourceStart == cursor && value.sourceEnd > cursor && value.sourceEnd <= run.readThrough
                    : value.sourceStart == cursor && value.sourceEnd == cursor
                guard validSourceRange, (2...15).contains(seconds),
                      kind != .transition || seconds <= 3,
                      kind != .transition || (previousKind != nil && previousKind != .transition),
                      !run.draft.segments.contains(where: { $0.id == value.id }), !value.synopsis.isEmpty else {
                    throw StoryAgentError.incompletePlan
                }
                let segment = StorySegment(id: value.id, title: value.title, synopsis: value.synopsis,
                                           sourceRange: .init(start: value.sourceStart, end: value.sourceEnd),
                                           kind: kind, seconds: seconds)
                run.draft.segments.append(segment)
                if kind == .story { cursor = value.sourceEnd }
            }
            try run.draft.validate()
            return .init("已追加 \(args.segments.count) 个分段（含明确类型与时长）；已覆盖原文至 \(cursor) / \(run.draft.source.count)")
        case "story_update_segment":
            guard run.stage == .refine else { throw StoryAgentError.wrongStage }
            struct Args: Decodable { let segmentID: String; let detail: StorySegmentDetail }
            let args = try JSONDecoder().decode(Args.self, from: data)
            guard run.targetIDs.contains(args.segmentID), let index = run.draft.segments.firstIndex(where: { $0.id == args.segmentID }),
                  run.draft.segments[index].attempt == nil, run.draft.segments[index].video == nil else { throw StoryAgentError.forbiddenTarget }
            guard hasReadSegment(args.segmentID, run: run) else { throw StoryAgentError.incompletePlan }
            try args.detail.validate(duration: run.draft.segments[index].seconds)
            try validateRelations(run.draft, segment: run.draft.segments[index])
            if let saved = run.draft.segments[index].detail {
                guard saved == args.detail else { throw StoryAgentError.forbiddenTarget }
                return .init("本段提示词已保存", madeProgress: false)
            }
            run.draft.segments[index].detail = args.detail
            run.draft.segments[index].firstFrames.confirmedImageID = nil
            run.draft.segments[index].lastFrames.confirmedImageID = nil
            return .init("\(run.draft.segments[index].seconds)秒\(run.draft.segments[index].kind == .transition ? "转场" : "剧情")镜头计划已保存：\(args.segmentID)")
        case "story_finish":
            try validateCompletion(run)
            return .init("本阶段规划完成，已通过客户端校验。")
        default: throw StoryAgentError.wrongStage
        }
    }

    static func validateCompletion(_ run: StoryAgentRun) throws {
        try run.draft.validate()
        for segment in run.draft.segments where run.stage == .outline || run.targetIDs.contains(segment.id) {
            try validateRelations(run.draft, segment: segment)
        }
        if run.stage == .outline {
            guard run.readThrough == run.draft.source.count, !run.draft.summary.isEmpty, !run.draft.segments.isEmpty else { throw StoryAgentError.incompletePlan }
            guard run.draft.characters.allSatisfy({ !$0.profile.roleInStory.isEmpty }) else { throw StoryAgentError.incompletePlan }
            guard run.draft.scenes.allSatisfy({ !$0.profile.roleInStory.isEmpty }) else { throw StoryAgentError.incompletePlan }
            struct RelationArgs: Decodable { let segmentID: String }
            let explicitlyRelated = Set(run.toolReceipts.values.compactMap { receipt -> String? in
                guard receipt.name == "story_save_segment_relations", !receipt.outcome.isError,
                      let data = receipt.arguments.data(using: .utf8),
                      let args = try? JSONDecoder().decode(RelationArgs.self, from: data) else { return nil }
                return args.segmentID
            })
            guard run.draft.segments.allSatisfy({ explicitlyRelated.contains($0.id) }) else {
                throw StoryAgentError.incompletePlan
            }
            var cursor = 0
            for (index, segment) in run.draft.segments.enumerated() {
                let range = segment.sourceRange
                if segment.kind == .transition {
                    guard index > 0, index + 1 < run.draft.segments.count,
                          run.draft.segments[index - 1].kind == .story,
                          run.draft.segments[index + 1].kind == .story,
                          range.start == cursor, range.end == cursor,
                          (2...3).contains(segment.seconds) else { throw StoryAgentError.incompletePlan }
                } else {
                    guard range.start == cursor, range.end > cursor else { throw StoryAgentError.incompletePlan }
                    cursor = range.end
                }
            }
            guard cursor == run.draft.source.count else { throw StoryAgentError.incompletePlan }
        } else {
            guard run.targetIDs.allSatisfy({ id in run.draft.segments.contains { $0.id == id && $0.detail != nil } }) else { throw StoryAgentError.incompletePlan }
        }
    }
    private static func validateRelations(_ project: StoryProject, segment: StorySegment) throws {
        guard !segment.sceneIDs.isEmpty else { throw StoryAgentError.incompletePlan }
        let relations = project.relations(for: segment.id)
        guard segment.characterIDs.allSatisfy({ characterID in relations.contains { $0.characterID == characterID } }) else {
            throw StoryAgentError.incompletePlan
        }
    }
    private static func guardNoResourceCollision(_ project: StoryProject, id: String, kind: StoryResource.Kind) throws {
        if let existing = project.resource(id: id), existing.kind != kind { throw StoryAgentError.forbiddenTarget }
    }
    private static func hasReadSegment(_ segmentID: String, run: StoryAgentRun) -> Bool {
        struct Args: Decodable { let segmentID: String }
        return run.toolReceipts.values.contains { receipt in
            guard receipt.name == "story_read_segment", !receipt.outcome.isError,
                  let data = receipt.arguments.data(using: .utf8),
                  let args = try? JSONDecoder().decode(Args.self, from: data) else { return false }
            return args.segmentID == segmentID
        }
    }
    private static func output(_ value: [String: Any], progress: Bool) throws -> AgentToolOutcome {
        .init(String(decoding: try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]), as: UTF8.self), madeProgress: progress)
    }
    private static func text(_ max: Int, min: Int = 0) -> [String: Any] { ["type": "string", "minLength": min, "maxLength": max] }
    private static func integer(_ min: Int, _ max: Int) -> [String: Any] { ["type": "integer", "minimum": min, "maximum": max] }
    private static func object(_ fields: [String: Any]) -> [String: Any] {
        ["type": "object", "properties": fields, "required": fields.keys.sorted(), "additionalProperties": false]
    }
}
