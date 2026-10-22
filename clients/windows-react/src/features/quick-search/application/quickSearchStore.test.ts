import { beforeEach, describe, expect, it, vi } from "vitest";
import type { QuickSearchPort } from "../ports/QuickSearchPort";
import { useQuickSearchStore } from "./quickSearchStore";

const presentationId = "11111111-1111-4111-8111-111111111111";

describe("quickSearchStore", () => {
  beforeEach(() => useQuickSearchStore.setState({
    presentationId: null,
    language: "zh-CN",
    query: "",
    results: [],
    selectedIndex: 0,
    isLoading: false,
    isSearchingFiles: false,
    isExecuting: false,
    diagnostic: null,
    error: null,
    lifecycleId: 0,
    requestId: 0,
  }));

  it("loads a presentation and publishes native results", async () => {
    const port = stubPort();
    await useQuickSearchStore.getState().open(port);
    useQuickSearchStore.getState().setQuery("project");
    await useQuickSearchStore.getState().search(port, false);
    expect(useQuickSearchStore.getState().presentationId).toBe(presentationId);
    expect(useQuickSearchStore.getState().results[0]?.title).toBe("Project One");
    expect(port.search).toHaveBeenCalledWith(presentationId, "project", 1, false);
  });

  it("invalidates an in-flight search when the surface closes", async () => {
    let finish!: (value: Awaited<ReturnType<QuickSearchPort["search"]>>) => void;
    const port = stubPort();
    vi.mocked(port.search).mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    await useQuickSearchStore.getState().open(port);
    const pending = useQuickSearchStore.getState().search(port, true);
    await useQuickSearchStore.getState().close(port);
    finish({ presentationId, requestId: 1, results: [], isFileSearchTimedOut: false, diagnostic: null });
    await pending;
    expect(useQuickSearchStore.getState().presentationId).toBeNull();
    expect(useQuickSearchStore.getState().isSearchingFiles).toBe(false);
  });
});

function stubPort(): QuickSearchPort {
  const bootstrap: QuickSearchPort["bootstrap"] = vi.fn(async () => ({
    presentationId,
    language: "en-US" as const,
  }));
  const search: QuickSearchPort["search"] = vi.fn(async (_presentationId, _query, requestId) => ({
    presentationId,
    requestId,
    results: [{
      id: "22222222-2222-4222-8222-222222222222",
      kind: "chatOs" as const,
      title: "Project One",
      subtitle: "ChatOS Project",
      icon: "project" as const,
    }],
    isFileSearchTimedOut: false,
    diagnostic: null,
  }));
  return {
    bootstrap,
    search,
    execute: vi.fn(async () => undefined),
    close: vi.fn(async () => undefined),
    subscribePresented: vi.fn(async () => () => undefined),
  };
}
