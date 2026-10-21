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
    expect(screen.getByRole("button", { name: "视频" })).toBeEnabled();
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

  it("recreates the macOS video workspace and submits only frozen typed inputs", async () => {
    const video = videoHistoryItem();
    const adapter = port({
      listModels: vi.fn(async () => [model(), videoModel()]),
      pickVideoReferenceAudio: vi.fn(async () => ({
        id: `video_audio_${"c".repeat(32)}`,
        name: "reference.m4a",
        mimeType: "audio/mp4" as const,
        byteSize: 4_096,
      })),
      startVideoGeneration: vi.fn(async ({ request }) => ({
        jobId: "44444444-4444-4444-8444-444444444444",
        clientRequestId: request.clientRequestId,
        status: "completed" as const,
        percent: 100,
        providerJobId: "provider-job",
        result: video,
        errorMessage: null,
        terminal: true,
        revision: 2,
      })),
    });
    render(<MediaStudioView ownerUserId="alice" port={adapter} />);

    const videoTab = await screen.findByRole("button", { name: "视频" });
    fireEvent.click(videoTab);
    expect(screen.getByRole("heading", { name: "视频画布" })).toBeVisible();
    expect(screen.getByRole("complementary", { name: "视频设置" })).toHaveClass("media-studio-card", "media-settings");
    expect(screen.getByRole("button", { name: "剧情模式" })).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: /添加参考音频/ }));
    expect(await screen.findByText("reference.m4a")).toBeVisible();
    fireEvent.change(screen.getByRole("textbox", { name: "视频提示词" }), {
      target: { value: "Cinematic clouds moving over a lake" },
    });
    fireEvent.change(screen.getByRole("combobox", { name: "视频分辨率" }), {
      target: { value: "2K" },
    });
    fireEvent.change(screen.getByRole("combobox", { name: "视频时长" }), {
      target: { value: "8" },
    });
    fireEvent.click(screen.getByRole("button", { name: "生成视频" }));

    await waitFor(() => expect(adapter.startVideoGeneration).toHaveBeenCalledWith({
      ownerUserId: "alice",
      request: expect.objectContaining({
        modelConfigId: "video-model",
        prompt: "Cinematic clouds moving over a lake",
        size: "2K",
        seconds: 8,
        ratio: "16:9",
        inputImageId: null,
        referenceAudioId: `video_audio_${"c".repeat(32)}`,
      }),
    }));
    expect(await screen.findByText("MiniMax-H3 · 已保存到本机")).toBeVisible();
    expect(document.querySelector("video")).toHaveAttribute("src", video.previewUrl);
  });
});

function port(overrides: Partial<MediaStudioPort> = {}): MediaStudioPort {
  return {
    listModels: vi.fn(async () => [model()]),
    loadImageHistory: vi.fn(async () => ({ items: [], nextCursor: null, unreadableCount: 0 })),
    loadVideoHistory: vi.fn(async () => ({ items: [], nextCursor: null, unreadableCount: 0 })),
    pickReferenceImages: vi.fn(async () => ({ references: [], errors: [] })),
    stageHistoryReferences: vi.fn(async () => ({ references: [], errors: [] })),
    discardReferenceImages: vi.fn(async () => undefined),
    pickVideoReferenceAudio: vi.fn(async () => null),
    discardVideoReferenceAudio: vi.fn(async () => undefined),
    discardPresentation: vi.fn(async () => undefined),
    generateImage: vi.fn(async () => ({
      status: "saved" as const,
      item: historyItem(GENERATED_ID, "generated"),
    })),
    startVideoGeneration: vi.fn(async ({ request }) => ({
      jobId: "44444444-4444-4444-8444-444444444444",
      clientRequestId: request.clientRequestId,
      status: "queued" as const,
      percent: null,
      providerJobId: "provider-job",
      result: null,
      errorMessage: null,
      terminal: false,
      revision: 1,
    })),
    pollVideoGeneration: vi.fn(async () => ({
      jobId: "44444444-4444-4444-8444-444444444444",
      clientRequestId: "request",
      status: "queued" as const,
      percent: null,
      providerJobId: "provider-job",
      result: null,
      errorMessage: null,
      terminal: false,
      revision: 2,
    })),
    cancelVideoGeneration: vi.fn(async () => undefined),
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

function videoModel(): MediaStudioModel {
  return {
    id: "video-model",
    name: "MiniMax Video",
    provider: "openai-compatible",
    modelName: "MiniMax-H3",
    enabled: true,
    taskEnabled: false,
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

function videoHistoryItem() {
  return {
    id: "55555555-5555-4555-8555-555555555555",
    prompt: "Cinematic clouds moving over a lake",
    modelName: "MiniMax-H3",
    createdAtUnixMs: 300,
    mimeType: "video/mp4" as const,
    contentToken: "d".repeat(32),
    previewUrl: `http://chatos-media.localhost/${SESSION_ID}/55555555-5555-4555-8555-555555555555/${"d".repeat(32)}`,
  };
}
