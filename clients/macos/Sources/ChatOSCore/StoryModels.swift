import Foundation

public struct StoryModelSelection: Codable, Equatable, Sendable {
    public var textModelID: String
    public var imageModelID: String
    public var videoModelID: String
    public init(textModelID: String, imageModelID: String, videoModelID: String) {
        self.textModelID = textModelID; self.imageModelID = imageModelID; self.videoModelID = videoModelID
    }
}

public struct StoryImageCollection: Codable, Equatable, Sendable {
    public var images: [StoryImage] = []
    public var confirmedImageID: UUID?
    public var generationAttemptID: UUID?
    public init() {}
    public var confirmedImage: StoryImage? { images.first { $0.id == confirmedImageID } }
    public func validate() throws {
        guard Set(images.map(\.id)).count == images.count,
              confirmedImageID == nil || images.contains(where: { $0.id == confirmedImageID }) else { throw StoryError.invalidPlan }
    }
}

public enum StoryFrameRole: String, Codable, CaseIterable, Sendable {
    case first
    case last
}

public struct StoryCharacter: Codable, Equatable, Identifiable, Sendable {
    public var id: String
    public var name: String
    public var profile: StoryCharacterProfile
    public var imagePrompt: String
    public var media = StoryImageCollection()
    public init(id: String, name: String, profile: StoryCharacterProfile, imagePrompt: String? = nil) {
        self.id = id; self.name = name; self.profile = profile; self.imagePrompt = imagePrompt ?? profile.imagePrompt
    }
}

public struct StoryScene: Codable, Equatable, Identifiable, Sendable {
    public var id: String
    public var name: String
    public var profile: StorySceneProfile
    public var imagePrompt: String
    public var media = StoryImageCollection()
    public init(id: String, name: String, profile: StorySceneProfile, imagePrompt: String? = nil) {
        self.id = id; self.name = name; self.profile = profile; self.imagePrompt = imagePrompt ?? profile.imagePrompt
    }
}

public struct StoryProp: Codable, Equatable, Identifiable, Sendable {
    public var id: String
    public var name: String
    public var description: String
    public var imagePrompt: String
    public var media = StoryImageCollection()
    public init(id: String, name: String, description: String, imagePrompt: String? = nil) {
        self.id = id; self.name = name; self.description = description; self.imagePrompt = imagePrompt ?? description
    }
}

/// A presentation adapter over normalized character/scene/prop tables. It is never persisted.
public struct StoryResource: Equatable, Identifiable, Sendable {
    public enum Kind: String, Codable, Sendable, CaseIterable { case character, scene, prop }
    public var id: String
    public var kind: Kind
    public var name: String
    public var prompt: String
    public var media: StoryImageCollection
    public var characterProfile: StoryCharacterProfile?
    public var sceneProfile: StorySceneProfile?
    public var images: [StoryImage] { media.images }
    public var confirmedImageID: UUID? { media.confirmedImageID }
    public var confirmedImage: StoryImage? { media.confirmedImage }
    public var imageGenerationAttemptID: UUID? { media.generationAttemptID }
}

public struct StoryProject: Codable, Equatable, Identifiable, Sendable {
    public var id = UUID()
    /// This is the first and only story-studio schema. There is no v1 migration.
    public var version = 2
    public var title: String
    public var description: String
    public var models: StoryModelSelection
    public var source = ""
    public var style = "自然光，电影感，保持角色外观、服装与场景一致"
    public var ratio = "16:9"
    public var summary = ""
    public var characters: [StoryCharacter] = []
    public var scenes: [StoryScene] = []
    public var props: [StoryProp] = []
    public var segments: [StorySegment] = []
    public var relations: [StorySegmentRelation] = []
    public var createdAt = Date()
    public var updatedAt = Date()

    private enum CodingKeys: String, CodingKey {
        case id, version, title, description, models, source, style, ratio, summary
        case characters, scenes, props, segments, relations, createdAt, updatedAt
    }

