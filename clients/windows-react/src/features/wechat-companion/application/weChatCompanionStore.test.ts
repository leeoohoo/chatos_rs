import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WeChatCompanionSnapshot } from "../domain/wechatCompanion";
import type { WeChatCompanionPort } from "../ports/WeChatCompanionPort";
import { useWeChatCompanionStore } from "./weChatCompanionStore";

describe("weChatCompanionStore", () => {
  beforeEach(() => useWeChatCompanionStore.getState().reset());

  it("loads the owner and completes the confirmed desktop binding flow", async () => {
    const port = stubPort();
    await useWeChatCompanionStore.getState().activate(port, "owner-1");
    await useWeChatCompanionStore.getState().issueTicket(port);
    await useWeChatCompanionStore.getState().pollTicket(port);
    expect(useWeChatCompanionStore.getState().ticket?.status).toBe("claimed");
    await useWeChatCompanionStore.getState().confirmTicket(port);
    expect(port.confirmTicket).toHaveBeenCalledWith("owner-1");
    expect(useWeChatCompanionStore.getState().ticket).toBeNull();
  });

  it("rejects a delayed load after the owner changes", async () => {
    let release!: (value: WeChatCompanionSnapshot) => void;
    const port = stubPort({ load: vi.fn(async () => new Promise<WeChatCompanionSnapshot>((resolve) => { release = resolve; })) });
    const pending = useWeChatCompanionStore.getState().activate(port, "owner-1");
    useWeChatCompanionStore.getState().reset();
    release(snapshot());
    await pending;
    expect(useWeChatCompanionStore.getState().snapshot).toBeNull();
  });

  it("backs off polling failures without discarding the active QR code", async () => {
    const port = stubPort({ pollTicket: vi.fn(async () => { throw new Error("offline"); }) });
    await useWeChatCompanionStore.getState().activate(port, "owner-1");
    await useWeChatCompanionStore.getState().issueTicket(port);
    await useWeChatCompanionStore.getState().pollTicket(port);
    expect(useWeChatCompanionStore.getState().ticket?.previewUrl).toContain("chatos-wechat");
    expect(useWeChatCompanionStore.getState().pollingDelayMs).toBe(4_000);
  });
});

function stubPort(overrides: Partial<WeChatCompanionPort> = {}): WeChatCompanionPort {
  return {
    load: vi.fn(async () => snapshot()),
    issueTicket: vi.fn(async () => ({ status: "issued" as const, expiresAtUnix: 4_102_444_800, previewUrl: "chatos-wechat://localhost/token" })),
    pollTicket: vi.fn(async () => ({ status: "claimed" as const, expiresAtUnix: 4_102_444_800, previewUrl: "chatos-wechat://localhost/token" })),
    confirmTicket: vi.fn(async () => undefined),
    revokeSession: vi.fn(async () => undefined),
    unbind: vi.fn(async () => undefined),
    discardTicket: vi.fn(async () => undefined),
    ...overrides,
  };
}

function snapshot(): WeChatCompanionSnapshot {
  return { binding: { bound: false, createdAt: null, lastLoginAt: null }, sessions: [] };
}
