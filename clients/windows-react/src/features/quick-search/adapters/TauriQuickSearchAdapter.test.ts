import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriQuickSearchAdapter } from "./TauriQuickSearchAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriQuickSearchAdapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("sends only presentation-scoped typed intents", async () => {
    const adapter = new TauriQuickSearchAdapter();
    vi.mocked(invoke).mockResolvedValueOnce({
      presentationId: "11111111-1111-4111-8111-111111111111",
      requestId: 3,
      results: [],
      isFileSearchTimedOut: false,
      diagnostic: null,
    });
    await adapter.search("11111111-1111-4111-8111-111111111111", "/ report", 3, true);
    expect(invoke).toHaveBeenCalledWith("query_quick_search", {
      input: {
        presentationId: "11111111-1111-4111-8111-111111111111",
        query: "/ report",
        requestId: 3,
        includeFiles: true,
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await adapter.execute(
      "11111111-1111-4111-8111-111111111111",
      "22222222-2222-4222-8222-222222222222",
    );
    expect(invoke).toHaveBeenLastCalledWith("execute_quick_search", {
      input: {
        presentationId: "11111111-1111-4111-8111-111111111111",
        resultId: "22222222-2222-4222-8222-222222222222",
      },
    });
  });
});
