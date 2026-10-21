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

    await screen.findByRole("heading", { name: "雨夜来客", level: 2 });
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

  it("browses complete paged planning history with native recovery actions", async () => {
    const canonical = { ...project(), source: "雨夜里，陌生人敲响了门。" };
    const latest = {
      ...planningRun(),
      draft: { ...plannedProject(), id: canonical.id },
      events: [{
        id: "44444444-4444-4444-8444-444444444444",
        dateUnixMs: 2,
        kind: "checkpoint",
        detail: "已保存人物与场景草稿",
        modelCalls: 2,
      }],
      modelCalls: 2,
      updatedAtUnixMs: 20,
    };
    const earlier = {
      ...planningRun(),
      id: "33333333-3333-4333-8333-333333333333",
      draft: { ...canonical, summary: "更早的已应用规划" },
      status: "completed" as const,
      applied: true,
      updatedAtUnixMs: 10,
    };
    const cursor = { modifiedAtUnixMs: 20, recordId: latest.id };
    const loadPlanningHistory = vi.fn(async (request: Parameters<StoryStudioPort["loadPlanningHistory"]>[0]) => request.after
      ? { runs: [earlier], nextCursor: null, unreadableCount: 1, recoveredProject: null }
      : { runs: [latest], nextCursor: cursor, unreadableCount: 0, recoveredProject: null });
    const storyPort = port({
      loadProjects: vi.fn(async () => ({ items: [canonical], nextCursor: null, unreadableCount: 0 })),
      loadPlanningHistory,
    });
    render(<StoryStudioView mediaPort={mediaPort()} ownerUserId="alice" port={storyPort} />);

    fireEvent.click(await screen.findByRole("button", { name: "查看规划历史" }));
    expect(screen.getByRole("heading", { name: "规划草稿与运行记录" })).toBeVisible();
    expect(screen.getByText("已保存人物与场景草稿")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "恢复这一条记录" })).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "加载更早记录" }));
    await waitFor(() => expect(loadPlanningHistory).toHaveBeenCalledWith(expect.objectContaining({
      projectId: canonical.id,
      after: cursor,
      limit: 20,
    })));
    expect(await screen.findByText("规划完成 · 已应用")).toBeVisible();
    expect(screen.getByText("1 条损坏记录已安全隔离，原文件保留且未用于恢复。")).toBeVisible();
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

  it("imports an authorized image-history reference without exposing provider data", async () => {
    const canonical = { ...project(), source: "雨夜里，陌生人敲响了门。" };
    const storyPort = port({
      loadProjects: vi.fn(async () => ({ items: [canonical], nextCursor: null, unreadableCount: 0 })),
      importHistoryImage: vi.fn(async () => ({ ...canonical, updatedAtUnixMs: 4 })),
    });
    const historyPort = mediaPort();
    historyPort.loadImageHistory = vi.fn(async () => ({
      items: [{
        id: "33333333-3333-4333-8333-333333333333",
        prompt: "雨夜人物",
        modelName: "gpt-image-1",
        createdAtUnixMs: 3,
        images: [{
          id: "image-a",
          mimeType: "image/png" as const,
          contentToken: "a".repeat(32),
          revisedPrompt: null,
          previewUrl: null,
        }],
      }],
      nextCursor: null,
      unreadableCount: 0,
    }));
    render(<StoryStudioView mediaPort={historyPort} ownerUserId="alice" port={storyPort} />);
    await screen.findByRole("heading", { name: "雨夜来客" });

    await act(async () => useStoryStudioStore.getState().loadStoryImageHistory(historyPort));
    expect(useStoryStudioStore.getState().storyImageHistory).toHaveLength(1);
    await act(async () => useStoryStudioStore.getState().importHistoryImage(
      storyPort,
      { kind: "frame", segmentId: "segment-a", frameRole: "first" },
      "33333333-3333-4333-8333-333333333333",
      "a".repeat(32),
    ));

    expect(storyPort.importHistoryImage).toHaveBeenCalledWith(expect.objectContaining({
      ownerUserId: "alice",
      recordId: "33333333-3333-4333-8333-333333333333",
      contentToken: "a".repeat(32),
      target: { kind: "frame", segmentId: "segment-a", frameRole: "first" },
    }));
  });

  it("sends duration as a narrow native intent", async () => {
    const canonical = { ...project(), source: "雨夜里，陌生人敲响了门。" };
    const storyPort = port({
      loadProjects: vi.fn(async () => ({ items: [canonical], nextCursor: null, unreadableCount: 0 })),
      updateSegmentDuration: vi.fn(async () => canonical),
    });
    render(<StoryStudioView mediaPort={mediaPort()} ownerUserId="alice" port={storyPort} />);
    await screen.findByRole("heading", { name: "雨夜来客" });

    await act(async () => useStoryStudioStore.getState().updateSegmentDuration(
      storyPort,
      "segment-a",
      10,
    ));

    expect(storyPort.updateSegmentDuration).toHaveBeenCalledWith(expect.objectContaining({
      projectId: canonical.id,
      baseFingerprint: canonical.baseFingerprint,
      segmentId: "segment-a",
      seconds: 10,
    }));
  });

  it("only unlocks an ambiguous image attempt after explicit verification", async () => {
    const canonical = { ...project(), source: "雨夜里，陌生人敲响了门。" };
    const storyPort = port({
      loadProjects: vi.fn(async () => ({ items: [canonical], nextCursor: null, unreadableCount: 0 })),
      allowImageRetry: vi.fn(async () => canonical),
    });
    render(<StoryStudioView mediaPort={mediaPort()} ownerUserId="alice" port={storyPort} />);
    await screen.findByRole("heading", { name: "雨夜来客" });

    await act(async () => useStoryStudioStore.getState().allowImageRetry(storyPort, {
      kind: "frame",
      segmentId: "segment-a",
      frameRole: "last",
    }));

    expect(storyPort.allowImageRetry).toHaveBeenCalledWith(expect.objectContaining({
      projectId: canonical.id,
      baseFingerprint: canonical.baseFingerprint,
      target: { kind: "frame", segmentId: "segment-a", frameRole: "last" },
    }));
  });

  it("archives a verified ambiguous video attempt through a narrow intent", async () => {
    const canonical = { ...project(), source: "雨夜里，陌生人敲响了门。" };
    const storyPort = port({
      loadProjects: vi.fn(async () => ({ items: [canonical], nextCursor: null, unreadableCount: 0 })),
      allowVideoRetry: vi.fn(async () => canonical),
    });
    render(<StoryStudioView mediaPort={mediaPort()} ownerUserId="alice" port={storyPort} />);
    await screen.findByRole("heading", { name: "雨夜来客" });

    await act(async () => useStoryStudioStore.getState().allowVideoRetry(storyPort, "segment-a"));

    expect(storyPort.allowVideoRetry).toHaveBeenCalledWith(expect.objectContaining({
      projectId: canonical.id,
      baseFingerprint: canonical.baseFingerprint,
      segmentId: "segment-a",
    }));
  });

  it("renders the native macOS-aligned media batch preview without starting work", async () => {
    const canonical = plannedProject();
    const storyPort = port({
      loadProjects: vi.fn(async () => ({ items: [canonical], nextCursor: null, unreadableCount: 0 })),
      previewMediaBatch: vi.fn(async () => ({
        eligibleTargetIds: ["segment-a"],
        preparation: {
          previewId: "44444444-4444-4444-8444-444444444444",
          projectId: canonical.id,
          projectFingerprint: canonical.baseFingerprint!,
          kind: "pipeline" as const,
          eligibleTargetIds: ["segment-a"],
          requestedTargetIds: ["segment-a"],
          steps: [
            { id: "assets:prop-a", kind: "assets" as const, targetId: "prop-a" },
            { id: "frames:segment-a", kind: "frames" as const, targetId: "segment-a" },
            { id: "videos:segment-a", kind: "videos" as const, targetId: "segment-a" },
          ],
          executionMode: "persistentBatch" as const,
        },
        validationError: null,
      })),
      startMediaBatch: vi.fn(async (request: Parameters<StoryStudioPort["startMediaBatch"]>[0]) => ({
        id: request.preparation.previewId,
        ownerUserId: "alice",
        projectId: canonical.id,
        kind: "pipeline" as const,
        status: "ready" as const,
        steps: request.preparation.steps.map((step) => ({
          ...step,
          completed: false,
          progressStatus: null,
        })),
        events: [{
          id: "55555555-5555-4555-8555-555555555555",
          dateUnixMs: 10,
          detail: "用户已确认批量制作计划",
        }],
        consentAtUnixMs: 10,
        updatedAtUnixMs: 10,
        error: null,
      })),
    });
    render(<StoryStudioView mediaPort={mediaPort()} ownerUserId="alice" port={storyPort} />);
    await screen.findByRole("heading", { name: "雨夜来客", level: 2 });

    fireEvent.click(screen.getByRole("button", { name: "批量制作" }));

    expect(await screen.findByText("原生执行计划已验证")).toBeVisible();
    expect(screen.getByText("可恢复持久化批次")).toBeVisible();
    expect(screen.getByText("素材图片 · 钥匙")).toBeVisible();
    expect(storyPort.previewMediaBatch).toHaveBeenCalledWith(expect.objectContaining({
      projectId: canonical.id,
      kind: "pipeline",
      targetIds: undefined,
    }));
    fireEvent.click(screen.getByRole("button", { name: "确认并创建队列" }));
    expect(await screen.findByText("制作队列")).toBeVisible();
    expect(screen.getByText("已完成 0 / 3 个步骤 · 一键全流程")).toBeVisible();
    expect(storyPort.startMediaBatch).toHaveBeenCalledWith(expect.objectContaining({
      preparation: expect.objectContaining({ previewId: "44444444-4444-4444-8444-444444444444" }),
    }));
    fireEvent.click(screen.getByRole("button", { name: "查看历史" }));
    expect(screen.getByRole("heading", { name: "制作历史" })).toBeVisible();
    expect(screen.getAllByText("用户已确认批量制作计划").length).toBeGreaterThan(0);
  });

  it("routes a videos-only preview into the independent sequential queue", async () => {
    const canonical = plannedProject();
    const preparation = {
      previewId: "66666666-6666-4666-8666-666666666666",
      projectId: canonical.id,
      projectFingerprint: canonical.baseFingerprint!,
      kind: "videos" as const,
      eligibleTargetIds: ["segment-a"],
      requestedTargetIds: ["segment-a"],
      steps: [{ id: "videos:segment-a", kind: "videos" as const, targetId: "segment-a" }],
      executionMode: "directVideos" as const,
    };
    const storyPort = port({
      loadProjects: vi.fn(async () => ({ items: [canonical], nextCursor: null, unreadableCount: 0 })),
      previewMediaBatch: vi.fn(async (request) => request.kind === "videos"
        ? { eligibleTargetIds: ["segment-a"], preparation, validationError: null }
        : { eligibleTargetIds: [], preparation: null, validationError: null }),
      startDirectVideoQueue: vi.fn(async () => ({
        project: canonical,
        completedTargetIds: ["segment-a"],
      })),
    });
    render(<StoryStudioView mediaPort={mediaPort()} ownerUserId="alice" port={storyPort} />);
    await screen.findByRole("heading", { name: "雨夜来客", level: 2 });

    fireEvent.click(screen.getByRole("button", { name: "批量制作" }));
    fireEvent.click(screen.getByRole("tab", { name: "分段视频" }));

    expect(await screen.findByText("独立顺序视频队列")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "确认并顺序生成" }));
    await waitFor(() => expect(storyPort.startDirectVideoQueue).toHaveBeenCalledWith(
      expect.objectContaining({ preparation }),
    ));
    expect(storyPort.startMediaBatch).not.toHaveBeenCalled();
  });
});

