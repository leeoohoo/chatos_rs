import Foundation

/// Pure Story media mutation boundary shared by native adapters and presentation code.
/// It owns attachment, confirmation and continuity invalidation rules without reading
/// files, calling providers or persisting a project.
public struct StoryMediaConfirmationUseCase: Sendable {
    public init() {}

    public func updatingResourcePrompt(
        resourceID: String,
        prompt: String,
        in project: StoryProject
    ) throws -> StoryProject? {
        var next = project
        guard var resource = next.resource(id: resourceID) else { return nil }
        guard !hasUnresolvedDependentVideo(resourceID: resourceID, in: next) else {
            throw StoryError.unresolvedSubmission
        }
        resource.prompt = prompt
        resource.media.confirmedImageID = nil
        try next.replaceResource(resource)
        invalidateDependentFrames(resourceID: resourceID, in: &next)
        try next.validate()
        return next
    }

    public func attachingImportedImage(
        _ image: StoryImage,
        target: StoryImageImportTarget,
        confirmsImportedImage: Bool,
        to project: StoryProject
    ) throws -> StoryProject {
        var next = project
        switch target {
        case .resource(let resourceID):
            guard var resource = next.resource(id: resourceID) else {
                throw StoryMediaConfirmationError.invalidTarget
            }
            resource.media.images.append(image)
            let automaticallyConfirmed = confirmsImportedImage
                || resource.confirmedImageID == nil
            if automaticallyConfirmed {
                resource.media.confirmedImageID = image.id
            }
            try next.replaceResource(resource)
            if automaticallyConfirmed {
                invalidateDependentFrames(resourceID: resourceID, in: &next)
            }
        case .frame(let segmentID, let role):
            guard let index = editableSegmentIndex(segmentID, in: next) else {
                throw StoryMediaConfirmationError.invalidTarget
            }
            switch role {
            case .first:
                next.segments[index].firstFrames.images.append(image)
                if confirmsImportedImage
                    || next.segments[index].confirmedFrameID == nil {
                    selectFirstFrame(image.id, at: index, in: &next)
                }
            case .last:
                next.segments[index].lastFrames.images.append(image)
                if confirmsImportedImage
                    || next.segments[index].confirmedLastFrameID == nil {
                    next.segments[index].confirmedLastFrameID = image.id
                }
            }
        }
        try next.validate()
        return next
    }

    public func confirmingImage(
        _ imageID: UUID,
        target: StoryImageImportTarget,
        in project: StoryProject
    ) throws -> StoryProject? {
        var next = project
        switch target {
        case .resource(let resourceID):
            guard var resource = next.resource(id: resourceID),
                  resource.images.contains(where: { $0.id == imageID }),
                  resource.confirmedImageID != imageID else { return nil }
            guard !hasUnresolvedDependentVideo(resourceID: resourceID, in: next) else {
                throw StoryError.unresolvedSubmission
            }
            resource.media.confirmedImageID = imageID
            try next.replaceResource(resource)
            invalidateDependentFrames(resourceID: resourceID, in: &next)
        case .frame(let segmentID, let role):
            guard let index = editableSegmentIndex(segmentID, in: next) else { return nil }
            switch role {
            case .first:
                guard next.segments[index].firstFrames.images.contains(where: { $0.id == imageID }) else {
                    return nil
                }
                selectFirstFrame(imageID, at: index, in: &next)
            case .last:
                guard next.segments[index].lastFrames.images.contains(where: { $0.id == imageID }) else {
                    return nil
                }
                next.segments[index].confirmedLastFrameID = imageID
            }
        }
        try next.validate()
        return next
    }

    public func clearingConfirmedLastFrame(
        segmentID: String,
        in project: StoryProject
    ) throws -> StoryProject? {
        var next = project
        guard let index = editableSegmentIndex(segmentID, in: next),
              next.segments[index].video == nil,
              next.segments[index].confirmedLastFrameID != nil
                || next.segments[index].useLastFrameForVideo else { return nil }
        next.segments[index].confirmedLastFrameID = nil
        next.segments[index].useLastFrameForVideo = false
        try next.validate()
        return next
    }

    public func confirmingLatestResourceImages(
        resourceIDs: [String],
        in project: StoryProject
    ) throws -> StoryProject? {
        let requested = Set(resourceIDs)
        guard !requested.isEmpty else { return nil }
        var next = project
        var changed = false
        for resourceID in next.resources.map(\.id) where requested.contains(resourceID) {
            guard var resource = next.resource(id: resourceID),
                  let latest = resource.images.last,
                  resource.confirmedImageID != latest.id else { continue }
            guard !hasUnresolvedDependentVideo(resourceID: resourceID, in: next) else {
                throw StoryError.unresolvedSubmission
            }
            resource.media.confirmedImageID = latest.id
            try next.replaceResource(resource)
            invalidateDependentFrames(resourceID: resourceID, in: &next)
            changed = true
        }
        guard changed else { return nil }
        try next.validate()
        return next
    }

