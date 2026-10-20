import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriMediaStudioAdapter } from "./TauriMediaStudioAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const OWNER_ID = "alice";
const SESSION_ID = "11111111-1111-4111-8111-111111111111";
const RECORD_ID = "22222222-2222-4222-8222-222222222222";
const PRESENTATION_ID = "33333333-3333-4333-8333-333333333333";
const REFERENCE_TOKEN = "44444444-4444-4444-8444-444444444444";
const CONTENT_TOKEN = "a".repeat(32);
const REFERENCE_ID = `reference_${"b".repeat(32)}`;

describe("TauriMediaStudioAdapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("parses the model catalog and rejects native secrets", async () => {
    const model = {
      id: "image-model",
      name: "Image Model",
      provider: "openai-compatible",
      modelName: "gpt-image-1",
      enabled: true,
      taskEnabled: true,
      hasApiKey: true,
    };
    vi.mocked(invoke).mockResolvedValueOnce([model]);
    await expect(new TauriMediaStudioAdapter().listModels(OWNER_ID)).resolves.toEqual([model]);
    expect(invoke).toHaveBeenCalledWith("list_media_studio_models", { ownerUserId: OWNER_ID });

    vi.mocked(invoke).mockResolvedValueOnce([{ ...model, apiKey: "secret" }]);
    await expect(new TauriMediaStudioAdapter().listModels(OWNER_ID)).rejects.toThrow();
  });

  it("loads a bounded page with explicit nullable native arguments", async () => {
    vi.mocked(invoke).mockResolvedValue(historyPage());
    await new TauriMediaStudioAdapter().loadImageHistory({ ownerUserId: OWNER_ID });
    expect(invoke).toHaveBeenCalledWith("load_media_studio_image_history", {
      ownerUserId: OWNER_ID,
      after: null,
      limit: null,
    });
  });

  it("passes only opaque history identities into the reference vault", async () => {
    vi.mocked(invoke).mockResolvedValue(referenceImport());
    const request = {
      ownerUserId: OWNER_ID,
      presentationId: PRESENTATION_ID,
      assets: [{ recordId: RECORD_ID, contentToken: CONTENT_TOKEN }],
    };
    await new TauriMediaStudioAdapter().stageHistoryReferences(request);
    expect(invoke).toHaveBeenCalledWith("stage_media_studio_history_references", { input: request });
  });

  it("rejects a reference preview bound to a different presentation", async () => {
    vi.mocked(invoke).mockResolvedValue({
      ...referenceImport(),
      references: [{
        ...referenceImport().references[0],
        previewUrl: `http://chatos-media.localhost/${SESSION_ID}/reference/${RECORD_ID}/${REFERENCE_TOKEN}`,
      }],
    });
    await expect(new TauriMediaStudioAdapter().pickReferenceImages({
      ownerUserId: OWNER_ID,
      presentationId: PRESENTATION_ID,
    })).rejects.toThrow("当前图片创作页面不匹配");
  });

  it("freezes the generation envelope and verifies non-persisted request identity", async () => {
    vi.mocked(invoke).mockResolvedValue(generatedWithoutPersistence());
    const request = generationRequest();
    await new TauriMediaStudioAdapter().generateImage({ ownerUserId: OWNER_ID, request });
    expect(invoke).toHaveBeenCalledWith("generate_media_studio_image", {
      ownerUserId: OWNER_ID,
      request,
    });

    vi.mocked(invoke).mockResolvedValue({
      ...generatedWithoutPersistence(),
      result: { ...generatedWithoutPersistence().result, clientRequestId: "request-2" },
    });
    await expect(new TauriMediaStudioAdapter().generateImage({ ownerUserId: OWNER_ID, request }))
      .rejects.toThrow("当前请求不匹配");
  });

  it("uses typed inputs for reference and presentation cleanup", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const adapter = new TauriMediaStudioAdapter();
    await adapter.discardReferenceImages({
      ownerUserId: OWNER_ID,
      presentationId: PRESENTATION_ID,
      referenceIds: [REFERENCE_ID],
    });
    await adapter.discardPresentation({ ownerUserId: OWNER_ID, presentationId: PRESENTATION_ID });
    expect(invoke).toHaveBeenNthCalledWith(1, "discard_media_studio_reference_images", {
      input: {
        ownerUserId: OWNER_ID,
        presentationId: PRESENTATION_ID,
        referenceIds: [REFERENCE_ID],
      },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "discard_media_studio_presentation", {
      input: { ownerUserId: OWNER_ID, presentationId: PRESENTATION_ID },
    });
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
        previewUrl: `http://chatos-media.localhost/${SESSION_ID}/${RECORD_ID}/${CONTENT_TOKEN}`,
      }],
    }],
    nextCursor: null,
    unreadableCount: 0,
  };
}

function referenceImport() {
  return {
    references: [{
      id: REFERENCE_ID,
      name: "reference.png",
      mimeType: "image/png",
      byteSize: 1_024,
      previewUrl: `http://chatos-media.localhost/${SESSION_ID}/reference/${PRESENTATION_ID}/${REFERENCE_TOKEN}`,
    }],
    errors: [],
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
