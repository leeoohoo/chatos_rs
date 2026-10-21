import { describe, expect, it } from "vitest";
import {
  generatedAssetPreviewUrl,
  mediaStudioGenerationOutcomeSchema,
  mediaStudioGenerationRequestSchema,
  mediaStudioHistoryPageSchema,
  mediaStudioModelSchema,
  mediaStudioReferenceImportResultSchema,
  mediaStudioVideoAudioDraftSchema,
  mediaStudioVideoGenerationRequestSchema,
  mediaStudioVideoHistoryPageSchema,
  mediaStudioVideoJobSnapshotSchema,
} from "./mediaStudio";

const SESSION_ID = "11111111-1111-4111-8111-111111111111";
const RECORD_ID = "22222222-2222-4222-8222-222222222222";
const PRESENTATION_ID = "33333333-3333-4333-8333-333333333333";
const REFERENCE_TOKEN = "44444444-4444-4444-8444-444444444444";
const CONTENT_TOKEN = "a".repeat(32);
const REFERENCE_ID = `reference_${"b".repeat(32)}`;

describe("media studio domain", () => {
  it("accepts the native model projection and rejects credentials or provider addresses", () => {
    const model = {
      id: "image-model",
      name: "Image Model",
      provider: "openai-compatible",
      modelName: "gpt-image-1",
      enabled: true,
      taskEnabled: true,
      hasApiKey: true,
    };
    expect(mediaStudioModelSchema.parse(model).id).toBe("image-model");
    expect(() => mediaStudioModelSchema.parse({ ...model, apiKey: "secret" })).toThrow();
    expect(() => mediaStudioModelSchema.parse({ ...model, baseUrl: "https://api.example.com" }))
      .toThrow();
  });

  it("accepts only exact session-scoped history preview URLs", () => {
    const page = historyPage();
    expect(mediaStudioHistoryPageSchema.parse(page).items[0]?.id).toBe(RECORD_ID);
    expect(() => mediaStudioHistoryPageSchema.parse({
      ...page,
      items: [{
        ...page.items[0],
        images: [{ ...page.items[0]!.images[0], previewUrl: "https://cdn.example/image.png" }],
      }],
    })).toThrow();
    expect(() => mediaStudioHistoryPageSchema.parse({
      ...page,
      items: [{
        ...page.items[0],
        images: [{
          ...page.items[0]!.images[0],
          previewUrl: `chatos-media://localhost/${SESSION_ID}/${PRESENTATION_ID}/${CONTENT_TOKEN}`,
        }],
      }],
    })).toThrow();
    expect(() => mediaStudioHistoryPageSchema.parse({
      ...page,
      items: [{ ...page.items[0], absolutePath: "C:\\Users\\Alice\\image.png" }],
    })).toThrow();
  });

  it("limits imported references and binds them to the controlled preview protocol", () => {
    const result = {
      references: [{
        id: REFERENCE_ID,
        name: "reference.png",
        mimeType: "image/png",
        byteSize: 1_024,
        previewUrl: `http://chatos-media.localhost/${SESSION_ID}/reference/${PRESENTATION_ID}/${REFERENCE_TOKEN}`,
      }],
      errors: [],
    };
    expect(mediaStudioReferenceImportResultSchema.parse(result).references).toHaveLength(1);
    expect(() => mediaStudioReferenceImportResultSchema.parse({
      ...result,
      references: [{ ...result.references[0], previewUrl: "file:///C:/Users/Alice/image.png" }],
    })).toThrow();
    expect(() => mediaStudioReferenceImportResultSchema.parse({
      ...result,
      references: Array.from({ length: 9 }, (_, index) => ({
        ...result.references[0],
        id: `reference_${index.toString(16).padStart(32, "0")}`,
      })),
    })).toThrow();
  });

  it("models saved and non-persisted generation as an explicit tagged result", () => {
    const saved = { status: "saved", item: historyPage().items[0] };
    expect(mediaStudioGenerationOutcomeSchema.parse(saved).status).toBe("saved");

    const generated = generatedWithoutPersistence();
    const parsed = mediaStudioGenerationOutcomeSchema.parse(generated);
    expect(parsed.status).toBe("generatedWithoutPersistence");
    if (parsed.status === "generatedWithoutPersistence") {
      expect(generatedAssetPreviewUrl(parsed.result.images[0]!))
        .toBe("data:image/png;base64,aW1hZ2U=");
    }
    expect(() => mediaStudioGenerationOutcomeSchema.parse({
      ...generated,
      result: {
        ...generated.result,
        images: [{
          ...generated.result.images[0],
          url: "https://cdn.example/image.png",
        }],
      },
    })).toThrow();
  });

  it("allows only opaque reference IDs in generation requests", () => {
    const request = generationRequest();
    expect(mediaStudioGenerationRequestSchema.parse(request).referenceImageIds)
      .toEqual([REFERENCE_ID]);
    expect(() => mediaStudioGenerationRequestSchema.parse({
      ...request,
      referenceImageIds: [REFERENCE_ID, REFERENCE_ID],
    })).toThrow();
    expect(() => mediaStudioGenerationRequestSchema.parse({
      ...request,
      referenceImagePaths: ["C:\\Users\\Alice\\reference.png"],
    })).toThrow();
  });

  it("accepts only exact video history previews and rejects paths or media bytes", () => {
    const page = videoHistoryPage();
    expect(mediaStudioVideoHistoryPageSchema.parse(page).items[0]?.id).toBe(RECORD_ID);
    expect(() => mediaStudioVideoHistoryPageSchema.parse({
      ...page,
      items: [{ ...page.items[0], previewUrl: `chatos-media://localhost/${SESSION_ID}/${PRESENTATION_ID}/${CONTENT_TOKEN}` }],
    })).toThrow();
    expect(() => mediaStudioVideoHistoryPageSchema.parse({
      ...page,
      items: [{ ...page.items[0], absolutePath: "C:\\Users\\Alice\\video.mp4" }],
    })).toThrow();
    expect(() => mediaStudioVideoHistoryPageSchema.parse({
      ...page,
      items: [{ ...page.items[0], base64Data: "dmlkZW8=" }],
    })).toThrow();
  });

  it("keeps video inputs opaque and enforces the MiniMax request envelope", () => {
    const audio = {
      id: `video_audio_${"c".repeat(32)}`,
      name: "reference.m4a",
      mimeType: "audio/mp4",
      byteSize: 4_096,
    };
    expect(mediaStudioVideoAudioDraftSchema.parse(audio)).toEqual(audio);
    expect(() => mediaStudioVideoAudioDraftSchema.parse({ ...audio, path: "/tmp/reference.m4a" }))
      .toThrow();

    const request = videoGenerationRequest();
    expect(mediaStudioVideoGenerationRequestSchema.parse(request).seconds).toBe(6);
    expect(() => mediaStudioVideoGenerationRequestSchema.parse({ ...request, seconds: 16 })).toThrow();
    expect(() => mediaStudioVideoGenerationRequestSchema.parse({ ...request, size: "1080P" })).toThrow();
    expect(() => mediaStudioVideoGenerationRequestSchema.parse({ ...request, inputImagePath: "C:\\first.png" }))
      .toThrow();
  });

  it("parses strict video job snapshots including provider unknown state", () => {
    const snapshot = videoJobSnapshot();
    expect(mediaStudioVideoJobSnapshotSchema.parse(snapshot).status).toBe("unknown");
    expect(() => mediaStudioVideoJobSnapshotSchema.parse({ ...snapshot, apiKey: "secret" })).toThrow();
    expect(() => mediaStudioVideoJobSnapshotSchema.parse({
      ...snapshot,
      status: "failed",
      terminal: true,
      errorMessage: null,
    })).toThrow();
  });
});

