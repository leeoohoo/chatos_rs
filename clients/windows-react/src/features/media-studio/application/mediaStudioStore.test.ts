import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  MediaStudioGenerationOutcome,
  MediaStudioHistoryItem,
  MediaStudioHistoryPage,
  MediaStudioModel,
  MediaStudioReference,
  MediaStudioReferenceImportResult,
  MediaStudioVideoJobSnapshot,
} from "../domain/mediaStudio";
import type { MediaStudioPort } from "../ports/MediaStudioPort";
import {
  mediaStudioDisplayItems,
  mediaStudioVideoCanvasState,
  useMediaStudioStore,
} from "./mediaStudioStore";

const RECORD_NEW = "11111111-1111-4111-8111-111111111111";
const RECORD_DUPLICATE = "22222222-2222-4222-8222-222222222222";
const RECORD_OLD = "33333333-3333-4333-8333-333333333333";
const CONTENT_TOKEN = "a".repeat(32);
const REFERENCE_ID = `reference_${"b".repeat(32)}`;
const REFERENCE_ID_2 = `reference_${"c".repeat(32)}`;

describe("media studio store", () => {
  beforeEach(() => useMediaStudioStore.getState().reset(port()));

  it("keeps model and history requests independent and rejects the previous account", async () => {
    const aliceModels = deferred<MediaStudioModel[]>();
    const bobModels = deferred<MediaStudioModel[]>();
    const adapter = port({
      listModels: vi.fn((owner) => owner === "alice" ? aliceModels.promise : bobModels.promise),
      loadImageHistory: vi.fn(async ({ ownerUserId }) => page([
        historyItem(ownerUserId === "alice" ? RECORD_NEW : RECORD_OLD, 10),
      ])),
    });

    const aliceActivation = useMediaStudioStore.getState().activate(adapter, "alice");
    await vi.waitFor(() => expect(adapter.listModels).toHaveBeenCalledWith("alice"));
    const bobActivation = useMediaStudioStore.getState().activate(adapter, "bob");
    await vi.waitFor(() => expect(adapter.listModels).toHaveBeenCalledWith("bob"));

    aliceModels.resolve([model("alice-model")]);
    await Promise.resolve();
    expect(useMediaStudioStore.getState().models).toEqual([]);

    bobModels.resolve([model("bob-model")]);
    await Promise.all([aliceActivation, bobActivation]);
    expect(useMediaStudioStore.getState().ownerUserId).toBe("bob");
    expect(useMediaStudioStore.getState().models.map((value) => value.id)).toEqual(["bob-model"]);
    expect(useMediaStudioStore.getState().history.map((value) => value.id)).toEqual([RECORD_OLD]);
    expect(adapter.discardPresentation).toHaveBeenCalledWith({
      ownerUserId: "alice",
      presentationId: expect.any(String),
    });
  });

  it("merges older history newest-first, deduplicates it, and accumulates unreadable records", async () => {
    const cursor = { modifiedAtUnixMs: 90, recordId: RECORD_DUPLICATE };
    const nextCursor = { modifiedAtUnixMs: 40, recordId: RECORD_OLD };
    const adapter = port({
      loadImageHistory: vi.fn(async ({ after }) => after == null
        ? page([
          historyItem(RECORD_DUPLICATE, 100),
          historyItem(RECORD_NEW, 200),
        ], cursor, 1)
        : page([
          historyItem(RECORD_OLD, 50),
          historyItem(RECORD_DUPLICATE, 10),
        ], nextCursor, 2)),
    });

    await useMediaStudioStore.getState().activate(adapter, "alice");
    await useMediaStudioStore.getState().loadOlderHistory(adapter);

    const state = useMediaStudioStore.getState();
    expect(state.history.map((item) => item.id)).toEqual([
      RECORD_NEW,
      RECORD_DUPLICATE,
      RECORD_OLD,
    ]);
    expect(state.unreadableHistoryCount).toBe(3);
    expect(state.historyCursor).toEqual(nextCursor);
    expect(state.historyErrorMessage).toContain("3 条记录无法读取");
  });

  it("uses latest-wins reference imports and cleans a late vault result", async () => {
    const first = deferred<MediaStudioReferenceImportResult>();
    const second = deferred<MediaStudioReferenceImportResult>();
    const adapter = port({
      pickReferenceImages: vi.fn()
        .mockImplementationOnce(() => first.promise)
        .mockImplementationOnce(() => second.promise),
    });
    await useMediaStudioStore.getState().activate(adapter, "alice");

    const firstImport = useMediaStudioStore.getState().pickReferences(adapter);
    const secondImport = useMediaStudioStore.getState().pickReferences(adapter);
    first.resolve({ references: [reference(REFERENCE_ID)], errors: [] });
    await firstImport;
    expect(useMediaStudioStore.getState().references).toEqual([]);
    await vi.waitFor(() => expect(adapter.discardReferenceImages).toHaveBeenCalledWith({
      ownerUserId: "alice",
      presentationId: expect.any(String),
      referenceIds: [REFERENCE_ID],
    }));

    second.resolve({ references: [reference(REFERENCE_ID_2)], errors: ["一张图片无法读取。"] });
    await secondImport;
    expect(useMediaStudioStore.getState().references.map((value) => value.id))
      .toEqual([REFERENCE_ID_2]);
    expect(useMediaStudioStore.getState().referenceErrorMessages).toEqual(["一张图片无法读取。"]);
  });

  it("only stages assets authorized by the current history", async () => {
    const adapter = port({
      loadImageHistory: vi.fn(async () => page([historyItem(RECORD_NEW, 100)])),
      stageHistoryReferences: vi.fn(async () => ({
        references: [reference(REFERENCE_ID)],
        errors: [],
      })),
    });
    await useMediaStudioStore.getState().activate(adapter, "alice");

    await useMediaStudioStore.getState().addHistoryReferences(adapter, [{
      recordId: RECORD_OLD,
      contentToken: CONTENT_TOKEN,
    }]);
    expect(adapter.stageHistoryReferences).not.toHaveBeenCalled();
    expect(useMediaStudioStore.getState().referenceErrorMessages[0]).toContain("当前账号图片历史");

    await useMediaStudioStore.getState().addHistoryReferences(adapter, [{
      recordId: RECORD_NEW,
      contentToken: CONTENT_TOKEN,
    }]);
    expect(adapter.stageHistoryReferences).toHaveBeenCalledWith({
      ownerUserId: "alice",
      presentationId: expect.any(String),
      assets: [{ recordId: RECORD_NEW, contentToken: CONTENT_TOKEN }],
    });
    expect(useMediaStudioStore.getState().references).toHaveLength(1);
  });

  it("freezes the submitted generation request and saved result becomes newest history", async () => {
    const generated = deferred<MediaStudioGenerationOutcome>();
    const adapter = port({
      loadImageHistory: vi.fn(async () => page([historyItem(RECORD_OLD, 10)])),
      generateImage: vi.fn(() => generated.promise),
    });
    await useMediaStudioStore.getState().activate(adapter, "alice");
    useMediaStudioStore.getState().setPrompt("  A quiet lake  ");
    useMediaStudioStore.getState().setCount(2);

    const generation = useMediaStudioStore.getState().generate(adapter);
    const request = vi.mocked(adapter.generateImage).mock.calls[0]![0];
    expect(request.request).toEqual(expect.objectContaining({
      prompt: "A quiet lake",
      modelConfigId: "image-model",
      size: "1024x1024",
      count: 2,
      referenceImageIds: [],
    }));

    useMediaStudioStore.getState().setPrompt("changed while generating");
    useMediaStudioStore.getState().setCount(8);
    await useMediaStudioStore.getState().pickReferences(adapter);
    expect(useMediaStudioStore.getState().prompt).toBe("  A quiet lake  ");
    expect(useMediaStudioStore.getState().count).toBe(2);
    expect(adapter.pickReferenceImages).not.toHaveBeenCalled();

    generated.resolve({ status: "saved", item: historyItem(RECORD_NEW, 200) });
    await generation;
    expect(useMediaStudioStore.getState().history.map((item) => item.id))
      .toEqual([RECORD_NEW, RECORD_OLD]);
    expect(useMediaStudioStore.getState().isGenerating).toBe(false);
  });

  it("keeps a generated result visible when persistence fails", async () => {
    const adapter = port({
      generateImage: vi.fn(async ({ request }) => ({
        status: "generatedWithoutPersistence" as const,
        result: {
          id: "generation-1",
          modelConfigId: request.modelConfigId,
          modelName: "gpt-image-1",
          createdAtUnixMs: 300,
          images: [{
            id: "asset-1",
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
    await useMediaStudioStore.getState().activate(adapter, "alice");
    useMediaStudioStore.getState().setPrompt("Poster");
    await useMediaStudioStore.getState().generate(adapter);

    const state = useMediaStudioStore.getState();
    expect(state.transientItems[0]).toEqual(expect.objectContaining({
      id: "generation-1",
      prompt: "Poster",
    }));
    expect(state.persistenceWarning).toContain("disk full");
    expect(mediaStudioDisplayItems(state)[0]).toEqual(expect.objectContaining({
      source: "generatedWithoutPersistence",
    }));
  });

  it("discards the native presentation and rejects generation completed after teardown", async () => {
    const generated = deferred<MediaStudioGenerationOutcome>();
    const adapter = port({ generateImage: vi.fn(() => generated.promise) });
    await useMediaStudioStore.getState().activate(adapter, "alice");
    const presentationId = useMediaStudioStore.getState().presentationId;
    useMediaStudioStore.getState().setPrompt("Poster");
    const generation = useMediaStudioStore.getState().generate(adapter);

    useMediaStudioStore.getState().deactivate(adapter);
    generated.resolve({ status: "saved", item: historyItem(RECORD_NEW, 200) });
    await generation;

    expect(useMediaStudioStore.getState().history).toEqual([]);
    expect(useMediaStudioStore.getState().transientItems).toEqual([]);
    await vi.waitFor(() => expect(adapter.discardPresentation).toHaveBeenCalledWith({
      ownerUserId: "alice",
      presentationId,
    }));
  });

  it("freezes video settings, polls the native job, and prepends the completed video", async () => {
    const started = deferred<MediaStudioVideoJobSnapshot>();
    let clientRequestId = "";
    const completed = videoItem("55555555-5555-4555-8555-555555555555", 300);
    const adapter = port({
      listModels: vi.fn(async () => [model("image-model"), videoModel()]),
      loadVideoHistory: vi.fn(async () => ({
        items: [videoItem("44444444-4444-4444-8444-444444444444", 100)],
        nextCursor: null,
        unreadableCount: 0,
      })),
      startVideoGeneration: vi.fn(({ request }) => {
        clientRequestId = request.clientRequestId;
        return started.promise;
      }),
      pollVideoGeneration: vi.fn(async () => videoSnapshot(clientRequestId, {
        status: "completed",
        percent: 100,
        result: completed,
        terminal: true,
        revision: 2,
      })),
    });
    await useMediaStudioStore.getState().activate(adapter, "alice");
    const state = useMediaStudioStore.getState();
    state.setVideoPrompt("  Crane shot over a quiet lake  ");
    state.setVideoSize("2K");
    state.setVideoSeconds(12);
    state.setVideoRatio("21:9");

    const generation = useMediaStudioStore.getState().generateVideo(adapter);
    expect(adapter.startVideoGeneration).toHaveBeenCalledWith({
      ownerUserId: "alice",
      request: expect.objectContaining({
        modelConfigId: "video-model",
        prompt: "Crane shot over a quiet lake",
        size: "2K",
        seconds: 12,
        ratio: "21:9",
        inputImageId: null,
        referenceAudioId: null,
      }),
    });
    useMediaStudioStore.getState().setVideoPrompt("late edit");
    useMediaStudioStore.getState().setVideoSeconds(4);
    expect(useMediaStudioStore.getState().videoPrompt).toContain("Crane shot");
    expect(useMediaStudioStore.getState().videoSeconds).toBe(12);

    started.resolve(videoSnapshot(clientRequestId));
    await generation;
    expect(mediaStudioVideoCanvasState(useMediaStudioStore.getState()).kind).toBe("progress");
    await useMediaStudioStore.getState().pollVideo(adapter);

    const finished = useMediaStudioStore.getState();
    expect(finished.videoHistory.map((item) => item.id)).toEqual([
      completed.id,
      "44444444-4444-4444-8444-444444444444",
    ]);
    expect(mediaStudioVideoCanvasState(finished)).toEqual({ kind: "result", item: completed });
  });

  it("restores the previous video immediately when the user stops waiting", async () => {
    const previous = videoItem("44444444-4444-4444-8444-444444444444", 100);
    const adapter = port({
      listModels: vi.fn(async () => [model("image-model"), videoModel()]),
      loadVideoHistory: vi.fn(async () => ({ items: [previous], nextCursor: null, unreadableCount: 0 })),
    });
    await useMediaStudioStore.getState().activate(adapter, "alice");
    useMediaStudioStore.getState().setVideoPrompt("Moving clouds");
    await useMediaStudioStore.getState().generateVideo(adapter);
    expect(mediaStudioVideoCanvasState(useMediaStudioStore.getState()).kind).toBe("progress");

    await useMediaStudioStore.getState().cancelVideo(adapter);
    expect(adapter.cancelVideoGeneration).toHaveBeenCalledWith({
      ownerUserId: "alice",
      presentationId: expect.any(String),
      jobId: "44444444-4444-4444-8444-444444444444",
    });
    expect(mediaStudioVideoCanvasState(useMediaStudioStore.getState()))
      .toEqual({ kind: "result", item: previous });
  });

  it("keeps a failed video canvas and cancels a start result that arrives after account switch", async () => {
    const delayed = deferred<MediaStudioVideoJobSnapshot>();
    let clientRequestId = "";
    const previous = videoItem("77777777-7777-4777-8777-777777777777", 100);
    const adapter = port({
      listModels: vi.fn(async () => [model("image-model"), videoModel()]),
      loadVideoHistory: vi.fn(async () => ({ items: [previous], nextCursor: null, unreadableCount: 0 })),
      startVideoGeneration: vi.fn(({ request }) => {
        clientRequestId = request.clientRequestId;
        return delayed.promise;
      }),
    });
    await useMediaStudioStore.getState().activate(adapter, "alice");
    useMediaStudioStore.getState().setVideoPrompt("Moving clouds");
    const generation = useMediaStudioStore.getState().generateVideo(adapter);
    const switched = useMediaStudioStore.getState().activate(adapter, "bob");
    await switched;
    delayed.resolve(videoSnapshot(clientRequestId, {
      status: "failed",
      errorMessage: "provider rejected",
      terminal: true,
    }));
    await generation;
    expect(useMediaStudioStore.getState().ownerUserId).toBe("bob");
    await vi.waitFor(() => expect(adapter.cancelVideoGeneration).toHaveBeenCalledWith({
      ownerUserId: "alice",
      presentationId: expect.any(String),
      jobId: "44444444-4444-4444-8444-444444444444",
    }));

    useMediaStudioStore.getState().setVideoPrompt("Retry");
    vi.mocked(adapter.startVideoGeneration).mockImplementationOnce(async ({ request }) => (
      videoSnapshot(request.clientRequestId, {
        status: "failed",
        errorMessage: "disk full",
        terminal: true,
      })
    ));
    await useMediaStudioStore.getState().generateVideo(adapter);
    expect(mediaStudioVideoCanvasState(useMediaStudioStore.getState()))
      .toEqual({ kind: "failed", message: "disk full" });
    expect(useMediaStudioStore.getState().videoHistory).toEqual([previous]);
  });
});

function port(overrides: Partial<MediaStudioPort> = {}): MediaStudioPort {
  return {
    listModels: vi.fn(async () => [model("image-model")]),
    loadImageHistory: vi.fn(async () => page([])),
    loadVideoHistory: vi.fn(async () => ({ items: [], nextCursor: null, unreadableCount: 0 })),
    pickReferenceImages: vi.fn(async () => ({ references: [], errors: [] })),
    stageHistoryReferences: vi.fn(async () => ({ references: [], errors: [] })),
    discardReferenceImages: vi.fn(async () => undefined),
    pickVideoReferenceAudio: vi.fn(async () => null),
    discardVideoReferenceAudio: vi.fn(async () => undefined),
    discardPresentation: vi.fn(async () => undefined),
    generateImage: vi.fn(async () => ({
      status: "saved" as const,
      item: historyItem(RECORD_NEW, 100),
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

function model(id: string): MediaStudioModel {
  return {
    id,
    name: id,
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

function page(
  items: MediaStudioHistoryItem[],
  nextCursor: MediaStudioHistoryPage["nextCursor"] = null,
  unreadableCount = 0,
): MediaStudioHistoryPage {
  return { items, nextCursor, unreadableCount };
}

function historyItem(id: string, createdAtUnixMs: number): MediaStudioHistoryItem {
  return {
    id,
    prompt: `prompt-${id}`,
    modelName: "gpt-image-1",
    createdAtUnixMs,
    images: [{
      id: `asset-${id}`,
      mimeType: "image/png",
      contentToken: CONTENT_TOKEN,
      revisedPrompt: null,
      previewUrl: null,
    }],
  };
}

function reference(id: string): MediaStudioReference {
  return {
    id,
    name: `${id}.png`,
    mimeType: "image/png",
    byteSize: 1_024,
    previewUrl: null,
  };
}

function videoItem(id: string, createdAtUnixMs: number) {
  return {
    id,
    prompt: `video-${id}`,
    modelName: "MiniMax-H3",
    createdAtUnixMs,
    mimeType: "video/mp4" as const,
    contentToken: "d".repeat(32),
    previewUrl: null,
  };
}

function videoSnapshot(
  clientRequestId: string,
  overrides: Partial<MediaStudioVideoJobSnapshot> = {},
): MediaStudioVideoJobSnapshot {
  return {
    jobId: "44444444-4444-4444-8444-444444444444",
    clientRequestId,
    status: "queued",
    percent: null,
    providerJobId: "provider-job",
    result: null,
    errorMessage: null,
    terminal: false,
    revision: 1,
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}
