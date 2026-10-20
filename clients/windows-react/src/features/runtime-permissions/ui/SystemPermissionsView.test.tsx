import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useSystemPermissionsStore } from "../application/systemPermissionsStore";
import type { SystemPermissions } from "../domain/systemPermissions";
import type { SystemPermissionsPort } from "../ports/SystemPermissionsPort";
import { SystemPermissionsView } from "./SystemPermissionsView";

describe("SystemPermissionsView", () => {
  beforeEach(() => useSystemPermissionsStore.getState().reset());
  afterEach(cleanup);

  it("waits for Host readiness before querying native capability state", () => {
    const port = stubPort();
    renderView({ hostReady: false, port });
    expect(screen.getByText("本地服务尚未就绪")).toBeVisible();
    expect(port.load).not.toHaveBeenCalled();
  });

  it("renders truthful Windows platform differences", async () => {
    renderView();
    expect(await screen.findByText("本机终端执行")).toBeVisible();
    expect(screen.getByText("Computer Use")).toBeVisible();
    expect(screen.getAllByText("尚未提供")).toHaveLength(2);
    expect(screen.getByText(/没有 macOS“完全磁盘访问”的等价总开关/)).toBeVisible();
    expect(screen.queryByRole("button", { name: "授权引导" })).not.toBeInTheDocument();
  });

  it("rechecks the native snapshot without blanking the trusted rows", async () => {
    const port = stubPort({
      load: vi.fn()
        .mockResolvedValueOnce(permissions())
        .mockResolvedValueOnce({ ...permissions(), platformLabel: "Windows 11" }),
    });
    renderView({ port });
    await screen.findByText("本机终端执行");
    fireEvent.click(screen.getByRole("button", { name: "重新检测" }));
    await waitFor(() => expect(port.load).toHaveBeenCalledTimes(2));
    expect(await screen.findByText(/Windows 11 下/)).toBeVisible();
  });

  it("opens only an explicitly requestable native authorization flow", async () => {
    const initial = permissions();
    initial.items[1] = {
      ...initial.items[1],
      status: "action_required",
      statusLabel: "需要授权",
      canRequest: true,
      requestLabel: "授权引导",
    };
    const port = stubPort({
      load: vi.fn(async () => initial),
      request: vi.fn(async () => permissions()),
    });
    renderView({ port });
    fireEvent.click(await screen.findByRole("button", { name: "授权引导" }));
    await waitFor(() => expect(port.request).toHaveBeenCalledWith("owner-1", "computer_use"));
  });
});

function renderView({
  hostReady = true,
  port = stubPort(),
}: {
  hostReady?: boolean;
  port?: SystemPermissionsPort;
} = {}) {
  return render(
    <SystemPermissionsView hostReady={hostReady} ownerUserId="owner-1" port={port} />,
  );
}

function permissions(): SystemPermissions {
  return {
    platform: "windows",
    platformLabel: "Windows",
    items: [
      {
        id: "terminal",
        label: "本机终端执行",
        summary: "由 Rust/Tauri 原生层启动受控的 PowerShell 进程",
        status: "ready",
        statusLabel: "已就绪",
        required: false,
        canRequest: false,
        requestLabel: "无需操作",
        note: "命令仍受工作区边界与审批策略约束。",
        lastError: null,
      },
      {
        id: "computer_use",
        label: "Computer Use",
        summary: "Windows UI Automation 执行能力尚未通过源码准入",
        status: "missing_dependency",
        statusLabel: "尚未提供",
        required: false,
        canRequest: false,
        requestLabel: "无需操作",
        note: "这不是缺少系统授权。",
        lastError: null,
      },
      {
        id: "screen_capture",
        label: "屏幕采集",
        summary: "Windows 屏幕采集执行能力尚未通过源码准入",
        status: "missing_dependency",
        statusLabel: "尚未提供",
        required: false,
        canRequest: false,
        requestLabel: "无需操作",
        note: "当前版本不会请求屏幕捕获权限。",
        lastError: null,
      },
      {
        id: "filesystem",
        label: "文件与目录访问",
        summary: "遵循当前 Windows 账号 ACL 与 ChatOS 已授权工作区",
        status: "on_demand",
        statusLabel: "按需授权",
        required: false,
        canRequest: false,
        requestLabel: "无需操作",
        note: "Windows 没有 macOS“完全磁盘访问”的等价总开关。",
        lastError: null,
      },
    ],
  };
}

function stubPort(overrides: Partial<SystemPermissionsPort> = {}): SystemPermissionsPort {
  return {
    load: vi.fn(async () => permissions()),
    request: vi.fn(async () => { throw new Error("not implemented"); }),
    ...overrides,
  };
}
