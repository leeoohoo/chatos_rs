import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RemoteConnectionPort } from "../ports/RemoteConnectionPort";
import { remoteConnectionFixture } from "../../../test/remoteConnectionFixture";
import { RemoteConnectionDetailView } from "./RemoteConnectionDetailView";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("RemoteConnectionDetailView", () => {
  it("uses the macOS tab order and opens on connection details", () => {
    renderView();
    const tablist = screen.getByRole("tablist", { name: "远端工作区" });
    expect(within(tablist).getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
      ">_远程终端",
      "▰SFTP 文件",
      "ⓘ连接信息",
    ]);
    expect(screen.getByRole("tab", { name: /连接信息/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText("严格校验")).toBeVisible();
    expect(screen.getByText("这台 Windows 设备 · ChatOS 客户端")).toBeVisible();
  });

  it("deletes only after showing the source-of-truth safety explanation", async () => {
    const adapter = port();
    const onDeleted = vi.fn();
    renderView(adapter, onDeleted);
    fireEvent.click(screen.getByRole("button", { name: "删除连接" }));
    expect(screen.getByText("只会删除 ChatOS 保存的连接配置，不会修改远端服务器。")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "删除" }));
    await waitFor(() => expect(adapter.delete).toHaveBeenCalledWith({
      ownerUserId: "alice",
      connectionId: "remote-1",
      expectedVersion: 1,
    }));
    expect(onDeleted).toHaveBeenCalledWith("remote-1");
  });
});

function renderView(adapter = port(), onDeleted = vi.fn()) {
  const connection = remoteConnectionFixture();
  return render(
    <RemoteConnectionDetailView
      connection={connection}
      connections={[connection]}
      ownerUserId="alice"
      port={adapter}
      onDeleted={onDeleted}
      onEdit={() => undefined}
    />,
  );
}

function port(overrides: Partial<RemoteConnectionPort> = {}): RemoteConnectionPort {
  return {
    list: vi.fn(async () => [remoteConnectionFixture()]),
    get: vi.fn(async () => remoteConnectionFixture()),
    create: vi.fn(async () => remoteConnectionFixture()),
    update: vi.fn(async () => remoteConnectionFixture({ version: 2 })),
    delete: vi.fn(async () => "remote-1"),
    testDraft: vi.fn(async () => ({ success: true, verificationRequired: false, message: "连接测试成功。", verificationPrompt: null })),
    testSaved: vi.fn(async () => ({ success: true, verificationRequired: false, message: "连接测试成功。", verificationPrompt: null })),
    pickCredentialFile: vi.fn(async () => null),
    discardCredentialFileSelections: vi.fn(async () => undefined),
    ...overrides,
  };
}
