import { beforeEach, describe, expect, it, vi } from "vitest";
import { useMediaWorkflowStore } from "./mediaWorkflowStore";
import { useMediaStudioStore } from "./mediaStudioStore";
import type { MediaStudioModel } from "../domain/mediaStudio";
import type { MediaStudioPort } from "../ports/MediaStudioPort";
import { newMediaWorkflow } from "../domain/mediaWorkflow";

beforeEach(() => {
  useMediaWorkflowStore.getState().reset();
  const image = workflowModel("image-model", "gpt-image-1", "image");
  const text = workflowModel("text-model", "gpt-6.1-sol", "chat");
  const video = workflowModel("video-model", "MiniMax-H3", "video");
  useMediaStudioStore.setState({
    models: [image, text, video],
    imageModels: [image],
    textModels: [text],
    videoModels: [video],
  });
});

describe("media workflow store", () => {
  it("keeps workflows account scoped and persists an edited draft", async () => {
    const workflow = newMediaWorkflow("image-model");
    workflow.nodes[0]!.prompt = "draw a lake";
    const port = mediaPort([workflow]);
    await useMediaWorkflowStore.getState().activate(port, "alice");
    expect(useMediaWorkflowStore.getState().workflows).toEqual([workflow]);
    useMediaWorkflowStore.getState().open(workflow);
    useMediaWorkflowStore.getState().updateDraft((value) => ({ ...value, name: "湖面工作流" }));
    await useMediaWorkflowStore.getState().save(port);
    expect(port.saveWorkflow).toHaveBeenCalledWith("alice", expect.objectContaining({ name: "湖面工作流" }));
    expect(useMediaWorkflowStore.getState().workflows[0]!.name).toBe("湖面工作流");
  });

  it("does not allow a second video node through the editor operations", () => {
    useMediaWorkflowStore.getState().create("image-model");
    useMediaWorkflowStore.getState().addNode("video", "video-model");
    useMediaWorkflowStore.getState().addNode("video", "video-model");
    expect(useMediaWorkflowStore.getState().draft?.nodes.filter((node) => node.kind === "video")).toHaveLength(1);
  });

  it("saves a draft before running and exposes the terminal result", async () => {
    const port = mediaPort([]);
    useMediaWorkflowStore.getState().create("image-model");
    useMediaWorkflowStore.getState().updateDraft((workflow) => ({ ...workflow, inputPrompt: "hello" }));
    await useMediaWorkflowStore.getState().activate(port, "alice");
    useMediaWorkflowStore.getState().create("image-model");
    useMediaWorkflowStore.getState().updateDraft((workflow) => ({
      ...workflow,
      inputPrompt: "hello",
      nodes: workflow.nodes.map((node) => ({ ...node, prompt: "draw" })),
    }));
    await useMediaWorkflowStore.getState().run(port);
    expect(port.saveWorkflow).toHaveBeenCalledTimes(1);
    expect(port.runWorkflow).toHaveBeenCalledTimes(1);
    expect(useMediaWorkflowStore.getState().runResult?.status).toBe("completed");
    expect(Object.values(useMediaWorkflowStore.getState().nodeStates)).toMatchObject([
      { phase: "completed", result: { type: "images" } },
    ]);
  });

  it("surfaces a stale save rejection without replacing the local draft", async () => {
    const workflow = newMediaWorkflow("image-model");
    workflow.nodes[0]!.prompt = "draw";
    const port = mediaPort([workflow]);
    port.saveWorkflow = vi.fn(async () => false);
    await useMediaWorkflowStore.getState().activate(port, "alice");
    useMediaWorkflowStore.getState().open(workflow);
    useMediaWorkflowStore.getState().updateDraft((value) => ({ ...value, name: "本地草稿" }));

    expect(await useMediaWorkflowStore.getState().save(port)).toBe(false);
    expect(useMediaWorkflowStore.getState().draft?.name).toBe("本地草稿");
    expect(useMediaWorkflowStore.getState().errorMessage).toContain("其他窗口更新");
  });

  it("marks the active node failed while preserving earlier completed results", async () => {
    const port = mediaPort([]);
    let listener: Parameters<MediaStudioPort["watchWorkflowRun"]>[1] | undefined;
    port.watchWorkflowRun = vi.fn(async (_scope, value) => {
      listener = value;
      return () => undefined;
    });
    port.runWorkflow = vi.fn(async (scope) => {
      const nodes = useMediaWorkflowStore.getState().draft!.nodes;
      listener?.({ ...scope, nodeId: nodes[0]!.id, phase: "completed", progress: null, result: { type: "text", text: "first" }, errorMessage: null });
      listener?.({ ...scope, nodeId: nodes[1]!.id, phase: "running", progress: "42%", result: null, errorMessage: null });
      throw new Error("视频节点失败");
    });
    await useMediaWorkflowStore.getState().activate(port, "alice");
    useMediaWorkflowStore.getState().create("text-model");
    useMediaWorkflowStore.getState().updateDraft((workflow) => ({
      ...workflow,
      nodes: [
        { ...workflow.nodes[0]!, kind: "text", prompt: "write" },
        { ...newMediaWorkflow("video-model").nodes[0]!, kind: "video", prompt: "animate" },
      ],
    }));

    await useMediaWorkflowStore.getState().run(port);
    const states = Object.values(useMediaWorkflowStore.getState().nodeStates);
    expect(states[0]).toEqual({ phase: "completed", result: { type: "text", text: "first" } });
    expect(states[1]).toEqual({ phase: "failed", errorMessage: "视频节点失败" });
  });

  it("preserves a native string failure for both the node and workflow error", async () => {
    const port = mediaPort([]);
    let listener: Parameters<MediaStudioPort["watchWorkflowRun"]>[1] | undefined;
    port.watchWorkflowRun = vi.fn(async (_scope, value) => {
      listener = value;
      return () => undefined;
    });
    port.runWorkflow = vi.fn(async (scope) => {
      const nodeId = useMediaWorkflowStore.getState().draft!.nodes[0]!.id;
      listener?.({ ...scope, nodeId, phase: "running", progress: null, result: null, errorMessage: null });
      throw "视频尺寸或时长不受当前协议支持。";
    });
    await useMediaWorkflowStore.getState().activate(port, "alice");
    useMediaWorkflowStore.getState().create("video-model", "video");

    await useMediaWorkflowStore.getState().run(port);

    expect(useMediaWorkflowStore.getState().errorMessage).toBe("视频尺寸或时长不受当前协议支持。");
    expect(Object.values(useMediaWorkflowStore.getState().nodeStates)).toEqual([
      { phase: "failed", errorMessage: "视频尺寸或时长不受当前协议支持。" },
    ]);
  });

  it("rejects an incompatible model before saving or invoking native generation", async () => {
    const port = mediaPort([]);
    await useMediaWorkflowStore.getState().activate(port, "alice");
    useMediaWorkflowStore.getState().create("text-model");

    await useMediaWorkflowStore.getState().run(port);

    expect(useMediaWorkflowStore.getState().errorMessage).toContain("图片节点");
    expect(port.saveWorkflow).not.toHaveBeenCalled();
    expect(port.runWorkflow).not.toHaveBeenCalled();
  });

  it("rejects a text model that cannot read an earlier image result", async () => {
    const text = workflowModel("text-model", "gpt-6.1-sol", "chat", false);
    useMediaStudioStore.setState({ textModels: [text], models: [
      ...useMediaStudioStore.getState().models.filter((model) => model.workload !== "chat"),
      text,
    ] });
    const port = mediaPort([]);
    await useMediaWorkflowStore.getState().activate(port, "alice");
    useMediaWorkflowStore.getState().create("image-model");
    useMediaWorkflowStore.getState().addNode("text", "text-model");

    await useMediaWorkflowStore.getState().run(port);

    expect(useMediaWorkflowStore.getState().errorMessage).toContain("不支持读取当前图片输入");
    expect(port.runWorkflow).not.toHaveBeenCalled();
  });
});

