import { invoke } from "@tauri-apps/api/core";
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
} from "../domain/mediaStudio";
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
