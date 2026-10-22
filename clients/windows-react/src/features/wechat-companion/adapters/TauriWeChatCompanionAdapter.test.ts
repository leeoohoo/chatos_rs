import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriWeChatCompanionAdapter } from "./TauriWeChatCompanionAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriWeChatCompanionAdapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("polls and confirms by owner without returning a browser-controlled ticket id", async () => {
    const adapter = new TauriWeChatCompanionAdapter();
    vi.mocked(invoke).mockResolvedValueOnce({
      status: "claimed", expiresAtUnix: 4_102_444_800,
      previewUrl: "chatos-wechat://localhost/opaque-token",
    });
    await adapter.pollTicket("owner-1");
    expect(invoke).toHaveBeenLastCalledWith("poll_wechat_bind_ticket", { ownerUserId: "owner-1" });
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await adapter.confirmTicket("owner-1");
    expect(invoke).toHaveBeenLastCalledWith("confirm_wechat_bind_ticket", { ownerUserId: "owner-1" });
  });

  it("rejects a native projection that leaks the server ticket identity", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      status: "issued", expiresAtUnix: 4_102_444_800,
      previewUrl: "chatos-wechat://localhost/opaque-token", ticketId: "secret-ticket",
    });
    await expect(new TauriWeChatCompanionAdapter().issueTicket("owner-1")).rejects.toThrow();
  });
});