function usePresentationId(storyPort: StoryStudioPort) {
  const watchPlanning = storyPort.watchPlanning as ReturnType<typeof vi.fn>;
  return watchPlanning.mock.calls[0]?.[0].presentationId as string;
}

function port(overrides: Partial<StoryStudioPort> = {}): StoryStudioPort {
  return {
    watchVideoGeneration: vi.fn(async () => () => undefined),
    watchMediaBatches: vi.fn(async () => () => undefined),
    watchPlanning: vi.fn(async () => () => undefined),
    loadPlanningHistory: vi.fn(async () => ({ runs: [], nextCursor: null, unreadableCount: 0, recoveredProject: null })),
    resumePlanning: vi.fn(async () => project()),
    abandonPlanning: vi.fn(async () => planningRun()),
    importLocalImage: vi.fn(async () => project()),
    importHistoryImage: vi.fn(async () => project()),
    updateSegmentDuration: vi.fn(async () => project()),
    allowImageRetry: vi.fn(async () => project()),
    allowVideoRetry: vi.fn(async () => project()),
    loadProjects: vi.fn(async () => ({ items: [], nextCursor: null, unreadableCount: 0 })),
    loadProject: vi.fn(async () => project()),
    previewMediaBatch: vi.fn(async () => { throw new Error("unused batch preview"); }),
    loadMediaBatchHistory: vi.fn(async () => ({ batches: [], nextCursor: null, unreadableCount: 0 })),
    startMediaBatch: vi.fn(async () => { throw new Error("unused batch start"); }),
    startDirectVideoQueue: vi.fn(async () => ({
      project: project(),
      completedTargetIds: ["segment-a"],
    })),
    resumeMediaBatch: vi.fn(() => new Promise<never>(() => undefined)),
    abandonMediaBatch: vi.fn(async () => { throw new Error("unused batch abandon"); }),
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

function mediaPort(): MediaStudioPort {
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

function plannedProject(): StoryProject {
  return {
    ...project(),
    source: "陌生人拿着钥匙走进雨夜。",
    props: [{
      id: "prop-a",
      name: "钥匙",
      description: "旧钥匙",
      imagePrompt: "一把黄铜旧钥匙",
      media: { images: [], confirmedImageId: null, generationAttemptId: null },
    }],
    segments: [{
      id: "segment-a",
      title: "雨夜来客",
      synopsis: "陌生人拿着钥匙走进雨夜。",
      kind: "story",
      seconds: 5,
      sourceRange: { start: 0, end: 12 },
      characterIds: [],
      sceneIds: [],
      propIds: ["prop-a"],
      detail: {
        firstFramePrompt: "雨夜中的陌生人",
        lastFramePrompt: null,
        shots: [{ start: 0, end: 5, prompt: "陌生人走近门口" }],
        continuityIn: "",
        continuityOut: "",
        audio: "雨声",
        constraints: "",
      },
      firstFrames: { images: [], confirmedImageId: null, generationAttemptId: null },
      userSelectedFirstFrameId: null,
      inheritedFirstFrameSourceSegmentId: null,
      lastFrames: { images: [], confirmedImageId: null, generationAttemptId: null },
      actualVideoLastFrameId: null,
      videoGuidanceMode: "first_frame",
      attempt: null,
      previousAttempts: [],
      video: null,
      archivedVideos: [],
      error: null,
    }],
  };
}
