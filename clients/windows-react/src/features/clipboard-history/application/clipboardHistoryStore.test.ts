import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipboardHistoryPort } from "../ports/ClipboardHistoryPort";
import { selectFilteredEntries, useClipboardHistoryStore } from "./clipboardHistoryStore";

describe("clipboard history store", () => {
  beforeEach(() => useClipboardHistoryStore.setState({ language: "zh-CN", entries: [], query: "", selectedIndex: 0, isLoading: false, isMutating: false, error: null, requestId: 0 }));

  it("loads, filters, pins, and restores through the native port", async () => {
    const port = stubPort();
    await useClipboardHistoryStore.getState().load(port);
    expect(useClipboardHistoryStore.getState().language).toBe("en-US");
    useClipboardHistoryStore.getState().setQuery("hello");
    expect(selectFilteredEntries(useClipboardHistoryStore.getState())).toHaveLength(1);
    const entry = useClipboardHistoryStore.getState().entries[0]!;
    await useClipboardHistoryStore.getState().setPinned(port, entry);
    expect(port.setPinned).toHaveBeenCalledWith(entry.id, true);
    await expect(useClipboardHistoryStore.getState().restore(port, entry)).resolves.toBe(true);
    expect(port.close).toHaveBeenCalled();
  });

  it("preserves the last trusted list when background refresh fails", async () => {
    const port = stubPort();
    await useClipboardHistoryStore.getState().load(port);
    vi.mocked(port.list).mockRejectedValueOnce(new Error("offline"));
    await useClipboardHistoryStore.getState().refresh(port);
    expect(useClipboardHistoryStore.getState().entries).toHaveLength(1);
    expect(useClipboardHistoryStore.getState().error).toBeNull();
  });
});

function stubPort(): ClipboardHistoryPort {
  const history = { language: "en-US" as const, entries: [{ id: "11111111-1111-4111-8111-111111111111", kind: "text" as const, createdAtUnixMs: 1, updatedAtUnixMs: 2, preview: "hello", byteCount: 5, isPinned: false, previewUrl: null }] };
  return {
    list: vi.fn(async () => history),
    setPinned: vi.fn(async () => ({ ...history, entries: [{ ...history.entries[0]!, isPinned: true }] })),
    delete: vi.fn(async () => ({ ...history, entries: [] })),
    clear: vi.fn(async () => ({ ...history, entries: [] })),
    restore: vi.fn(async () => undefined),
    close: vi.fn(async () => undefined),
  };
}