    public init(title: String, description: String, models: StoryModelSelection) {
        self.title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        self.description = description.trimmingCharacters(in: .whitespacesAndNewlines)
        self.models = models
    }

    public init(from decoder: any Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        version = try values.decode(Int.self, forKey: .version)
        guard version == 2 else {
            throw DecodingError.dataCorruptedError(forKey: .version, in: values,
                debugDescription: "Unsupported story project schema. Only the new StoryProject schema is accepted.")
        }
        id = try values.decode(UUID.self, forKey: .id)
        title = try values.decode(String.self, forKey: .title)
        description = try values.decode(String.self, forKey: .description)
        models = try values.decode(StoryModelSelection.self, forKey: .models)
        source = try values.decode(String.self, forKey: .source)
        style = try values.decode(String.self, forKey: .style)
        ratio = try values.decode(String.self, forKey: .ratio)
        summary = try values.decode(String.self, forKey: .summary)
        characters = try values.decode([StoryCharacter].self, forKey: .characters)
        scenes = try values.decode([StoryScene].self, forKey: .scenes)
        props = try values.decode([StoryProp].self, forKey: .props)
        segments = try values.decode([StorySegment].self, forKey: .segments)
        relations = try values.decode([StorySegmentRelation].self, forKey: .relations)
        createdAt = try values.decode(Date.self, forKey: .createdAt)
        updatedAt = try values.decode(Date.self, forKey: .updatedAt)
    }

    public var totalSeconds: Int { segments.reduce(0) { $0 + $1.seconds } }
    public var completedCount: Int { segments.filter { $0.video != nil }.count }
    public var hasUnresolvedVideoJobs: Bool { segments.contains { $0.attempt != nil && $0.video == nil } }
    public var hasUnresolvedImageJobs: Bool {
        resources.contains { $0.media.generationAttemptID != nil }
        || segments.contains {
            $0.firstFrames.generationAttemptID != nil || $0.lastFrames.generationAttemptID != nil
        }
    }
    public var hasUnresolvedJobs: Bool { hasUnresolvedVideoJobs || hasUnresolvedImageJobs }
    public var resources: [StoryResource] {
        characters.map { .init(id: $0.id, kind: .character, name: $0.name, prompt: $0.imagePrompt, media: $0.media, characterProfile: $0.profile) }
        + scenes.map { .init(id: $0.id, kind: .scene, name: $0.name, prompt: $0.imagePrompt, media: $0.media, sceneProfile: $0.profile) }
        + props.map { .init(id: $0.id, kind: .prop, name: $0.name, prompt: $0.imagePrompt, media: $0.media) }
    }
    public func resource(id: String) -> StoryResource? { resources.first { $0.id == id } }
    public mutating func replaceResource(_ resource: StoryResource) throws {
        switch resource.kind {
        case .character:
            guard let profile = resource.characterProfile, let index = characters.firstIndex(where: { $0.id == resource.id }) else { throw StoryError.invalidPlan }
            characters[index] = .init(id: resource.id, name: resource.name, profile: profile, imagePrompt: resource.prompt)
            characters[index].media = resource.media
        case .scene:
            guard let profile = resource.sceneProfile, let index = scenes.firstIndex(where: { $0.id == resource.id }) else { throw StoryError.invalidPlan }
            scenes[index] = .init(id: resource.id, name: resource.name, profile: profile, imagePrompt: resource.prompt)
            scenes[index].media = resource.media
        case .prop:
            guard let index = props.firstIndex(where: { $0.id == resource.id }) else { throw StoryError.invalidPlan }
            props[index] = .init(id: resource.id, name: resource.name, description: props[index].description, imagePrompt: resource.prompt)
            props[index].media = resource.media
        }
    }
    public func relations(for segmentID: String) -> [StorySegmentRelation] { relations.filter { $0.segmentID == segmentID } }

