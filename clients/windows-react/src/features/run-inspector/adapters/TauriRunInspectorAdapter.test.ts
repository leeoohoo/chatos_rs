import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import { TauriRunInspectorAdapter } from "./TauriRunInspectorAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriRunInspectorAdapter", () => {
  it("preserves string errors from the native Run inspector", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("Run record was pruned");

    await expect(
      new TauriRunInspectorAdapter().loadRun("owner-1", "run-1", 0),
    ).rejects.toThrow("Run record was pruned");
  });

  it("preserves structured native error messages", async () => {
    vi.mocked(invoke).mockRejectedValueOnce({ message: "Run resource is unavailable" });

    await expect(
      new TauriRunInspectorAdapter().loadRun("owner-1", "run-1", 0),
    ).rejects.toThrow("Run resource is unavailable");
  });
});
