import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useGeneralSettingsStore } from "../../general-settings/application/generalSettingsStore";
import { useWeChatCompanionStore } from "../application/weChatCompanionStore";
import type { WeChatCompanionPort } from "../ports/WeChatCompanionPort";
import { WeChatCompanionView } from "./WeChatCompanionView";

describe("WeChatCompanionView", () => {
  beforeEach(() => {
    useGeneralSettingsStore.getState().reset();
    useWeChatCompanionStore.getState().reset();
  });
  afterEach(cleanup);

  it("shows the bound state and requires confirmation before unbinding", async () => {
    const port = stubPort({
      load: vi.fn(async () => ({
        binding: { bound: true, createdAt: "2026-10-22T00:00:00Z", lastLoginAt: null },
        sessions: [],
      })),
    });
    render(<WeChatCompanionView ownerUserId="owner-1" port={port} />);
    expect(await screen.findByText("微信小程序已绑定")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "解除绑定" }));
    expect(screen.getByRole("dialog", { name: "解除微信绑定" })).toBeVisible();
    fireEvent.click(screen.getByRole("dialog").getElementsByTagName("button")[1]);
    await waitFor(() => expect(port.unbind).toHaveBeenCalledWith("owner-1"));
  });

  it("renders a native opaque QR preview without exposing a ticket id", async () => {
    const port = stubPort();
    render(<WeChatCompanionView ownerUserId="owner-1" port={port} />);
    fireEvent.click(await screen.findByRole("button", { name: "生成小程序码" }));
    const image = await screen.findByRole("img", { name: "微信小程序码" });
    expect(image).toHaveAttribute("src", "chatos-wechat://localhost/opaque-token");
    expect(document.body.textContent).not.toContain("ticket-");
  });
});

function stubPort(overrides: Partial<WeChatCompanionPort> = {}): WeChatCompanionPort {
  return {
    load: vi.fn(async () => ({ binding: { bound: false, createdAt: null, lastLoginAt: null }, sessions: [] })),
    issueTicket: vi.fn(async () => ({ status: "issued" as const, expiresAtUnix: Math.floor(Date.now() / 1_000) + 120, previewUrl: "chatos-wechat://localhost/opaque-token" })),
    pollTicket: vi.fn(async () => ({ status: "issued" as const, expiresAtUnix: Math.floor(Date.now() / 1_000) + 120, previewUrl: "chatos-wechat://localhost/opaque-token" })),
    confirmTicket: vi.fn(async () => undefined),
    revokeSession: vi.fn(async () => undefined),
    unbind: vi.fn(async () => undefined),
    discardTicket: vi.fn(async () => undefined),
    ...overrides,
  };
}
