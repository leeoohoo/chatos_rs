import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { z } from "zod";
import {
  storyProjectCursorSchema,
  storyProjectPageSchema,
  storyProjectSchema,
  storyPlanningEventSchema,
  storyPlanningHistoryPageSchema,
  storyPlanningOutcomeSchema,
  storyPlanningRunSchema,
  storySegmentSchema,
  storyMediaBatchKindSchema,
  storyMediaBatchPreviewResultSchema,
  storyMediaBatchHistoryPageSchema,
  storyMediaBatchEventSchema,
  storyMediaBatchExecutionOutcomeSchema,
  storyMediaBatchSnapshotSchema,
  storyDirectVideoQueueOutcomeSchema,
  storyVideoGenerationOutcomeSchema,
  storyVideoGenerationEventSchema,
} from "../domain/storyStudio";
import type { StoryPresentationScope, StoryStudioPort } from "../ports/StoryStudioPort";

const ownerUserIdSchema = z.string().trim().min(1).max(1_024);
const uuidSchema = z.string().uuid().refine((value) => value === value.toLowerCase());
const identifierSchema = z.string().trim().min(1).max(1_024);
const fingerprintSchema = z.string().regex(/^[0-9a-f]{64}$/u);
const safeText = (maximum: number) => z.string().max(maximum).refine(
  (value) => !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u.test(value),
);

export class TauriStoryStudioAdapter implements StoryStudioPort {
  async watchVideoGeneration(
    request: Parameters<StoryStudioPort["watchVideoGeneration"]>[0],
    listener: Parameters<StoryStudioPort["watchVideoGeneration"]>[1],
  ) {
    const expected = scope(request);
    return listen<unknown>("story-video-generation", (event) => {
      const parsed = storyVideoGenerationEventSchema.safeParse(event.payload);
      if (!parsed.success || parsed.data.presentationId !== expected.presentationId) return;
      listener(parsed.data);
    });
  }

  async watchMediaBatches(
    request: Parameters<StoryStudioPort["watchMediaBatches"]>[0],
    listener: Parameters<StoryStudioPort["watchMediaBatches"]>[1],
  ) {
    const expected = scope(request);
    return listen<unknown>("story-media-batch", (event) => {
      const parsed = storyMediaBatchEventSchema.safeParse(event.payload);
      if (!parsed.success
        || parsed.data.presentationId !== expected.presentationId
        || parsed.data.batch.ownerUserId !== expected.ownerUserId) return;
      listener(parsed.data);
    });
  }

  async watchPlanning(
    request: Parameters<StoryStudioPort["watchPlanning"]>[0],
    listener: Parameters<StoryStudioPort["watchPlanning"]>[1],
  ) {
    const expected = scope(request);
    return listen<unknown>("story-planning-run", (event) => {
      const parsed = storyPlanningEventSchema.safeParse(event.payload);
      if (!parsed.success
        || parsed.data.presentationId !== expected.presentationId
        || parsed.data.run.ownerUserId !== expected.ownerUserId) return;
      listener(parsed.data);
    });
  }

  async loadProjects(request: Parameters<StoryStudioPort["loadProjects"]>[0]) {
    const input = {
      ...scope(request),
      after: request.after == null ? null : storyProjectCursorSchema.parse(request.after),
      limit: request.limit == null ? null : z.number().int().min(1).max(200).parse(request.limit),
    };
    return storyProjectPageSchema.parse(await invoke("load_story_projects", { input }));
  }

  async loadProject(request: Parameters<StoryStudioPort["loadProject"]>[0]) {
    const input = { ...scope(request), projectId: uuidSchema.parse(request.projectId) };
    return storyProjectSchema.parse(await invoke("load_story_project", { input }));
  }

