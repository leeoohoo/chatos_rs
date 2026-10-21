import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriAgentManagementAdapter } from "./TauriAgentManagementAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriAgentManagementAdapter avatar boundary", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("passes only owner and presentation identity to the native picker", async () => {
    const draft = {
      id: `reference_${"a".repeat(32)}`,
      name: "avatar.png",
      mimeType: "image/png",
      byteSize: 1_024,
      previewUrl: "chatos-media://localhost/session/reference/presentation/token",
    };
    vi.mocked(invoke).mockResolvedValueOnce(draft);
    await expect(new TauriAgentManagementAdapter().pickAvatar("owner-1", "presentation-1"))
      .resolves.toEqual(draft);
    expect(invoke).toHaveBeenCalledWith("pick_agent_avatar", {
      input: { ownerUserId: "owner-1", presentationId: "presentation-1" },
    });
  });

  it("rejects unsafe picker projections and forwards scoped cleanup", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      id: `reference_${"a".repeat(32)}`,
      name: "avatar.png",
      mimeType: "image/png",
      byteSize: 1_024,
      previewUrl: "file:///private/avatar.png",
    });
    const adapter = new TauriAgentManagementAdapter();
    await expect(adapter.pickAvatar("owner-1", "presentation-1")).rejects.toThrow();

    vi.mocked(invoke).mockResolvedValue(undefined);
    await adapter.discardAvatarReferences("owner-1", "presentation-1", ["reference-1"]);
    await adapter.discardAvatarPresentation("owner-1", "presentation-1");
    expect(invoke).toHaveBeenNthCalledWith(2, "discard_media_studio_reference_images", {
      input: {
        ownerUserId: "owner-1",
        presentationId: "presentation-1",
        referenceIds: ["reference-1"],
      },
    });
    expect(invoke).toHaveBeenNthCalledWith(3, "discard_media_studio_presentation", {
      input: { ownerUserId: "owner-1", presentationId: "presentation-1" },
    });
  });
});