    public func validate() throws {
        guard version == 2, !title.isEmpty, title.count <= 120, description.count <= 4_000,
              source.count <= 80_000, style.count <= 2_000, summary.count <= 16_000,
              !models.textModelID.isEmpty, !models.imageModelID.isEmpty, !models.videoModelID.isEmpty,
              segments.count <= 200, characters.count <= 100, scenes.count <= 100, props.count <= 100, relations.count <= 1_600,
              Set(segments.map(\.id)).count == segments.count else { throw StoryError.invalidProject }
        let allIDs = characters.map(\.id) + scenes.map(\.id) + props.map(\.id)
        guard Set(allIDs).count == allIDs.count else { throw StoryError.invalidPlan }
        for character in characters {
            try validateRecord(id: character.id, name: character.name, prompt: character.imagePrompt, media: character.media)
            try character.profile.validate()
        }
        for scene in scenes {
            try validateRecord(id: scene.id, name: scene.name, prompt: scene.imagePrompt, media: scene.media)
            try scene.profile.validate()
        }
        for prop in props {
            try validateRecord(id: prop.id, name: prop.name, prompt: prop.imagePrompt, media: prop.media)
            guard !prop.description.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, prop.description.count <= 2_000 else { throw StoryError.invalidPlan }
        }
        let characterIDs = Set(characters.map(\.id)), sceneIDs = Set(scenes.map(\.id)), propIDs = Set(props.map(\.id))
        for (segmentIndex, segment) in segments.enumerated() {
            guard (2...15).contains(segment.seconds), !segment.id.isEmpty, segment.id.count <= 128,
                  !segment.title.isEmpty, segment.title.count <= 120, segment.synopsis.count <= 2_000,
                  segment.characterIDs.count + segment.sceneIDs.count + segment.propIDs.count <= 8,
                  Set(segment.characterIDs).count == segment.characterIDs.count,
                  Set(segment.sceneIDs).count == segment.sceneIDs.count,
                  Set(segment.propIDs).count == segment.propIDs.count,
                  segment.characterIDs.allSatisfy(characterIDs.contains),
                  segment.sceneIDs.allSatisfy(sceneIDs.contains),
                  segment.propIDs.allSatisfy(propIDs.contains) else { throw StoryError.invalidPlan }
            try segment.firstFrames.validate()
            try segment.lastFrames.validate()
            guard segment.sourceRange.start >= 0, segment.sourceRange.end >= segment.sourceRange.start,
                  segment.sourceRange.end <= source.count else {
                throw StoryError.invalidPlan
            }
            if segment.kind == .transition {
                guard segmentIndex > 0, segmentIndex + 1 < segments.count,
                      segments[segmentIndex - 1].kind == .story,
                      segments[segmentIndex + 1].kind == .story,
                      segment.sourceRange.start == segment.sourceRange.end,
                      segment.seconds <= 3 else { throw StoryError.invalidPlan }
            } else {
                guard segment.sourceRange.end > segment.sourceRange.start else { throw StoryError.invalidPlan }
            }
            if let detail = segment.detail { try detail.validate(duration: segment.seconds) }
        }
        guard Set(relations.map(\.id)).count == relations.count else { throw StoryError.invalidPlan }
        for relation in relations {
            guard !relation.id.isEmpty, relation.id.count <= 128,
                  segments.contains(where: { $0.id == relation.segmentID }),
                  characterIDs.contains(relation.characterID), sceneIDs.contains(relation.sceneID),
                  segments.first(where: { $0.id == relation.segmentID })?.characterIDs.contains(relation.characterID) == true,
                  segments.first(where: { $0.id == relation.segmentID })?.sceneIDs.contains(relation.sceneID) == true,
                  !relation.action.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, relation.action.count <= 400,
                  !relation.position.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, relation.position.count <= 400,
                  relation.startSecond >= 0,
                  relation.endSecond <= (segments.first(where: { $0.id == relation.segmentID })?.seconds ?? 0),
                  relation.startSecond < relation.endSecond else { throw StoryError.invalidPlan }
        }
        for (index, relation) in relations.enumerated() {
            guard !relations.dropFirst(index + 1).contains(where: { other in
                other.segmentID == relation.segmentID && other.characterID == relation.characterID && other.sceneID != relation.sceneID
                && max(other.startSecond, relation.startSecond) < min(other.endSecond, relation.endSecond)
            }) else { throw StoryError.invalidPlan }
        }
    }
    private func validateRecord(id: String, name: String, prompt: String, media: StoryImageCollection) throws {
        guard !id.isEmpty, id.count <= 128, !name.isEmpty, name.count <= 120,
              !prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, prompt.count <= 4_000 else { throw StoryError.invalidPlan }
        try media.validate()
    }
}

