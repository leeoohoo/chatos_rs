import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjectConversationPort } from "../ports/ProjectConversationPort";
import { useProjectConversationStore } from "./projectConversationStore";

describe("project conversation preparation", () => {
  beforeEach(() => useProjectConversationStore.getState().reset());

  it("reuses an indexed conversation without invoking the adapter", async () => {
    const port = stubPort();

    await useProjectConversationStore.getState().prepare(port, input({
      existingConversationId: "conversation-existing",
    }));

    expect(port.prepareProjectConversation).not.toHaveBeenCalled();
    expect(useProjectConversationStore.getState().phase("project-1")).toEqual({
      kind: "available",
      conversationId: "conversation-existing",
    });
  });

  it("keeps a failure until an explicit retry succeeds", async () => {
    const prepare = vi
      .fn<ProjectConversationPort["prepareProjectConversation"]>()
      .mockRejectedValueOnce(new Error("Host unavailable"))
      .mockResolvedValueOnce({ projectId: "project-1", conversationId: "conversation-1" });
    const port = { prepareProjectConversation: prepare };

    await useProjectConversationStore.getState().prepare(port, input());
    expect(useProjectConversationStore.getState().phase("project-1")).toEqual({
      kind: "failed",
      message: "Host unavailable",
    });
    await useProjectConversationStore.getState().prepare(port, input());
    expect(prepare).toHaveBeenCalledTimes(1);

    await useProjectConversationStore.getState().prepare(port, input({ force: true }));
    expect(useProjectConversationStore.getState().phase("project-1")).toEqual({
      kind: "available",
      conversationId: "conversation-1",
    });
  });

  it("rejects a late result after the authenticated owner changes", async () => {
    let resolve!: (value: { projectId: string; conversationId: string }) => void;
    const pending = new Promise<{ projectId: string; conversationId: string }>((done) => {
      resolve = done;
    });
    const port = stubPort(pending);
    const preparation = useProjectConversationStore.getState().prepare(port, input());

    useProjectConversationStore.getState().reset("owner-2");
    resolve({ projectId: "project-1", conversationId: "stale" });
    await preparation;

    expect(useProjectConversationStore.getState().ownerUserId).toBe("owner-2");
    expect(useProjectConversationStore.getState().phase("project-1")).toEqual({ kind: "idle" });
  });

  it("waits for Host readiness and the default project contact", async () => {
    const port = stubPort();

    await useProjectConversationStore.getState().prepare(port, input({ hostReady: false }));
    await useProjectConversationStore.getState().prepare(port, input({ defaultContactReady: false }));

    expect(port.prepareProjectConversation).not.toHaveBeenCalled();
    expect(useProjectConversationStore.getState().phase("project-1")).toEqual({ kind: "idle" });
  });

  it("clears a removed project's cached phase and rejects its late preparation", async () => {
    let resolve!: (value: { projectId: string; conversationId: string }) => void;
    const pending = new Promise<{ projectId: string; conversationId: string }>((done) => { resolve = done; });
    const preparation = useProjectConversationStore.getState().prepare(stubPort(pending), input());

    useProjectConversationStore.getState().remove("project-1");
    resolve({ projectId: "project-1", conversationId: "stale" });
    await preparation;

    expect(useProjectConversationStore.getState().phase("project-1")).toEqual({ kind: "idle" });
  });
});

interface TestInput {
  ownerUserId: string;
  projectId: string;
  existingConversationId: string | null;
  hostReady: boolean;
  defaultContactReady: boolean;
  force?: boolean;
}

function input(overrides: Partial<TestInput> = {}): TestInput {
  return {
    ownerUserId: "owner-1",
    projectId: "project-1",
    existingConversationId: null,
    hostReady: true,
    defaultContactReady: true,
    ...overrides,
  };
}

function stubPort(result = Promise.resolve({ projectId: "project-1", conversationId: "conversation-1" })) {
  return {
    prepareProjectConversation: vi.fn(() => result),
  } satisfies ProjectConversationPort;
}
