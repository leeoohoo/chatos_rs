import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { MediaStudioPort } from "../../media-studio/ports/MediaStudioPort";
import type { StoryPlanningEvent, StoryProject } from "../domain/storyStudio";
import type { StoryStudioPort } from "../ports/StoryStudioPort";
import { useStoryStudioStore } from "../application/storyStudioStore";
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

  it("shows live planning progress without applying the event draft", async () => {
    const canonical = { ...project(), source: "雨夜里，陌生人敲响了门。" };
    let planningListener: ((event: StoryPlanningEvent) => void) | undefined;
    const unlisten = vi.fn();
    const storyPort = port({
      watchPlanning: vi.fn(async (_request, listener) => {
        planningListener = listener;
        return unlisten;
      }),
      loadProjects: vi.fn(async () => ({ items: [canonical], nextCursor: null, unreadableCount: 0 })),
      startPlanning: vi.fn(() => new Promise<StoryProject | null>(() => undefined)),
    });
    const view = render(<StoryStudioView mediaPort={mediaPort()} ownerUserId="alice" port={storyPort} />);

    await screen.findByRole("heading", { name: "雨夜来客" });
    fireEvent.click(screen.getByRole("button", { name: "生成剧情规划" }));
    expect(await screen.findByText("正在准备规划")).toBeVisible();
    expect(planningListener).toBeDefined();

    act(() => planningListener?.({
      presentationId: usePresentationId(storyPort),
      run: {
        ...planningRun(),
        draft: { ...canonical, title: "不应提前应用的草稿" },
        status: "running",
        modelCalls: 3,
        updatedAtUnixMs: Date.now() + 1,
      },
    }));

    expect(screen.getByText("规划 Agent 正在工作")).toBeVisible();
    expect(screen.getByText("剧情拆解 · 已调用模型 3 次")).toBeVisible();
    expect(screen.getByRole("heading", { name: "雨夜来客" })).toBeVisible();
    expect(screen.queryByRole("heading", { name: "不应提前应用的草稿" })).not.toBeInTheDocument();

    view.unmount();
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("offers the macOS recovery actions for a matching interrupted draft", async () => {
    const canonical = { ...project(), source: "雨夜里，陌生人敲响了门。" };
    const interrupted = {
      ...planningRun(),
      draft: { ...canonical, summary: "已经完成一部分剧情拆解" },
      modelCalls: 8,
      stopReason: "应用退出前已保存当前步骤。",
    };
    const storyPort = port({
      loadProjects: vi.fn(async () => ({ items: [canonical], nextCursor: null, unreadableCount: 0 })),
      loadPlanningHistory: vi.fn(async () => ({
        runs: [interrupted],
        nextCursor: null,
        unreadableCount: 0,
        recoveredProject: null,
      })),
      resumePlanning: vi.fn(async () => canonical),
    });
    render(<StoryStudioView mediaPort={mediaPort()} ownerUserId="alice" port={storyPort} />);

    expect(await screen.findByText("上次规划已中断，草稿已安全保存")).toBeVisible();
    expect(screen.getByText("应用退出前已保存当前步骤。")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "继续规划" }));

    await waitFor(() => expect(storyPort.resumePlanning).toHaveBeenCalledWith(expect.objectContaining({
      ownerUserId: "alice",
      projectId: canonical.id,
      runId: interrupted.id,
    })));
  });

  it("keeps the native picker and imported bytes behind the Story adapter", async () => {
    const canonical = { ...project(), source: "雨夜里，陌生人敲响了门。" };
    const imported = { ...canonical, updatedAtUnixMs: 3 };
    const storyPort = port({
      loadProjects: vi.fn(async () => ({ items: [canonical], nextCursor: null, unreadableCount: 0 })),
      importLocalImage: vi.fn(async () => imported),
    });
    render(<StoryStudioView mediaPort={mediaPort()} ownerUserId="alice" port={storyPort} />);
    await screen.findByRole("heading", { name: "雨夜来客" });

    await act(async () => useStoryStudioStore.getState().importLocalImage(storyPort, {
      kind: "resource",
      resourceId: "character-a",
    }));

    expect(storyPort.importLocalImage).toHaveBeenCalledWith(expect.objectContaining({
      ownerUserId: "alice",
      projectId: canonical.id,
      baseFingerprint: canonical.baseFingerprint,
      target: { kind: "resource", resourceId: "character-a" },
    }));
    expect(useStoryStudioStore.getState().projects[0]?.updatedAtUnixMs).toBe(3);
  });
});

function usePresentationId(storyPort: StoryStudioPort) {
  const watchPlanning = storyPort.watchPlanning as ReturnType<typeof vi.fn>;
  return watchPlanning.mock.calls[0]?.[0].presentationId as string;
}

function port(overrides: Partial<StoryStudioPort> = {}): StoryStudioPort {
  return {
    watchPlanning: vi.fn(async () => () => undefined),
    loadPlanningHistory: vi.fn(async () => ({ runs: [], nextCursor: null, unreadableCount: 0, recoveredProject: null })),
    resumePlanning: vi.fn(async () => project()),
    abandonPlanning: vi.fn(async () => planningRun()),
    importLocalImage: vi.fn(async () => project()),
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
    updateVideoGuidance: vi.fn(async () => project()),
    archiveCompletedVideo: vi.fn(async () => project()),
    discardPresentation: vi.fn(async () => undefined),
    ...overrides,
  };
}

function planningRun() {
  return {
    id: "22222222-2222-4222-8222-222222222222",
    ownerUserId: "alice",
    projectId: project().id,
    stage: "outline" as const,
    targetIds: [],
    baseProjectFingerprint: "a".repeat(64),
    draft: project(),
    events: [],
    status: "paused" as const,
    stopReason: null,
    modelCalls: 1,
    usesCloudMemory: false,
    applied: false,
    abandonedAtUnixMs: null,
    updatedAtUnixMs: 2,
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
