import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { z } from "zod";
import {
  isReferencePreviewForPresentation,
  mediaStudioGenerationOutcomeSchema,
  mediaStudioGenerationRequestSchema,
  mediaStudioHistoryCursorSchema,
  mediaStudioHistoryPageSchema,
  mediaStudioModelListSchema,
  mediaStudioReferenceIdSchema,
  mediaStudioReferenceImportResultSchema,
  mediaStudioUuidSchema,
  mediaStudioVideoAudioDraftSchema,
  mediaStudioVideoGenerationRequestSchema,
  mediaStudioVideoHistoryCursorSchema,
  mediaStudioVideoHistoryPageSchema,
  mediaStudioVideoJobSnapshotSchema,
} from "../domain/mediaStudio";
import {
  mediaWorkflowListSchema,
  mediaWorkflowRunEventSchema,
  mediaWorkflowRunResultSchema,
  mediaWorkflowSchema,
  type MediaWorkflow,
} from "../domain/mediaWorkflow";
import type {
  MediaStudioHistoryReference,
  MediaStudioPort,
  MediaStudioPresentationScope,
} from "../ports/MediaStudioPort";

const ownerUserIdSchema = z.string().trim().min(1).max(1_024);
const historyReferenceSchema = z.object({
  recordId: mediaStudioUuidSchema,
  contentToken: z.string().regex(/^[0-9a-f]{32}$/u),
}).strict();

export class TauriMediaStudioAdapter implements MediaStudioPort {
  async listWorkflows(ownerUserId: string) {
    return mediaWorkflowListSchema.parse(
      await invoke("list_media_studio_workflows", { ownerUserId: ownerUserIdSchema.parse(ownerUserId) }),
    );
  }

  async saveWorkflow(ownerUserId: string, workflow: MediaWorkflow) {
    const input = mediaWorkflowSchema.parse(workflow);
    return z.boolean().parse(await invoke("save_media_studio_workflow", {
      ownerUserId: ownerUserIdSchema.parse(ownerUserId),
      workflow: input,
    }));
  }

  async deleteWorkflow(ownerUserId: string, workflowId: string) {
    await invoke("delete_media_studio_workflow", {
      input: {
        ownerUserId: ownerUserIdSchema.parse(ownerUserId),
        workflowId: mediaWorkflowSchema.shape.id.parse(workflowId),
      },
    });
  }

  async watchWorkflowRun(
    request: Parameters<MediaStudioPort["watchWorkflowRun"]>[0],
    listener: Parameters<MediaStudioPort["watchWorkflowRun"]>[1],
  ) {
    const expected = {
      ownerUserId: ownerUserIdSchema.parse(request.ownerUserId),
      workflowId: mediaWorkflowSchema.shape.id.parse(request.workflowId),
      clientRequestId: ownerUserIdSchema.parse(request.clientRequestId),
    };
    return listen<unknown>("media-workflow-run", (event) => {
      const parsed = mediaWorkflowRunEventSchema.safeParse(event.payload);
      if (!parsed.success
        || parsed.data.ownerUserId !== expected.ownerUserId
        || parsed.data.workflowId !== expected.workflowId
        || parsed.data.clientRequestId !== expected.clientRequestId) return;
      listener(parsed.data);
    });
  }

  async runWorkflow(request: Parameters<MediaStudioPort["runWorkflow"]>[0]) {
    const ownerUserId = ownerUserIdSchema.parse(request.ownerUserId);
    const workflowId = mediaWorkflowSchema.shape.id.parse(request.workflowId);
    const clientRequestId = ownerUserIdSchema.parse(request.clientRequestId);
    const result = mediaWorkflowRunResultSchema.parse(
      await invoke("run_media_studio_workflow", {
        request: { ownerUserId, workflowId, clientRequestId },
      }),
    );
    if (result.workflowId !== workflowId || result.clientRequestId !== clientRequestId) {
      throw new Error("工作流运行结果与当前请求不匹配。");
    }
    return result;
  }

  async listModels(ownerUserId: string) {
    return mediaStudioModelListSchema.parse(
      await invoke("list_media_studio_models", {
        ownerUserId: ownerUserIdSchema.parse(ownerUserId),
      }),
    );
  }

  async loadImageHistory(request: Parameters<MediaStudioPort["loadImageHistory"]>[0]) {
    const ownerUserId = ownerUserIdSchema.parse(request.ownerUserId);
    const after = request.after == null
      ? null
      : mediaStudioHistoryCursorSchema.parse(request.after);
    const limit = request.limit == null
      ? null
      : z.number().int().min(1).max(200).parse(request.limit);
    return mediaStudioHistoryPageSchema.parse(
      await invoke("load_media_studio_image_history", { ownerUserId, after, limit }),
    );
  }

  async loadVideoHistory(request: Parameters<MediaStudioPort["loadVideoHistory"]>[0]) {
    const ownerUserId = ownerUserIdSchema.parse(request.ownerUserId);
    const after = request.after == null
      ? null
      : mediaStudioVideoHistoryCursorSchema.parse(request.after);
    const limit = request.limit == null
      ? null
      : z.number().int().min(1).max(200).parse(request.limit);
    return mediaStudioVideoHistoryPageSchema.parse(
      await invoke("load_media_studio_video_history", { ownerUserId, after, limit }),
    );
  }