/// A written character portrait, not a generated image or a provider task.
public struct StoryCharacterProfile: Codable, Equatable, Sendable {
    public var isProtagonist: Bool
    public var roleInStory: String
    public var appearance: String
    public var personality: String
    public var motivation: String
    public var relationships: String
    public var costume: String
    public var consistencyNotes: String
    public init(isProtagonist: Bool, roleInStory: String, appearance: String, personality: String, motivation: String,
                relationships: String, costume: String, consistencyNotes: String) {
        self.isProtagonist = isProtagonist; self.roleInStory = roleInStory; self.appearance = appearance
        self.personality = personality; self.motivation = motivation; self.relationships = relationships
        self.costume = costume; self.consistencyNotes = consistencyNotes
    }
    public func validate() throws {
        let fields = [roleInStory, appearance, personality, motivation, relationships, costume, consistencyNotes]
        guard fields.allSatisfy({ !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && $0.count <= 600 }) else { throw StoryError.invalidPlan }
    }
    public var imagePrompt: String { [appearance, costume, consistencyNotes].joined(separator: "\n") }
}

public struct StorySceneProfile: Codable, Equatable, Sendable {
    public var roleInStory: String
    public var setting: String
    public var spatialLayout: String
    public var lightingAndPalette: String
    public var keyElements: String
    public var atmosphere: String
    public var consistencyNotes: String
    public init(roleInStory: String, setting: String, spatialLayout: String, lightingAndPalette: String,
                keyElements: String, atmosphere: String, consistencyNotes: String) {
        self.roleInStory = roleInStory; self.setting = setting; self.spatialLayout = spatialLayout
        self.lightingAndPalette = lightingAndPalette; self.keyElements = keyElements
        self.atmosphere = atmosphere; self.consistencyNotes = consistencyNotes
    }
    public func validate() throws {
        let fields = [roleInStory, setting, spatialLayout, lightingAndPalette, keyElements, atmosphere, consistencyNotes]
        guard fields.allSatisfy({ !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && $0.count <= 600 }) else { throw StoryError.invalidPlan }
    }
    public var imagePrompt: String { [setting, spatialLayout, lightingAndPalette, keyElements, atmosphere, consistencyNotes].joined(separator: "\n") }
}

public struct StoryImage: Codable, Equatable, Identifiable, Sendable {
    public var id = UUID()
    public var filename: String
    public var mimeType: String
    /// IDs are deliberately retained independently: the local file ID is not a
    /// replacement for the caller attempt or the provider's response IDs.
    public var sourceResourceID: String?
    public var generationAttemptID: UUID?
    public var providerResultID: String?
    public var providerAssetID: String?
    public init(filename: String, mimeType: String, sourceResourceID: String? = nil,
                generationAttemptID: UUID? = nil, providerResultID: String? = nil,
                providerAssetID: String? = nil) {
        self.filename = filename; self.mimeType = mimeType; self.sourceResourceID = sourceResourceID
        self.generationAttemptID = generationAttemptID; self.providerResultID = providerResultID
        self.providerAssetID = providerAssetID
    }
}

public enum StorySegmentKind: String, Codable, CaseIterable, Sendable {
    case story
    case transition
}

