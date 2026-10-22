import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriClipboardHistoryAdapter } from "./TauriClipboardHistoryAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriClipboardHistoryAdapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("uses narrow native commands", async () => {
    const adapter = new TauriClipboardHistoryAdapter();
    vi.mocked(invoke).mockResolvedValueOnce({ language: "zh-CN", entries: [] });
    await adapter.setPinned("11111111-1111-4111-8111-111111111111", true);
    expect(invoke).toHaveBeenCalledWith("set_clipboard_history_entry_pinned", { id: "11111111-1111-4111-8111-111111111111", pinned: true });
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await adapter.restore("11111111-1111-4111-8111-111111111111");
    expect(invoke).toHaveBeenLastCalledWith("restore_clipboard_history_entry", { id: "11111111-1111-4111-8111-111111111111" });
  });
});
