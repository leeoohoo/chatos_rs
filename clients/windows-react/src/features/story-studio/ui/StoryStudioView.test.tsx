import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { MediaStudioPort } from "../../media-studio/ports/MediaStudioPort";
import type { StoryProject } from "../domain/storyStudio";
import type { StoryStudioPort } from "../ports/StoryStudioPort";
import { StoryStudioView } from "./StoryStudioView";

afterEach(cleanup);

describe("StoryStudioView", () => {
  it("creates a native project and saves source through typed intents", async () => {
    const created = project();
    const storyPort = port({
      createProject: vi.fn(async () => created),
      updateSource: vi.fn(async (request) => ({
        ...created,
        source: request.source,
        style: request.style,
        ratio: request.ratio,
        baseFingerprint: "b".repeat(64),
      })),
    });
    render(<StoryStudioView mediaPort={mediaPort()} ownerUserId="alice" port={storyPort} />);

    fireEvent.click((await screen.findAllByRole("button", { name: "新建剧情项目" }))[0]!);
    fireEvent.change(screen.getByLabelText("项目名称"), { target: { value: "雨夜来客" } });
    fireEvent.click(screen.getByRole("button", { name: "创建项目" }));
    expect(await screen.findByRole("heading", { name: "雨夜来客" })).toBeVisible();
    expect(storyPort.createProject).toHaveBeenCalledWith(expect.objectContaining({
      ownerUserId: "alice",
      presentationId: expect.any(String),
      textModelId: "text-model",
      imageModelId: "image-model",
      videoModelId: "video-model",
    }));

    fireEvent.change(screen.getByRole("textbox", { name: "故事原文" }), {
      target: { value: "雨夜里，陌生人敲响了门。" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存原文" }));
    await waitFor(() => expect(storyPort.updateSource).toHaveBeenCalledWith(expect.objectContaining({
      projectId: created.id,
      baseFingerprint: "a".repeat(64),
      source: "雨夜里，陌生人敲响了门。",
    })));
  });
});

function port(overrides: Partial<StoryStudioPort> = {}): StoryStudioPort {
  return {
    loadProjects: vi.fn(async () => ({ items: [], nextCursor: null, unreadableCount: 0 })),
    loadProject: vi.fn(async () => project()),
    createProject: vi.fn(async () => project()),
    updateSource: vi.fn(async () => project()),
    startPlanning: vi.fn(async () => project()),
    generateAssetImage: vi.fn(async () => project()),
    generateFrameImage: vi.fn(async () => project()),
    confirmLatestResourceImages: vi.fn(async () => project()),
    confirmLatestFrames: vi.fn(async () => project()),
    startVideo: vi.fn(async () => ({ project: project(), status: "queued", percent: null, terminal: false })),
    pollVideo: vi.fn(async () => ({ project: project(), status: "completed", percent: 100, terminal: true })),
    discardPresentation: vi.fn(async () => undefined),
    ...overrides,
  };
}

function mediaPort() {
  return {
    listModels: vi.fn(async () => [
      model("text-model", "Text", "gpt-5"),
      model("image-model", "Image", "gpt-image-1"),
      model("video-model", "Video", "MiniMax-H3"),
    ]),
  } as unknown as MediaStudioPort;
}

function model(id: string, name: string, modelName: string) {
  return {
    id,
    name,
    provider: "openai-compatible",
    modelName,
    enabled: true,
    taskEnabled: true,
    hasApiKey: true,
  };
}

function project(): StoryProject {
  return {
    id: "11111111-1111-4111-8111-111111111111",
    baseFingerprint: "a".repeat(64),
    version: 2,
    title: "雨夜来客",
    description: "",
    models: {
      textModelId: "text-model",
      imageModelId: "image-model",
      videoModelId: "video-model",
      supportedVideoDurations: [4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    },
    source: "",
    style: "自然光，电影感",
    ratio: "16:9",
    summary: "",
    characters: [],
    scenes: [],
    props: [],
    segments: [],
    relations: [],
    createdAtUnixMs: 1,
    updatedAtUnixMs: 1,
  };
}