function workflowModel(
  id: string,
  modelName: string,
  workload: MediaStudioModel["workload"],
  supportsImageInput = true,
): MediaStudioModel {
  return {
    id,
    name: id,
    provider: "openai-compatible",
    modelName,
    workload,
    enabled: true,
    taskEnabled: true,
    hasApiKey: true,
    supportsImageInput,
  };
}

function mediaPort(workflows: ReturnType<typeof newMediaWorkflow>[]): MediaStudioPort {
  let savedWorkflow: ReturnType<typeof newMediaWorkflow> | undefined;
  let listener: Parameters<MediaStudioPort["watchWorkflowRun"]>[1] | undefined;
  return {
    listWorkflows: vi.fn(async () => workflows),
    saveWorkflow: vi.fn(async (_owner, workflow) => {
      savedWorkflow = structuredClone(workflow);
      return true;
    }),
    deleteWorkflow: vi.fn(async () => undefined),
    watchWorkflowRun: vi.fn(async (_scope, value) => {
      listener = value;
      return () => { listener = undefined; };
    }),
    runWorkflow: vi.fn(async (scope) => {
      const workflow = savedWorkflow ?? workflows.find((value) => value.id === scope.workflowId)!;
      const nodeResults = workflow.nodes.map((node) => ({
        nodeId: node.id,
        result: node.kind === "image"
          ? { type: "images" as const, item: imageHistoryItem() }
          : node.kind === "video"
            ? { type: "video" as const, item: videoHistoryItem() }
            : { type: "text" as const, text: "done" },
      }));
      workflow.nodes.forEach((node, index) => {
        listener?.({ ...scope, nodeId: node.id, phase: "running", progress: null, result: null, errorMessage: null });
        listener?.({ ...scope, nodeId: node.id, phase: "completed", progress: null, result: nodeResults[index]!.result, errorMessage: null });
      });
      return {
        workflowId: scope.workflowId,
        clientRequestId: scope.clientRequestId,
        status: "completed" as const,
        finalText: "",
        imageRecordIds: [],
        videoRecordIds: [],
        nodeResults,
        errorMessage: null,
      };
    }),
    listModels: vi.fn(async () => []),
    loadImageHistory: vi.fn(async () => ({ items: [], nextCursor: null, unreadableCount: 0 })),
    loadVideoHistory: vi.fn(async () => ({ items: [], nextCursor: null, unreadableCount: 0 })),
    pickReferenceImages: vi.fn(async () => ({ references: [], errors: [] })),
    stageHistoryReferences: vi.fn(async () => ({ references: [], errors: [] })),
    discardReferenceImages: vi.fn(async () => undefined),
    pickVideoReferenceAudio: vi.fn(async () => null),
    discardVideoReferenceAudio: vi.fn(async () => undefined),
    discardPresentation: vi.fn(async () => undefined),
    generateImage: vi.fn(),
    startVideoGeneration: vi.fn(),
    pollVideoGeneration: vi.fn(),
    cancelVideoGeneration: vi.fn(),
  };
}

function imageHistoryItem() {
  const session = "33333333-3333-4333-8333-333333333333";
  const id = "11111111-1111-4111-8111-111111111111";
  const token = "a".repeat(32);
  return {
    id,
    prompt: "draw",
    modelName: "image-model",
    createdAtUnixMs: 1,
    images: [{
      id: "asset-1",
      mimeType: "image/png" as const,
      contentToken: token,
      revisedPrompt: null,
      previewUrl: `http://chatos-media.localhost/${session}/${id}/${token}`,
    }],
  };
}

function videoHistoryItem() {
  const session = "33333333-3333-4333-8333-333333333333";
  const id = "22222222-2222-4222-8222-222222222222";
  const token = "b".repeat(32);
  return {
    id,
    prompt: "animate",
    modelName: "video-model",
    createdAtUnixMs: 1,
    mimeType: "video/mp4" as const,
    contentToken: token,
    previewUrl: `http://chatos-media.localhost/${session}/${id}/${token}`,
  };
}