  async pickReferenceImages(request: MediaStudioPresentationScope) {
    const input = presentationScope(request);
    return parseReferenceImport(
      await invoke("pick_media_studio_reference_images", { input }),
      input.presentationId,
    );
  }

  async stageHistoryReferences(
    request: Parameters<MediaStudioPort["stageHistoryReferences"]>[0],
  ) {
    const input = {
      ...presentationScope(request),
      assets: z.array(historyReferenceSchema).max(8).parse(request.assets).map(exactHistoryReference),
    };
    return parseReferenceImport(
      await invoke("stage_media_studio_history_references", { input }),
      input.presentationId,
    );
  }

  async discardReferenceImages(
    request: Parameters<MediaStudioPort["discardReferenceImages"]>[0],
  ) {
    const input = {
      ...presentationScope(request),
      referenceIds: z.array(mediaStudioReferenceIdSchema).max(8).parse(request.referenceIds),
    };
    await invoke("discard_media_studio_reference_images", { input });
  }

  async pickVideoReferenceAudio(request: MediaStudioPresentationScope) {
    const input = presentationScope(request);
    const value = await invoke("pick_media_studio_video_reference_audio", { input });
    return value == null ? null : mediaStudioVideoAudioDraftSchema.parse(value);
  }

  async discardVideoReferenceAudio(
    request: Parameters<MediaStudioPort["discardVideoReferenceAudio"]>[0],
  ) {
    const input = {
      ...presentationScope(request),
      referenceAudioId: mediaStudioVideoAudioDraftSchema.shape.id.parse(request.referenceAudioId),
    };
    await invoke("discard_media_studio_video_reference_audio", { input });
  }

  async discardPresentation(request: MediaStudioPresentationScope) {
    await invoke("discard_media_studio_presentation", { input: presentationScope(request) });
  }

  async generateImage(request: Parameters<MediaStudioPort["generateImage"]>[0]) {
    const ownerUserId = ownerUserIdSchema.parse(request.ownerUserId);
    const input = mediaStudioGenerationRequestSchema.parse(request.request);
    const outcome = mediaStudioGenerationOutcomeSchema.parse(
      await invoke("generate_media_studio_image", { ownerUserId, request: input }),
    );
    if (outcome.status === "generatedWithoutPersistence"
      && (outcome.result.clientRequestId !== input.clientRequestId
        || outcome.result.modelConfigId !== input.modelConfigId)) {
      throw new Error("图片生成结果与当前请求不匹配。");
    }
    return outcome;
  }

  async startVideoGeneration(request: Parameters<MediaStudioPort["startVideoGeneration"]>[0]) {
    const ownerUserId = ownerUserIdSchema.parse(request.ownerUserId);
    const input = mediaStudioVideoGenerationRequestSchema.parse(request.request);
    const snapshot = mediaStudioVideoJobSnapshotSchema.parse(
      await invoke("start_media_studio_video_generation", { ownerUserId, request: input }),
    );
    if (snapshot.clientRequestId !== input.clientRequestId) {
      throw new Error("视频生成任务与当前请求不匹配。");
    }
    return snapshot;
  }

  async pollVideoGeneration(request: Parameters<MediaStudioPort["pollVideoGeneration"]>[0]) {
    const input = videoJobInput(request);
    const clientRequestId = z.string().trim().min(1).max(1_024).parse(request.clientRequestId);
    const snapshot = mediaStudioVideoJobSnapshotSchema.parse(
      await invoke("poll_media_studio_video_generation", { input }),
    );
    if (snapshot.jobId !== input.jobId || snapshot.clientRequestId !== clientRequestId) {
      throw new Error("视频生成状态与当前任务不匹配。");
    }
    return snapshot;
  }

  async cancelVideoGeneration(request: Parameters<MediaStudioPort["cancelVideoGeneration"]>[0]) {
    await invoke("cancel_media_studio_video_generation", { input: videoJobInput(request) });
  }
}

function presentationScope(request: MediaStudioPresentationScope) {
  return {
    ownerUserId: ownerUserIdSchema.parse(request.ownerUserId),
    presentationId: mediaStudioUuidSchema.parse(request.presentationId),
  };
}

function exactHistoryReference(reference: MediaStudioHistoryReference) {
  return { recordId: reference.recordId, contentToken: reference.contentToken };
}

function videoJobInput(request: MediaStudioPresentationScope & { jobId: string }) {
  return {
    ...presentationScope(request),
    jobId: mediaStudioUuidSchema.parse(request.jobId),
  };
}

function parseReferenceImport(value: unknown, presentationId: string) {
  const result = mediaStudioReferenceImportResultSchema.parse(value);
  if (result.references.some(
    (reference) => reference.previewUrl != null
      && !isReferencePreviewForPresentation(reference.previewUrl, presentationId),
  )) {
    throw new Error("参考图预览地址与当前图片创作页面不匹配。");
  }
  return result;
}
