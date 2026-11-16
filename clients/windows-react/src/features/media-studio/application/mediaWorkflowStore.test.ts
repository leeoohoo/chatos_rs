import { beforeEach, describe, expect, it, vi } from "vitest";
import { useMediaWorkflowStore } from "./mediaWorkflowStore";
import type { MediaStudioPort } from "../ports/MediaStudioPort";
import { newMediaWorkflow } from "../domain/mediaWorkflow";

beforeEach(() => useMediaWorkflowStore.getState().reset());

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
  });
});

function mediaPort(workflows: ReturnType<typeof newMediaWorkflow>[]): MediaStudioPort {
  return {
    listWorkflows: vi.fn(async () => workflows),
    saveWorkflow: vi.fn(async () => undefined),
    deleteWorkflow: vi.fn(async () => undefined),
    runWorkflow: vi.fn(async ({ workflowId, clientRequestId }) => ({
      workflowId,
      clientRequestId,
      status: "completed" as const,
      finalText: "",
      imageRecordIds: [],
      videoRecordIds: [],
      errorMessage: null,
    })),
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