public struct StorySegment: Codable, Equatable, Identifiable, Sendable {
    public var id: String
    public var title: String
    public var synopsis: String
    public var kind: StorySegmentKind = .story
    public var seconds = 15
    public var sourceRange: StorySourceRange
    public var characterIDs: [String]
    public var sceneIDs: [String]
    public var propIDs: [String]
    public var detail: StorySegmentDetail?
    public var firstFrames = StoryImageCollection()
    /// Set only when the confirmed first frame was deterministically inherited from
    /// the immediately preceding segment. User-selected/uploaded/generated frames keep this nil.
    public var inheritedFirstFrameSourceSegmentID: String?
    public var lastFrames = StoryImageCollection()
    public var useLastFrameForVideo = true
    public var attempt: StoryVideoAttempt?
    public var previousAttempts: [StoryVideoAttempt] = []
    public var video: StoryVideo?
    public var error: String?
    public init(id: String, title: String, synopsis: String, sourceRange: StorySourceRange,
                kind: StorySegmentKind = .story, seconds: Int = 15,
                characterIDs: [String] = [], sceneIDs: [String] = [], propIDs: [String] = []) {
        self.id = id; self.title = title; self.synopsis = synopsis; self.sourceRange = sourceRange
        self.kind = kind; self.seconds = seconds
        self.characterIDs = characterIDs; self.sceneIDs = sceneIDs; self.propIDs = propIDs
    }
    private enum CodingKeys: String, CodingKey {
        case id, title, synopsis, kind, seconds, sourceRange, characterIDs, sceneIDs, propIDs, detail
        case firstFrames, inheritedFirstFrameSourceSegmentID, lastFrames, useLastFrameForVideo
        case attempt, previousAttempts, video, error
    }
    public init(from decoder: any Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        id = try values.decode(String.self, forKey: .id)
        title = try values.decode(String.self, forKey: .title)
        synopsis = try values.decode(String.self, forKey: .synopsis)
        kind = try values.decodeIfPresent(StorySegmentKind.self, forKey: .kind) ?? .story
        seconds = try values.decodeIfPresent(Int.self, forKey: .seconds) ?? 15
        sourceRange = try values.decode(StorySourceRange.self, forKey: .sourceRange)
        characterIDs = try values.decodeIfPresent([String].self, forKey: .characterIDs) ?? []
        sceneIDs = try values.decodeIfPresent([String].self, forKey: .sceneIDs) ?? []
        propIDs = try values.decodeIfPresent([String].self, forKey: .propIDs) ?? []
        detail = try values.decodeIfPresent(StorySegmentDetail.self, forKey: .detail)
        firstFrames = try values.decodeIfPresent(StoryImageCollection.self, forKey: .firstFrames) ?? .init()
        inheritedFirstFrameSourceSegmentID = try values.decodeIfPresent(String.self, forKey: .inheritedFirstFrameSourceSegmentID)
        // version=2 projects written before tail-frame support intentionally decode to an empty collection.
        lastFrames = try values.decodeIfPresent(StoryImageCollection.self, forKey: .lastFrames) ?? .init()
        useLastFrameForVideo = try values.decodeIfPresent(Bool.self, forKey: .useLastFrameForVideo) ?? true
        attempt = try values.decodeIfPresent(StoryVideoAttempt.self, forKey: .attempt)
        previousAttempts = try values.decodeIfPresent([StoryVideoAttempt].self, forKey: .previousAttempts) ?? []
        video = try values.decodeIfPresent(StoryVideo.self, forKey: .video)
        error = try values.decodeIfPresent(String.self, forKey: .error)
    }
    public var resourceIDs: [String] { characterIDs + sceneIDs + propIDs }
    public var firstFrame: StoryImage? { firstFrames.confirmedImage }
    public var lastFrame: StoryImage? { lastFrames.confirmedImage }
    public var confirmedFrameID: UUID? {
        get { firstFrames.confirmedImageID }
        set { firstFrames.confirmedImageID = newValue }
    }
    public var imageGenerationAttemptID: UUID? {
        get { firstFrames.generationAttemptID }
        set { firstFrames.generationAttemptID = newValue }
    }
    public var confirmedLastFrameID: UUID? {
        get { lastFrames.confirmedImageID }
        set { lastFrames.confirmedImageID = newValue }
    }
    public var lastFrameGenerationAttemptID: UUID? {
        get { lastFrames.generationAttemptID }
        set { lastFrames.generationAttemptID = newValue }
    }
    public func frames(for role: StoryFrameRole) -> StoryImageCollection {
        role == .first ? firstFrames : lastFrames
    }
    public var isReady: Bool { detail != nil && firstFrame != nil && attempt == nil && video == nil }
}

