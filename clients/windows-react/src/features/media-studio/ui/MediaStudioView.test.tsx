import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useMediaStudioStore } from "../application/mediaStudioStore";
import type {
  MediaStudioHistoryItem,
  MediaStudioModel,
} from "../domain/mediaStudio";
import type { MediaStudioPort } from "../ports/MediaStudioPort";
import { MediaStudioView } from "./MediaStudioView";

const SESSION_ID = "11111111-1111-4111-8111-111111111111";
const RECORD_ID = "22222222-2222-4222-8222-222222222222";
const GENERATED_ID = "33333333-3333-4333-8333-333333333333";
const CONTENT_TOKEN = "a".repeat(32);
const REFERENCE_ID = `reference_${"b".repeat(32)}`;

beforeEach(() => useMediaStudioStore.getState().reset(port()));
afterEach(cleanup);

describe("MediaStudioView", () => {
  it("matches the macOS image workspace and generates from a frozen form", async () => {
    const adapter = port({
      loadImageHistory: vi.fn(async () => ({ items: [], nextCursor: null, unreadableCount: 0 })),
      generateImage: vi.fn(async () => ({
        status: "saved" as const,
        item: historyItem(GENERATED_ID, "极简产品广告海报"),
      })),
    });
    render(<MediaStudioView ownerUserId="alice" port={adapter} />);

    expect(await screen.findByRole("heading", { name: "创作画布" })).toBeVisible();
    expect(screen.getByText("独立于聊天的图片与视频工作台")).toBeVisible();
    expect(screen.getByRole("button", { name: "视频" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "产品海报" }));
    expect(screen.getByRole("textbox", { name: "提示词" }))
      .toHaveValue("极简产品广告海报，干净背景，工作室灯光，高级材质与留白");

    fireEvent.click(screen.getByRole("button", { name: "生成图片" }));
    await waitFor(() => expect(adapter.generateImage).toHaveBeenCalledWith({
      ownerUserId: "alice",
      request: expect.objectContaining({
        modelConfigId: "image-model",
        prompt: expect.stringContaining("极简产品广告海报"),
        size: "1024x1024",
        count: 1,
        referenceImageIds: [],
      }),
    }));
    expect(await screen.findByText("已完成")).toBeVisible();
  });

  it("selects only current history assets through the native reference vault", async () => {
    const adapter = port({
      loadImageHistory: vi.fn(async () => ({
        items: [historyItem(RECORD_ID, "quiet lake")],
        nextCursor: null,
        unreadableCount: 0,
      })),
      stageHistoryReferences: vi.fn(async () => ({
        references: [{
          id: REFERENCE_ID,
          name: "generated.png",
          mimeType: "image/png" as const,
          byteSize: 1_024,
          previewUrl: null,
        }],
        errors: [],
      })),
    });
    render(<MediaStudioView ownerUserId="alice" port={adapter} />);

    const choose = await screen.findByRole("button", { name: /从生成记录选择多张/ });
    fireEvent.click(choose);
    const dialog = screen.getByRole("dialog", { name: "从生成记录选择参考图" });
    fireEvent.click(dialog.querySelector(".media-reference-sheet__grid button")!);
    fireEvent.click(screen.getByRole("button", { name: "添加参考图" }));

    await waitFor(() => expect(adapter.stageHistoryReferences).toHaveBeenCalledWith({
      ownerUserId: "alice",
      presentationId: expect.any(String),
      assets: [{ recordId: RECORD_ID, contentToken: CONTENT_TOKEN }],
    }));
    expect(screen.getByText("1 / 8")).toBeVisible();
  });

  it("keeps non-persisted generated media visible and discards the page on unmount", async () => {
    const adapter = port({
      generateImage: vi.fn(async ({ request }) => ({
        status: "generatedWithoutPersistence" as const,
        result: {
          id: "transient-1",
          modelConfigId: request.modelConfigId,
          modelName: "gpt-image-1",
          createdAtUnixMs: 300,
          images: [{
            id: "asset-transient",
            mimeType: "image/png" as const,
            base64Data: "aW1hZ2U=",
            url: null,
            revisedPrompt: null,
          }],
          clientRequestId: request.clientRequestId,
        },
        persistenceErrorMessage: "disk full",
      })),
    });
    const rendered = render(<MediaStudioView ownerUserId="alice" port={adapter} />);
    await screen.findByRole("heading", { name: "创作画布" });
    fireEvent.change(screen.getByRole("textbox", { name: "提示词" }), { target: { value: "Poster" } });
    fireEvent.click(screen.getByRole("button", { name: "生成图片" }));

    expect(await screen.findByText(/disk full/)).toBeVisible();
    expect(screen.getByText("仅保留在当前页面，尚未写入本机创作记录")).toBeVisible();
    const presentationId = useMediaStudioStore.getState().presentationId;
    rendered.unmount();
    await waitFor(() => expect(adapter.discardPresentation).toHaveBeenCalledWith({
      ownerUserId: "alice",
      presentationId,
    }));
  });
});

function port(overrides: Partial<MediaStudioPort> = {}): MediaStudioPort {
  return {
    listModels: vi.fn(async () => [model()]),
    loadImageHistory: vi.fn(async () => ({ items: [], nextCursor: null, unreadableCount: 0 })),
    pickReferenceImages: vi.fn(async () => ({ references: [], errors: [] })),
    stageHistoryReferences: vi.fn(async () => ({ references: [], errors: [] })),
    discardReferenceImages: vi.fn(async () => undefined),
    discardPresentation: vi.fn(async () => undefined),
    generateImage: vi.fn(async () => ({
      status: "saved" as const,
      item: historyItem(GENERATED_ID, "generated"),
    })),
    ...overrides,
  };
}

function model(): MediaStudioModel {
  return {
    id: "image-model",
    name: "Image Model",
    provider: "openai-compatible",
    modelName: "gpt-image-1",
    enabled: true,
    taskEnabled: true,
    hasApiKey: true,
  };
}

function historyItem(id: string, prompt: string): MediaStudioHistoryItem {
  return {
    id,
    prompt,
    modelName: "gpt-image-1",
    createdAtUnixMs: 200,
    images: [{
      id: `asset-${id}`,
      mimeType: "image/png",
      contentToken: CONTENT_TOKEN,
      revisedPrompt: null,
      previewUrl: `http://chatos-media.localhost/${SESSION_ID}/${id}/${CONTENT_TOKEN}`,
    }],
  };
}
