import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AskUserPrompt } from "../domain/askUser";
import type { AskUserPort } from "../ports/AskUserPort";
import { sampleAskUserPrompt } from "../../../test/askUserFixture";
import { useAskUserStore } from "./askUserStore";

describe("Ask User store", () => {
  beforeEach(() => useAskUserStore.getState().reset());

  it("rejects a late refresh after switching conversations", async () => {
    const first = deferred<AskUserPrompt[]>();
    const port = stubPort({
      loadPrompts: vi.fn(async (_owner, conversation) => (
        conversation === "conversation-a" ? first.promise : [prompt("b", "conversation-b")]
      )),
    });
    const oldLoad = useAskUserStore.getState().activate(port, "user-1", "conversation-a");
    await useAskUserStore.getState().activate(port, "user-1", "conversation-b");
    first.resolve([prompt("a", "conversation-a")]);
    await oldLoad;
    expect(useAskUserStore.getState().conversationId).toBe("conversation-b");
    expect(useAskUserStore.getState().prompts.map((item) => item.id)).toEqual(["local-ask:b"]);
  });

  it("allows independent prompt actions but prevents duplicate action on one prompt", async () => {
    const first = deferred<AskUserPrompt>();
    const second = deferred<AskUserPrompt>();
    const port = stubPort({
      loadPrompts: vi.fn(async () => [prompt("a"), prompt("b")]),
      submitPrompt: vi.fn(async (request) => (
        request.promptId === "local-ask:a" ? first.promise : second.promise
      )),
    });
    await useAskUserStore.getState().activate(port, "user-1", "conversation-1");
    const [a, b] = useAskUserStore.getState().prompts;
    const actionA = useAskUserStore.getState().submit(port, a, { values: {}, selection: null });
    const duplicateA = await useAskUserStore.getState().submit(port, a, { values: {}, selection: null });
    const actionB = useAskUserStore.getState().submit(port, b, { values: {}, selection: null });
    expect(duplicateA).toBe(false);
    expect(Object.keys(useAskUserStore.getState().actionIds).sort()).toEqual([
      "local-ask:a",
      "local-ask:b",
    ]);
    first.resolve({ ...a, status: "ok" });
    second.resolve({ ...b, status: "canceled" });
    expect(await actionA).toBe(true);
    expect(await actionB).toBe(true);
    expect(useAskUserStore.getState().prompts.map((item) => item.status)).toEqual(["ok", "canceled"]);
  });

  it("keeps a prompt-level error and rereads Host truth after failure", async () => {
    const pending = prompt("a");
    const port = stubPort({
      loadPrompts: vi.fn()
        .mockResolvedValueOnce([pending])
        .mockResolvedValueOnce([]),
      submitPrompt: vi.fn(async () => { throw new Error("Run is no longer waiting"); }),
    });
    await useAskUserStore.getState().activate(port, "user-1", "conversation-1");
    expect(await useAskUserStore.getState().submit(port, pending, { values: {}, selection: null })).toBe(false);
    expect(useAskUserStore.getState().promptErrors[pending.id]).toBe("Run is no longer waiting");
    expect(useAskUserStore.getState().prompts).toEqual([]);
    expect(useAskUserStore.getState().actionIds).toEqual({});
    expect(port.loadPrompts).toHaveBeenCalledTimes(2);
  });

  it("does not let an old refresh overwrite a completed mutation", async () => {
    const pending = prompt("a");
    const stale = deferred<AskUserPrompt[]>();
    const port = stubPort({
      loadPrompts: vi.fn()
        .mockResolvedValueOnce([pending])
        .mockImplementationOnce(async () => stale.promise),
      submitPrompt: vi.fn(async () => ({ ...pending, status: "ok" as const })),
    });
    await useAskUserStore.getState().activate(port, "user-1", "conversation-1");
    const refreshing = useAskUserStore.getState().refresh(port);
    await useAskUserStore.getState().submit(port, pending, { values: {}, selection: null });
    stale.resolve([pending]);
    await refreshing;
    expect(useAskUserStore.getState().prompts[0].status).toBe("ok");
  });
});

function prompt(id: string, conversationId = "conversation-1"): AskUserPrompt {
  return { ...sampleAskUserPrompt(), id: `local-ask:${id}`, sessionId: conversationId };
}
function stubPort(overrides: Partial<AskUserPort> = {}): AskUserPort {
  return {
    loadPrompts: vi.fn(async () => []),
    submitPrompt: vi.fn(async () => { throw new Error("not implemented"); }),
    cancelPrompt: vi.fn(async () => { throw new Error("not implemented"); }),
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}