public struct StorySourceRange: Codable, Equatable, Sendable {
    public var start: Int
    public var end: Int
    public init(start: Int, end: Int) { self.start = start; self.end = end }
}

public struct StorySegmentRelation: Codable, Equatable, Identifiable, Sendable {
    public var id: String
    public var segmentID: String
    public var characterID: String
    public var sceneID: String
    public var action: String
    public var position: String
    public var startSecond: Int
    public var endSecond: Int
    public init(id: String = UUID().uuidString, segmentID: String, characterID: String, sceneID: String,
                action: String, position: String, startSecond: Int = 0, endSecond: Int = 15) {
        self.id = id; self.segmentID = segmentID; self.characterID = characterID; self.sceneID = sceneID
        self.action = action; self.position = position; self.startSecond = startSecond; self.endSecond = endSecond
    }
}

public struct StorySegmentDetail: Codable, Equatable, Sendable {
    public struct Shot: Codable, Equatable, Sendable {
        public var start: Int
        public var end: Int
        public var prompt: String
        public init(start: Int, end: Int, prompt: String) { self.start = start; self.end = end; self.prompt = prompt }
    }
    public var firstFramePrompt: String
    public var lastFramePrompt: String?
    public var shots: [Shot]
    public var continuityIn: String
    public var continuityOut: String
    public var audio: String
    public var constraints: String
    public init(firstFramePrompt: String, shots: [Shot], continuityIn: String, continuityOut: String, audio: String,
                constraints: String, lastFramePrompt: String? = nil) {
        self.firstFramePrompt = firstFramePrompt; self.shots = shots; self.continuityIn = continuityIn
        self.continuityOut = continuityOut; self.audio = audio; self.constraints = constraints
        self.lastFramePrompt = lastFramePrompt
    }
    public func validate(duration: Int = 15) throws {
        guard (2...15).contains(duration) else { throw StoryError.invalidPlan }
        guard !firstFramePrompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              firstFramePrompt.count <= 3_000, (1...8).contains(shots.count),
              lastFramePrompt == nil || (!lastFramePrompt!.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                                         && lastFramePrompt!.count <= 3_000),
              continuityIn.count <= 1_000, continuityOut.count <= 1_000, audio.count <= 1_000,
              constraints.count <= 2_000 else { throw StoryError.invalidPlan }
        var cursor = 0
        for shot in shots {
            guard shot.start == cursor, shot.end > shot.start, shot.end <= duration,
                  !shot.prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                  shot.prompt.count <= 2_000 else { throw StoryError.invalidPlan }
            cursor = shot.end
        }
        guard cursor == duration else { throw StoryError.invalidPlan }
    }
    public var videoPrompt: String {
        (["入镜：\(continuityIn)"] + shots.map { "\($0.start)–\($0.end)秒：\($0.prompt)" }
         + ["出镜：\(continuityOut)", "声音：\(audio)", "约束：\(constraints)"]).joined(separator: "\n")
    }
    /// Older projects did not persist a tail-frame prompt. Their final shot and exit state
    /// provide a stable local fallback without forcing an AI migration or invalidating data.
    public var effectiveLastFramePrompt: String {
        if let value = lastFramePrompt?.trimmingCharacters(in: .whitespacesAndNewlines), !value.isEmpty { return value }
        return [shots.last?.prompt, continuityOut].compactMap { value in
            guard let value, !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
            return value
        }.joined(separator: "\n")
    }
}