function historyPage() {
  return {
    items: [{
      id: RECORD_ID,
      prompt: "A quiet lake",
      modelName: "gpt-image-1",
      createdAtUnixMs: 1_700_000_000_000,
      images: [{
        id: "generated-image-1",
        mimeType: "image/png",
        contentToken: CONTENT_TOKEN,
        revisedPrompt: null,
        previewUrl: `chatos-media://localhost/${SESSION_ID}/${RECORD_ID}/${CONTENT_TOKEN}`,
      }],
    }],
    nextCursor: { modifiedAtUnixMs: 1_700_000_000_001, recordId: RECORD_ID },
    unreadableCount: 0,
  };
}

function generationRequest() {
  return {
    presentationId: PRESENTATION_ID,
    modelConfigId: "image-model",
    prompt: "A quiet lake",
    size: "1024x1024",
    count: 1,
    referenceImageIds: [REFERENCE_ID],
    clientRequestId: "request-1",
  };
}

function generatedWithoutPersistence() {
  return {
    status: "generatedWithoutPersistence",
    result: {
      id: "generation-1",
      modelConfigId: "image-model",
      modelName: "gpt-image-1",
      createdAtUnixMs: 1_700_000_000_000,
      images: [{
        id: "generated-image-1",
        mimeType: "image/png",
        base64Data: "aW1hZ2U=",
        url: null,
        revisedPrompt: null,
      }],
      clientRequestId: "request-1",
    },
    persistenceErrorMessage: "图片已生成，但未能保存到历史记录。",
  };
}

function videoHistoryPage() {
  return {
    items: [{
      id: RECORD_ID,
      prompt: "A cinematic lake",
      modelName: "MiniMax-H3",
      createdAtUnixMs: 1_700_000_000_000,
      mimeType: "video/mp4",
      contentToken: CONTENT_TOKEN,
      previewUrl: `chatos-media://localhost/${SESSION_ID}/${RECORD_ID}/${CONTENT_TOKEN}`,
    }],
    nextCursor: null,
    unreadableCount: 0,
  };
}

function videoGenerationRequest() {
  return {
    presentationId: PRESENTATION_ID,
    modelConfigId: "video-model",
    prompt: "A cinematic lake",
    size: "768P",
    seconds: 6,
    ratio: "16:9",
    inputImageId: REFERENCE_ID,
    referenceAudioId: `video_audio_${"c".repeat(32)}`,
    clientRequestId: "video-request-1",
  };
}

function videoJobSnapshot() {
  return {
    jobId: "55555555-5555-4555-8555-555555555555",
    clientRequestId: "video-request-1",
    status: "unknown",
    percent: null,
    providerJobId: "provider-job-1",
    result: null,
    errorMessage: null,
    terminal: false,
    revision: 1,
  };
}