    public func confirmingLatestFrames(
        segmentID: String,
        useConfirmedLastFrameForVideo: Bool,
        in project: StoryProject
    ) throws -> StoryProject? {
        var next = project
        guard let index = editableSegmentIndex(segmentID, in: next),
              next.segments[index].video == nil else { return nil }
        var changed = false
        if next.segments[index].confirmedFrameID == nil,
           let latest = next.segments[index].firstFrames.images.last {
            selectFirstFrame(latest.id, at: index, in: &next)
            changed = true
        }
        if next.segments[index].confirmedLastFrameID == nil,
           let latest = next.segments[index].lastFrames.images.last {
            next.segments[index].confirmedLastFrameID = latest.id
            changed = true
        }
        if useConfirmedLastFrameForVideo,
           next.segments[index].confirmedLastFrameID != nil,
           !next.segments[index].useLastFrameForVideo {
            next.segments[index].useLastFrameForVideo = true
            changed = true
        }
        guard changed else { return nil }
        try next.validate()
        return next
    }

    public func applyingRefinedDetail(
        _ detail: StorySegmentDetail,
        segmentID: String,
        in project: StoryProject
    ) throws -> StoryProject {
        var next = project
        guard let index = editableSegmentIndex(segmentID, in: next),
              next.segments[index].video == nil else {
            throw StoryMediaConfirmationError.invalidTarget
        }
        try detail.validate(duration: next.segments[index].seconds)
        next.segments[index].detail = detail
        next.segments[index].confirmedFrameID = nil
        next.segments[index].confirmedLastFrameID = nil
        next.segments[index].useLastFrameForVideo = false
        try next.validate()
        return next
    }

    public func applyingSegmentEdit(
        _ edited: StorySegment,
        relations: [StorySegmentRelation],
        to project: StoryProject
    ) throws -> StoryProject? {
        var next = project
        guard let index = editableSegmentIndex(edited.id, in: next),
              next.segments[index].video == nil else { return nil }
        if let detail = edited.detail {
            try detail.validate(duration: edited.seconds)
        }
        let original = next.segments[index]
        let originalRelations = next.relations(for: edited.id)
        if changesContinuity(
            original: original,
            edited: edited,
            originalRelations: originalRelations,
            relations: relations
        ) {
            for adjacent in [index - 1, index + 1]
                where next.segments.indices.contains(adjacent)
                    && next.segments[adjacent].attempt == nil {
                next.segments[adjacent].detail = nil
                next.segments[adjacent].confirmedFrameID = nil
                next.segments[adjacent].confirmedLastFrameID = nil
            }
        }
        next.segments[index].title = edited.title
        next.segments[index].synopsis = edited.synopsis
        next.segments[index].kind = edited.kind
        next.segments[index].seconds = edited.seconds
        next.segments[index].detail = edited.detail
        next.segments[index].characterIDs = edited.characterIDs
        next.segments[index].sceneIDs = edited.sceneIDs
        next.segments[index].propIDs = edited.propIDs
        next.segments[index].videoGuidanceMode = edited.videoGuidanceMode
        next.relations.removeAll { $0.segmentID == edited.id }
        next.relations.append(contentsOf: relations)
        next.segments[index].confirmedFrameID = nil
        next.segments[index].confirmedLastFrameID = nil
        try next.validate()
        return next
    }

    public func invalidatingAllSegmentPlans(in project: StoryProject) -> StoryProject {
        var next = project
        for index in next.segments.indices {
            next.segments[index].detail = nil
            next.segments[index].confirmedFrameID = nil
            next.segments[index].confirmedLastFrameID = nil
        }
        return next
    }

    private func editableSegmentIndex(_ segmentID: String, in project: StoryProject) -> Int? {
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }),
              project.segments[index].attempt == nil else { return nil }
        return index
    }

    private func hasUnresolvedDependentVideo(
        resourceID: String,
        in project: StoryProject
    ) -> Bool {
        project.segments.contains {
            $0.resourceIDs.contains(resourceID) && $0.attempt != nil && $0.video == nil
        }
    }

    private func changesContinuity(
        original: StorySegment,
        edited: StorySegment,
        originalRelations: [StorySegmentRelation],
        relations: [StorySegmentRelation]
    ) -> Bool {
        original.synopsis != edited.synopsis
            || original.kind != edited.kind
            || original.seconds != edited.seconds
            || original.detail?.continuityIn != edited.detail?.continuityIn
            || original.detail?.continuityOut != edited.detail?.continuityOut
            || original.characterIDs != edited.characterIDs
            || original.sceneIDs != edited.sceneIDs
            || original.propIDs != edited.propIDs
            || originalRelations != relations
    }

    private func invalidateDependentFrames(resourceID: String, in project: inout StoryProject) {
        for index in project.segments.indices
            where project.segments[index].resourceIDs.contains(resourceID)
                && project.segments[index].attempt == nil {
            project.segments[index].confirmedFrameID = nil
            project.segments[index].confirmedLastFrameID = nil
        }
    }

    private func selectFirstFrame(_ imageID: UUID, at index: Int, in project: inout StoryProject) {
        project.segments[index].confirmedFrameID = imageID
        project.segments[index].userSelectedFirstFrameID = imageID
        project.segments[index].inheritedFirstFrameSourceSegmentID = nil
    }
}

public enum StoryMediaConfirmationError: LocalizedError, Equatable, Sendable {
    case invalidTarget

    public var errorDescription: String? {
        switch self {
        case .invalidTarget:
            "剧情媒体确认目标已经变化，请重新选择后再试。"
        }
    }
}