public struct StoryVideoAttempt: Codable, Equatable, Sendable {
    public var id = UUID()
    public var modelConfigID: String
    public var prompt: String
    public var size: String
    public var ratio: String
    public var seconds: Int
    public var jobID: String?
    public var status = "submitting"
    public var createdAt = Date()
    public init(modelConfigID: String, prompt: String, size: String, ratio: String, seconds: Int = 15) {
        self.modelConfigID = modelConfigID; self.prompt = prompt; self.size = size; self.ratio = ratio
        self.seconds = seconds
    }

    private enum CodingKeys: String, CodingKey {
        case id, modelConfigID, prompt, size, ratio, seconds, jobID, status, createdAt
    }

    public init(from decoder: any Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        id = try values.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
        modelConfigID = try values.decode(String.self, forKey: .modelConfigID)
        prompt = try values.decode(String.self, forKey: .prompt)
        size = try values.decode(String.self, forKey: .size)
        ratio = try values.decode(String.self, forKey: .ratio)
        seconds = try values.decodeIfPresent(Int.self, forKey: .seconds) ?? 15
        jobID = try values.decodeIfPresent(String.self, forKey: .jobID)
        status = try values.decodeIfPresent(String.self, forKey: .status) ?? "submitting"
        createdAt = try values.decodeIfPresent(Date.self, forKey: .createdAt) ?? Date()
    }
}

public struct StoryVideo: Codable, Equatable, Sendable {
    public var filename: String
    public var jobID: String
    public var modelName: String
    public init(filename: String, jobID: String, modelName: String) {
        self.filename = filename; self.jobID = jobID; self.modelName = modelName
    }
}

public struct StoryPlanningRequest: Sendable {
    public var modelConfigID: String
    public var systemPrompt: String
    public var context: String
    public var toolName: String
    public var schema: Data
    public init(modelConfigID: String, systemPrompt: String, context: String, toolName: String, schema: Data) {
        self.modelConfigID = modelConfigID; self.systemPrompt = systemPrompt; self.context = context
        self.toolName = toolName; self.schema = schema
    }
}

public protocol StoryPlanningServicing: Sendable {
    func plan(_ request: StoryPlanningRequest) async throws -> Data
}

public protocol ResumableVideoGenerationServicing: MediaGenerationServicing {
    func resumeVideo(_ request: VideoGenerationRequest, jobID: String,
                     progress: @escaping @Sendable (VideoGenerationProgress) async -> Void) async throws -> VideoGenerationResult
}

public protocol SessionBoundMediaGenerationServicing: MediaGenerationServicing {
    func boundToCurrentSession() async throws -> any MediaGenerationServicing
}

/// Distinguishes deterministic local/preflight rejection from a transport failure where a
/// billable provider task may already have been created.
public protocol MediaGenerationSubmissionFailure: Error {
    var requestMayHaveBeenSubmitted: Bool { get }
}

public enum StoryError: LocalizedError {
    case invalidProject, invalidPlan, missingModel, unavailable, unsafeFile, unresolvedSubmission, unsupportedDuration
    public var errorDescription: String? {
        switch self {
        case .invalidProject: "剧情数据无效：请检查标题、模型和内容长度。"
        case .invalidPlan: "剧情数据或分段计划无效：请检查分段类型、2–15 秒时长、人物、场景、关联关系和镜头时间线。"
        case .missingModel: "所选模型已不可用，请刷新模型列表并在剧情设置中重新选择。"
        case .unavailable: "当前文本模型不支持此规划协议，请选择支持工具调用的 OpenAI 兼容文本模型。"
        case .unsafeFile: "剧情素材文件无效、丢失或超出大小限制。"
        case .unresolvedSubmission: "上次提交结果尚未确认。为避免重复计费，不会自动重新提交；请先核对服务商任务记录。"
        case .unsupportedDuration: "当前视频模型不支持这个分段时长，请调整为该模型支持的时长后再生成。"
        }
    }
}
