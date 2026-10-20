import { describe, expect, it } from "vitest";
import {
  accessControlSnapshotSchema,
  approvalPolicyLabel,
  networkAccessLabel,
} from "./accessControl";

describe("access control domain", () => {
  it("accepts the truthful limited Windows backend", () => {
    const snapshot = accessControlSnapshotSchema.parse(fixture());
    expect(snapshot.backends[0].filesystemIsolation).toBe(false);
    expect(snapshot.backends[0].networkIsolation).toBe(false);
    expect(snapshot.backends[0].processTreeControl).toBe(true);
    expect(approvalPolicyLabel(snapshot)).toBe("按需审批 · 用户");
    expect(networkAccessLabel(snapshot.settings.defaultNetworkAccess)).toBe("跟随 Windows 本机");
  });

  it("rejects duplicate or missing default backends", () => {
    const duplicate = fixture();
    duplicate.backends.push({ ...duplicate.backends[0] });
    expect(() => accessControlSnapshotSchema.parse(duplicate)).toThrow();
    const missing = fixture();
    missing.settings.defaultBackend = "missing";
    expect(() => accessControlSnapshotSchema.parse(missing)).toThrow();
  });
});

export function fixture() {
  return {
    settings: {
      enabled: true,
      editable: false,
      defaultBackend: "windows_local_process",
      defaultPermissionProfileId: "host_account",
      defaultPermissionProfileName: "Windows 账号权限",
      defaultApprovalPolicy: "on_request" as const,
      defaultApprovalReviewer: "user" as const,
      defaultNetworkAccess: "host" as const,
      permissionConfigurationError: "文件和网络隔离尚未提供。",
      policyRevision: "windows-local-process-v1:request_approval",
    },
    backends: [{
      backend: "windows_local_process",
      label: "Windows 本机进程",
      status: "limited" as const,
      statusLabel: "部分保护",
      selectable: false,
      filesystemIsolation: false,
      networkIsolation: false,
      processTreeControl: true,
      message: "通过 Windows Job Object 回收进程树。",
    }],
  };
}
