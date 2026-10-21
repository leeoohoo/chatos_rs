import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import {
  storyProjectCursorSchema,
  storyProjectPageSchema,
  storyProjectSchema,
  storyVideoGenerationOutcomeSchema,
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

  async startPlanning(request: Parameters<StoryStudioPort["startPlanning"]>[0]) {
    const input = {
      ...scope(request),
      projectId: uuidSchema.parse(request.projectId),
      stage: z.enum(["outline", "refine"]).parse(request.stage),
      targetIds: z.array(identifierSchema).max(200).parse(request.targetIds),
      userIdeas: safeText(2_000).parse(request.userIdeas),
    };
    const value = z.object({
      run: z.unknown(),
      project: storyProjectSchema.nullable(),
      stopReason: safeText(2_000).nullable(),
    }).strip().parse(await invoke("start_story_planning", { input }));
    return value.project;
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

  async discardPresentation(request: StoryPresentationScope) {
    await invoke("discard_story_studio_presentation", { input: scope(request) });
  }
}

function scope(request: StoryPresentationScope) {
  return {
    ownerUserId: ownerUserIdSchema.parse(request.ownerUserId),
    presentationId: uuidSchema.parse(request.presentationId),
  };
}
