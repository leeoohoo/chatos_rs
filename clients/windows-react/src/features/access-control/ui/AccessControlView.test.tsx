import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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

  it("renders the macOS information architecture with only enforced Windows controls editable", async () => {
    const port = stubPort();
    render(<AccessControlView hostReady ownerUserId="owner-1" port={port} />);
    expect(await screen.findByRole("heading", { name: "权限控制" })).toBeVisible();
    expect(screen.getByRole("heading", { name: "默认任务权限" })).toBeVisible();
    expect(screen.getByText("新任务在没有项目级覆盖时使用这套策略")).toBeVisible();
    expect(screen.getByRole("heading", { name: "运行后端" })).toBeVisible();
    expect(screen.getByText("当前版本只允许使用本机进程后端")).toBeVisible();
    expect(screen.getByText("审批策略可编辑")).toBeInTheDocument();
    expect(screen.getAllByText("Windows 账号权限")).toHaveLength(2);
    expect(screen.getByLabelText("审批策略")).toHaveValue("on_request");
    expect(screen.getByLabelText("审批人")).toHaveValue("user");
    expect(screen.getByText("Windows 本机进程")).toBeInTheDocument();
    expect(screen.getByText("— 文件隔离")).toBeInTheDocument();
    expect(screen.getByText("— 网络隔离")).toBeInTheDocument();
    expect(screen.getByText("✓ 进程树控制")).toBeInTheDocument();
    expect(screen.getByLabelText("本机权限控制")).toBeDisabled();
    expect(screen.getByLabelText("文件权限")).toBeDisabled();
    expect(screen.getByLabelText("审批策略")).toBeEnabled();
    expect(screen.getByLabelText("审批人")).toBeEnabled();
    expect(screen.getByLabelText("网络访问")).toBeDisabled();
  });

  it("requires an explicit risk confirmation before enabling full control", async () => {
    const port = stubPort();
    render(<AccessControlView hostReady ownerUserId="owner-1" port={port} />);
    await screen.findByText("Windows 本机进程");
    fireEvent.change(screen.getByLabelText("审批策略"), { target: { value: "never" } });
    expect(screen.getByRole("alertdialog")).toHaveTextContent("启用完全控制？");
    fireEvent.click(screen.getByRole("button", { name: "确认更改" }));
    await waitFor(() => expect(port.updateApproval).toHaveBeenCalledWith("owner-1", "never", "user", true));
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
  return {
    load: vi.fn(async () => snapshot()),
    updateApproval: vi.fn(async (_ownerUserId, approvalPolicy, approvalReviewer) => ({
      ...snapshot(),
      settings: {
        ...snapshot().settings,
        defaultApprovalPolicy: approvalPolicy,
        defaultApprovalReviewer: approvalPolicy === "never" ? "policy" : approvalReviewer,
      },
    })),
  };
}

function snapshot(): AccessControlSnapshot {
  return {
    settings: {
      enabled: true,
      editable: true,
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