  async previewMediaBatch(request: Parameters<StoryStudioPort["previewMediaBatch"]>[0]) {
    const input = {
      ...scope(request),
      preview: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        kind: storyMediaBatchKindSchema.parse(request.kind),
        targetIds: request.targetIds == null
          ? null
          : z.array(identifierSchema).min(1).max(500).parse(request.targetIds),
        userIdeas: safeText(2_000).parse(request.userIdeas),
      },
    };
    return storyMediaBatchPreviewResultSchema.parse(
      await invoke("preview_story_media_batch", { input }),
    );
  }

  async loadMediaBatchHistory(request: Parameters<StoryStudioPort["loadMediaBatchHistory"]>[0]) {
    const input = {
      ...scope(request),
      projectId: uuidSchema.parse(request.projectId),
      after: request.after == null ? null : storyProjectCursorSchema.parse(request.after),
      limit: request.limit == null ? null : z.number().int().min(1).max(200).parse(request.limit),
    };
    return storyMediaBatchHistoryPageSchema.parse(
      await invoke("load_story_media_batch_history", { input }),
    );
  }

  async startMediaBatch(request: Parameters<StoryStudioPort["startMediaBatch"]>[0]) {
    const preparation = request.preparation;
    const input = {
      ...scope(request),
      request: {
        projectId: uuidSchema.parse(preparation.projectId),
        baseFingerprint: fingerprintSchema.parse(preparation.projectFingerprint),
        previewId: uuidSchema.parse(preparation.previewId),
        kind: storyMediaBatchKindSchema.exclude(["videos"]).parse(preparation.kind),
        targetIds: z.array(identifierSchema).min(1).max(500).parse(preparation.requestedTargetIds),
        userIdeas: safeText(2_000).parse(request.userIdeas),
      },
    };
    return storyMediaBatchSnapshotSchema.parse(
      await invoke("start_story_media_batch", { input }),
    );
  }

  async startDirectVideoQueue(
    request: Parameters<StoryStudioPort["startDirectVideoQueue"]>[0],
  ) {
    const preparation = request.preparation;
    const input = {
      ...scope(request),
      request: {
        projectId: uuidSchema.parse(preparation.projectId),
        baseFingerprint: fingerprintSchema.parse(preparation.projectFingerprint),
        previewId: uuidSchema.parse(preparation.previewId),
        kind: z.literal("videos").parse(preparation.kind),
        targetIds: z.array(identifierSchema).min(1).max(500).parse(preparation.requestedTargetIds),
        userIdeas: safeText(2_000).parse(request.userIdeas),
      },
    };
    return storyDirectVideoQueueOutcomeSchema.parse(
      await invoke("start_story_direct_video_queue", { input }),
    );
  }

  async abandonMediaBatch(request: Parameters<StoryStudioPort["abandonMediaBatch"]>[0]) {
    const input = {
      ...scope(request),
      projectId: uuidSchema.parse(request.projectId),
      batchId: uuidSchema.parse(request.batchId),
    };
    return storyMediaBatchSnapshotSchema.parse(
      await invoke("abandon_story_media_batch", { input }),
    );
  }

  async resumeMediaBatch(request: Parameters<StoryStudioPort["resumeMediaBatch"]>[0]) {
    const input = {
      ...scope(request),
      projectId: uuidSchema.parse(request.projectId),
      batchId: uuidSchema.parse(request.batchId),
    };
    return storyMediaBatchExecutionOutcomeSchema.parse(
      await invoke("resume_story_media_batch", { input }),
    );
  }

  async createProject(request: Parameters<StoryStudioPort["createProject"]>[0]) {
    const input = {
      ...scope(request),
      project: {
        title: safeText(120).trim().min(1).parse(request.title),
        description: safeText(4_000).parse(request.description),
        models: {
          textModelId: identifierSchema.parse(request.textModelId),
          imageModelId: identifierSchema.parse(request.imageModelId),
          videoModelId: identifierSchema.parse(request.videoModelId),
        },
      },
    };
    return storyProjectSchema.parse(await invoke("create_story_project", { input }));
  }

  async updateProjectSettings(
    request: Parameters<StoryStudioPort["updateProjectSettings"]>[0],
  ) {
    const input = {
      ...scope(request),
      change: {
        projectId: uuidSchema.parse(request.projectId),
        title: safeText(120).trim().min(1).parse(request.title),
        description: safeText(4_000).parse(request.description),
        models: {
          textModelId: identifierSchema.parse(request.textModelId),
          imageModelId: identifierSchema.parse(request.imageModelId),
          videoModelId: identifierSchema.parse(request.videoModelId),
        },
      },
    };
    return storyProjectSchema.parse(
      await invoke("update_story_project_settings", { input }),
    );
  }

  async updateSource(request: Parameters<StoryStudioPort["updateSource"]>[0]) {
    const input = {
      ...scope(request),
      edit: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        source: safeText(80_000).parse(request.source),
        style: safeText(2_000).parse(request.style),
        ratio: z.enum(["16:9", "9:16", "1:1", "4:3", "3:4", "21:9"])
          .parse(request.ratio),
      },
    };
    return storyProjectSchema.parse(await invoke("update_story_project_source", { input }));
  }

  async updateResourcePrompt(
    request: Parameters<StoryStudioPort["updateResourcePrompt"]>[0],
  ) {
    const input = {
      ...scope(request),
      edit: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        resourceId: identifierSchema.parse(request.resourceId),
        prompt: safeText(4_000).trim().min(1).parse(request.prompt),
      },
    };
    return storyProjectSchema.parse(
      await invoke("update_story_resource_prompt", { input }),
    );
  }

  async startPlanning(request: Parameters<StoryStudioPort["startPlanning"]>[0]) {
    const input = {
      ...scope(request),
      projectId: uuidSchema.parse(request.projectId),
      stage: z.enum(["outline", "refine"]).parse(request.stage),
      targetIds: z.array(identifierSchema).max(200).parse(request.targetIds),
      userIdeas: safeText(2_000).parse(request.userIdeas),
    };
    const value = storyPlanningOutcomeSchema.parse(await invoke("start_story_planning", { input }));
    return value.project;
  }

  async loadPlanningHistory(request: Parameters<StoryStudioPort["loadPlanningHistory"]>[0]) {
    const input = {
      ...scope(request),
      projectId: uuidSchema.parse(request.projectId),
      after: request.after == null ? null : storyProjectCursorSchema.parse(request.after),
      limit: request.limit == null ? null : z.number().int().min(1).max(200).parse(request.limit),
    };
    return storyPlanningHistoryPageSchema.parse(
      await invoke("load_story_planning_history", { input }),
    );
  }

  async resumePlanning(request: Parameters<StoryStudioPort["resumePlanning"]>[0]) {
    const input = {
      ...scope(request),
      projectId: uuidSchema.parse(request.projectId),
      runId: uuidSchema.parse(request.runId),
    };
    const value = storyPlanningOutcomeSchema.parse(await invoke("resume_story_planning", { input }));
    return value.project;
  }

  async abandonPlanning(request: Parameters<StoryStudioPort["abandonPlanning"]>[0]) {
    const input = {
      ...scope(request),
      projectId: uuidSchema.parse(request.projectId),
      runId: uuidSchema.parse(request.runId),
    };
    return storyPlanningRunSchema.parse(await invoke("abandon_story_planning", { input }));
  }

  async importLocalImage(request: Parameters<StoryStudioPort["importLocalImage"]>[0]) {
    const input = {
      ...scope(request),
      import: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        ...imageImportTarget(request.target),
        confirmsImportedImage: true,
      },
    };
    const value = await invoke("import_local_story_image", { input });
    return value == null ? null : storyProjectSchema.parse(value);
  }

  async importHistoryImage(request: Parameters<StoryStudioPort["importHistoryImage"]>[0]) {
    const input = {
      ...scope(request),
      recordId: uuidSchema.parse(request.recordId),
      contentToken: z.string().regex(/^[0-9a-f]{32}$/u).parse(request.contentToken),
      import: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        ...imageImportTarget(request.target),
        confirmsImportedImage: true,
      },
    };
    return storyProjectSchema.parse(await invoke("import_story_history_image", { input }));
  }

  async updateSegmentDuration(request: Parameters<StoryStudioPort["updateSegmentDuration"]>[0]) {
    const input = {
      ...scope(request),
      change: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        segmentId: identifierSchema.parse(request.segmentId),
        seconds: z.number().int().min(2).max(15).parse(request.seconds),
      },
    };
    return storyProjectSchema.parse(await invoke("update_story_segment_duration", { input }));
  }

  async updateSegment(request: Parameters<StoryStudioPort["updateSegment"]>[0]) {
    const relationSchema = z.object({
      id: identifierSchema,
      characterId: identifierSchema,
      sceneId: identifierSchema,
      action: safeText(400),
      position: safeText(400),
      startSecond: z.number().int().min(0).max(15),
      endSecond: z.number().int().min(1).max(15),
    }).strict();
    const segmentSchema = z.object({
      segmentId: identifierSchema,
      title: safeText(120).trim().min(1),
      synopsis: safeText(2_000),
      kind: z.enum(["story", "transition"]),
      seconds: z.number().int().min(2).max(15),
      detail: storySegmentSchema.shape.detail,
      characterIds: z.array(identifierSchema).max(8),
      sceneIds: z.array(identifierSchema).max(8),
      propIds: z.array(identifierSchema).max(8),
      videoGuidanceMode: storySegmentSchema.shape.videoGuidanceMode,
      relations: z.array(relationSchema).max(8),
    }).strict();
    const input = {
      ...scope(request),
      edit: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        segment: segmentSchema.parse(request.segment),
      },
    };
    return storyProjectSchema.parse(await invoke("update_story_segment", { input }));
  }

  async updateSegmentStructure(
    request: Parameters<StoryStudioPort["updateSegmentStructure"]>[0],
  ) {
    const operation = z.enum(["add", "delete", "move"]).parse(request.operation);
    const input = {
      ...scope(request),
      edit: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        mutation: {
          operation,
          segmentId: operation === "add"
            ? null
            : identifierSchema.parse(request.segmentId),
          offset: operation === "move"
            ? z.union([z.literal(-1), z.literal(1)]).parse(request.offset)
            : null,
        },
      },
    };
    return storyProjectSchema.parse(
      await invoke("update_story_segment_structure", { input }),
    );
  }

  async allowImageRetry(request: Parameters<StoryStudioPort["allowImageRetry"]>[0]) {
    const input = {
      ...scope(request),
      target: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        ...imageImportTarget(request.target),
      },
    };
    return storyProjectSchema.parse(await invoke("allow_story_image_retry", { input }));
  }

  async generateAssetImage(request: Parameters<StoryStudioPort["generateAssetImage"]>[0]) {
    const input = {
      ...scope(request),
      request: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        resourceId: identifierSchema.parse(request.resourceId),
        userIdeas: safeText(1_000).parse(request.userIdeas),
      },
    };
    return storyProjectSchema.parse(await invoke("generate_story_asset_image", { input }));
  }

  async generateFrameImage(request: Parameters<StoryStudioPort["generateFrameImage"]>[0]) {
    const input = {
      ...scope(request),
      request: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        segmentId: identifierSchema.parse(request.segmentId),
        frameRole: z.enum(["first", "last"]).parse(request.frameRole),
        referenceResourceIds: z.array(identifierSchema).min(1).max(8)
          .parse(request.referenceResourceIds),
        userIdeas: safeText(1_000).parse(request.userIdeas),
      },
    };
    return storyProjectSchema.parse(await invoke("generate_story_frame_image", { input }));
  }

  async confirmLatestResourceImages(
    request: Parameters<StoryStudioPort["confirmLatestResourceImages"]>[0],
  ) {
    const input = {
      ...scope(request),
      confirmation: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        resourceIds: z.array(identifierSchema).min(1).max(100).parse(request.resourceIds),
      },
    };
    return storyProjectSchema.parse(
      await invoke("confirm_latest_story_resource_images", { input }),
    );
  }

  async confirmImage(request: Parameters<StoryStudioPort["confirmImage"]>[0]) {
    const input = {
      ...scope(request),
      confirmation: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        ...imageImportTarget(request.target),
        imageId: identifierSchema.parse(request.imageId),
      },
    };
    return storyProjectSchema.parse(await invoke("confirm_story_image", { input }));
  }

  async confirmLatestFrames(request: Parameters<StoryStudioPort["confirmLatestFrames"]>[0]) {
    const input = {
      ...scope(request),
      confirmation: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        segmentId: identifierSchema.parse(request.segmentId),
        useConfirmedLastFrameForVideo: z.boolean().parse(request.useConfirmedLastFrameForVideo),
      },
    };
    return storyProjectSchema.parse(await invoke("confirm_latest_story_frames", { input }));
  }

  async clearLastFrame(request: Parameters<StoryStudioPort["clearLastFrame"]>[0]) {
    const input = {
      ...scope(request),
      change: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        segmentId: identifierSchema.parse(request.segmentId),
      },
    };
    return storyProjectSchema.parse(await invoke("clear_story_last_frame", { input }));
  }

  async startVideo(request: Parameters<StoryStudioPort["startVideo"]>[0]) {
    const input = {
      ...scope(request),
      request: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        segmentId: identifierSchema.parse(request.segmentId),
        size: z.enum(["768P", "2K"]).parse(request.size),
        userIdeas: safeText(1_000).parse(request.userIdeas),
      },
    };
    return storyVideoGenerationOutcomeSchema.parse(
      await invoke("start_story_video_generation", { input }),
    );
  }

  async pollVideo(request: Parameters<StoryStudioPort["pollVideo"]>[0]) {
    const input = {
      ...scope(request),
      request: {
        projectId: uuidSchema.parse(request.projectId),
        segmentId: identifierSchema.parse(request.segmentId),
      },
    };
    return storyVideoGenerationOutcomeSchema.parse(
      await invoke("poll_story_video_generation", { input }),
    );
  }

  async updateVideoGuidance(request: Parameters<StoryStudioPort["updateVideoGuidance"]>[0]) {
    const input = {
      ...scope(request),
      change: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        segmentId: identifierSchema.parse(request.segmentId),
        mode: z.enum(["first_frame", "first_and_last_frames", "previous_video", "source_video"])
          .parse(request.mode),
      },
    };
    return storyProjectSchema.parse(await invoke("update_story_video_guidance", { input }));
  }

  async archiveCompletedVideo(request: Parameters<StoryStudioPort["archiveCompletedVideo"]>[0]) {
    const input = {
      ...scope(request),
      change: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        segmentId: identifierSchema.parse(request.segmentId),
        useSourceVideo: z.boolean().parse(request.useSourceVideo),
      },
    };
    return storyProjectSchema.parse(await invoke("archive_story_completed_video", { input }));
  }

  async allowVideoRetry(request: Parameters<StoryStudioPort["allowVideoRetry"]>[0]) {
    const input = {
      ...scope(request),
      change: {
        projectId: uuidSchema.parse(request.projectId),
        baseFingerprint: fingerprintSchema.parse(request.baseFingerprint),
        segmentId: identifierSchema.parse(request.segmentId),
      },
    };
    return storyProjectSchema.parse(await invoke("allow_story_video_retry", { input }));
  }

  async discardPresentation(request: StoryPresentationScope) {
    await invoke("discard_story_studio_presentation", { input: scope(request) });
  }
}

function imageImportTarget(target: Parameters<StoryStudioPort["importLocalImage"]>[0]["target"]) {
  return target.kind === "resource"
    ? {
        resourceId: identifierSchema.parse(target.resourceId),
        segmentId: null,
        frameRole: null,
      }
    : {
        resourceId: null,
        segmentId: identifierSchema.parse(target.segmentId),
        frameRole: z.enum(["first", "last"]).parse(target.frameRole),
      };
}

function scope(request: StoryPresentationScope) {
  return {
    ownerUserId: ownerUserIdSchema.parse(request.ownerUserId),
    presentationId: uuidSchema.parse(request.presentationId),
  };
}
