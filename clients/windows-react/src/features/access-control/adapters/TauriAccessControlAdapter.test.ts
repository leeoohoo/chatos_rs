import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriAccessControlAdapter } from "./TauriAccessControlAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriAccessControlAdapter", () => {
  beforeEach(() => vi.clearAllMocks());

  it.each([
    ["on_request", "user", false, "request_approval"],
    ["on_request", "auto_review", true, "auto_approval"],
    ["never", "user", true, "full_control"],
  ] as const)("maps %s/%s to the enforced tool approval mode", async (policy, reviewer, riskAcknowledged, mode) => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({})
      .mockResolvedValueOnce(snapshot());

    await new TauriAccessControlAdapter().updateApproval("owner-1", policy, reviewer, riskAcknowledged);

    expect(invoke).toHaveBeenNthCalledWith(1, "update_tool_approval_mode", {
      input: { ownerUserId: "owner-1", mode, riskAcknowledged },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "load_access_control", { ownerUserId: "owner-1" });
  });
});

function snapshot() {
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
      permissionConfigurationError: "文件与网络隔离尚不可用。",
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
