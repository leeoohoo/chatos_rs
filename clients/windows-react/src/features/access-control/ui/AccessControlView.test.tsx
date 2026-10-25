import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AccessControlSnapshot } from "../domain/accessControl";
import type { AccessControlPort } from "../ports/AccessControlPort";
import { useAccessControlStore } from "../application/accessControlStore";
import { AccessControlView } from "./AccessControlView";

describe("AccessControlView", () => {
  beforeEach(() => useAccessControlStore.getState().reset());
  afterEach(cleanup);

  it("shows Host unavailability without calling the adapter", () => {
    const port = stubPort();
    render(<AccessControlView hostReady={false} ownerUserId="owner-1" port={port} />);
    expect(screen.getByText("本地服务尚未就绪")).toBeInTheDocument();
    expect(port.load).not.toHaveBeenCalled();
  });

  it("renders the macOS information architecture with truthful read-only Windows capabilities", async () => {
    const port = stubPort();
    render(<AccessControlView hostReady ownerUserId="owner-1" port={port} />);
    expect(await screen.findByRole("heading", { name: "权限控制" })).toBeVisible();
    expect(screen.getByRole("heading", { name: "默认任务权限" })).toBeVisible();
    expect(screen.getByText("新任务在没有项目级覆盖时使用这套策略")).toBeVisible();
    expect(screen.getByRole("heading", { name: "运行后端" })).toBeVisible();
    expect(screen.getByText("当前版本只允许使用本机进程后端")).toBeVisible();
    expect(screen.getByText("系统强制 · 只读")).toBeInTheDocument();
    expect(screen.getByText("Windows 账号权限")).toBeInTheDocument();
    expect(screen.getByText("按需审批 · 用户")).toBeInTheDocument();
    expect(screen.getByText("Windows 本机进程")).toBeInTheDocument();
    expect(screen.getByText("— 文件隔离")).toBeInTheDocument();
    expect(screen.getByText("— 网络隔离")).toBeInTheDocument();
    expect(screen.getByText("✓ 进程树控制")).toBeInTheDocument();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  });

  it("preserves the trusted snapshot when a refresh fails", async () => {
    const port = stubPort();
    render(<AccessControlView hostReady ownerUserId="owner-1" port={port} />);
    expect(await screen.findByText("Windows 本机进程")).toBeInTheDocument();
    vi.mocked(port.load).mockRejectedValueOnce(new Error("Host restarting"));
    await act(async () => { screen.getByRole("button", { name: "刷新" }).click(); });
    expect(await screen.findByRole("alert")).toHaveTextContent("Host restarting");
    expect(screen.getByText("Windows 本机进程")).toBeInTheDocument();
  });
});

function stubPort(): AccessControlPort {
  return { load: vi.fn(async () => snapshot()) };
}

function snapshot(): AccessControlSnapshot {
  return {
    settings: {
      enabled: true,
      editable: false,
      defaultBackend: "windows_local_process",
      defaultPermissionProfileId: "host_account",
      defaultPermissionProfileName: "Windows 账号权限",
      defaultApprovalPolicy: "on_request",
      defaultApprovalReviewer: "user",
      defaultNetworkAccess: "host",
      permissionConfigurationError: "当前 Windows 后端尚未提供文件或网络隔离。",
      policyRevision: "windows-local-process-v1:request_approval",
    },
    backends: [{
      backend: "windows_local_process",
      label: "Windows 本机进程",
      status: "limited",
      statusLabel: "部分保护",
      selectable: false,
      filesystemIsolation: false,
      networkIsolation: false,
      processTreeControl: true,
      message: "通过 Windows Job Object 回收进程树。",
    }],
  };
}
