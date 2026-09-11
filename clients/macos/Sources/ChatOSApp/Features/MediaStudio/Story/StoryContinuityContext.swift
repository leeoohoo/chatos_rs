import ChatOSCore
import Foundation

/// Deterministic continuity data shared by planning and billable media generation.
/// The language model must not have to infer a neighboring segment's final state from
/// its synopsis when the project already contains a saved shot plan and confirmed frames.
enum StoryContinuityContext {
    static func context(_ project: StoryProject, segmentID: String) throws -> [String: Any] {
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }) else {
            throw StoryError.invalidPlan
        }
        var value: [String: Any] = ["current": segmentState(project.segments[index])]
        if index > 0 { value["previous"] = segmentState(project.segments[index - 1]) }
        if index + 1 < project.segments.count { value["next"] = segmentState(project.segments[index + 1]) }
        return value
    }

    static func sourceExcerpt(_ project: StoryProject, segmentID: String) throws -> String {
        guard let segment = project.segments.first(where: { $0.id == segmentID }),
              segment.sourceRange.start >= 0, segment.sourceRange.end <= project.source.count,
              segment.sourceRange.end >= segment.sourceRange.start,
              segment.kind == .transition || segment.sourceRange.end > segment.sourceRange.start else {
            throw StoryError.invalidPlan
        }
        if segment.kind == .transition && segment.sourceRange.start == segment.sourceRange.end {
            return "（转场分段位于原文边界 \(segment.sourceRange.start)，不额外消耗剧情原文。）"
        }
        return String(project.source.dropFirst(segment.sourceRange.start)
            .prefix(segment.sourceRange.end - segment.sourceRange.start))
    }

    static func previousTail(_ project: StoryProject, segmentID: String) -> (segment: StorySegment, image: StoryImage)? {
        guard let index = project.segments.firstIndex(where: { $0.id == segmentID }), index > 0,
              let image = project.segments[index - 1].lastFrame else { return nil }
        return (project.segments[index - 1], image)
    }

    /// Directly reuses a confirmed tail as the next segment's confirmed first frame.
    /// This is a local data reconciliation only: it never invokes an image model.
    @discardableResult
    static func reconcileInheritedFirstFrames(_ project: inout StoryProject) -> Bool {
        guard project.segments.count > 1 else { return false }
        var changed = false
        for index in 1..<project.segments.count {
            let previousSegmentID = project.segments[index - 1].id
            let currentConfirmedID = project.segments[index].firstFrames.confirmedImageID
            let currentIsInherited = project.segments[index].inheritedFirstFrameSourceSegmentID != nil
            guard let tail = project.segments[index - 1].lastFrame else {
                if currentIsInherited {
                    project.segments[index].firstFrames.confirmedImageID = nil
                    project.segments[index].inheritedFirstFrameSourceSegmentID = nil
                    changed = true
                }
                continue
            }
            if !project.segments[index].firstFrames.images.contains(where: { $0.id == tail.id }) {
                project.segments[index].firstFrames.images.append(tail)
                changed = true
            }
            if currentConfirmedID == nil || currentIsInherited {
                if project.segments[index].firstFrames.confirmedImageID != tail.id {
                    project.segments[index].firstFrames.confirmedImageID = tail.id
                    changed = true
                }
                if project.segments[index].inheritedFirstFrameSourceSegmentID != previousSegmentID {
                    project.segments[index].inheritedFirstFrameSourceSegmentID = previousSegmentID
                    changed = true
                }
            }
        }
        return changed
    }

    private static func segmentState(_ segment: StorySegment) -> [String: Any] {
        var value: [String: Any] = [
            "id": segment.id,
            "title": segment.title,
            "synopsis": clipped(segment.synopsis, limit: 1_000),
            "kind": segment.kind.rawValue,
            "seconds": segment.seconds,
            "hasConfirmedFirstFrame": segment.firstFrame != nil,
            "hasConfirmedLastFrame": segment.lastFrame != nil,
            "videoCompleted": segment.video != nil,
        ]
        if let detail = segment.detail {
            value["firstFramePrompt"] = clipped(detail.firstFramePrompt, limit: 2_500)
            value["lastFramePrompt"] = clipped(detail.effectiveLastFramePrompt, limit: 2_500)
            value["continuityIn"] = clipped(detail.continuityIn, limit: 1_000)
            value["continuityOut"] = clipped(detail.continuityOut, limit: 1_000)
            value["constraints"] = clipped(detail.constraints, limit: 1_500)
            value["shots"] = detail.shots.map {
                ["start": $0.start, "end": $0.end, "prompt": clipped($0.prompt, limit: 1_200)] as [String: Any]
            }
        }
        if let attempt = segment.attempt {
            value["videoRequest"] = [
                "modelConfigID": attempt.modelConfigID,
                "size": attempt.size,
                "ratio": attempt.ratio,
                "status": attempt.status,
            ]
        }
        if let video = segment.video { value["videoModelName"] = video.modelName }
        return value
    }

    private static func clipped(_ value: String, limit: Int) -> String {
        String(value.prefix(limit))
    }
}
