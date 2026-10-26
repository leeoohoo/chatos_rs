import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AccessControlSnapshot } from "../domain/accessControl";
import type { AccessControlPort } from "../ports/AccessControlPort";
import { useAccessControlStore } from "./accessControlStore";

describe("access control store", () => {
  beforeEach(() => useAccessControlStore.getState().reset());

  it("keeps the last trusted snapshot when refresh fails", async () => {
    const snapshot = fixture();
    const port = stubPort(vi.fn().mockResolvedValueOnce(snapshot).mockRejectedValueOnce(new Error("Host restarting")));
    await useAccessControlStore.getState().activate(port, "owner-1");
    await useAccessControlStore.getState().refresh(port);
    expect(useAccessControlStore.getState().snapshot).toEqual(snapshot);
    expect(useAccessControlStore.getState().error).toBe("Host restarting");
  });

  it("rejects late results after owner replacement", async () => {
    const first = deferred<AccessControlSnapshot>();
    const second = fixture();
    second.settings.policyRevision = "owner-b";
    const port = stubPort(vi.fn(async (owner: string) => owner === "owner-a" ? first.promise : second));
    const stale = useAccessControlStore.getState().activate(port, "owner-a");
    await useAccessControlStore.getState().activate(port, "owner-b");
    first.resolve(fixture());
    await stale;
    expect(useAccessControlStore.getState().ownerUserId).toBe("owner-b");
    expect(useAccessControlStore.getState().snapshot?.settings.policyRevision).toBe("owner-b");
  });

  it("rejects late results after Host suspension", async () => {
    const late = deferred<AccessControlSnapshot>();
    const port = stubPort(vi.fn(async () => late.promise));
    const load = useAccessControlStore.getState().activate(port, "owner-1");
    useAccessControlStore.getState().suspend("owner-1");
    late.resolve(fixture());
    await load;
    expect(useAccessControlStore.getState().snapshot).toBeNull();
    expect(useAccessControlStore.getState().isLoading).toBe(false);
  });

  it("clears owner state for an empty activation", async () => {
    const port = stubPort();
    await useAccessControlStore.getState().activate(port, "owner-1");
    await useAccessControlStore.getState().activate(port, "  ");
    expect(useAccessControlStore.getState().ownerUserId).toBeNull();
    expect(useAccessControlStore.getState().snapshot).toBeNull();
  });

  it("updates the effective approval policy and rejects a late mutation after suspension", async () => {
    const updated = fixture();
    updated.settings.defaultApprovalPolicy = "never";
    updated.settings.defaultApprovalReviewer = "policy";
    const late = deferred<AccessControlSnapshot>();
    const updateApproval = vi.fn()
      .mockResolvedValueOnce(updated)
      .mockImplementationOnce(async () => late.promise);
    const port = stubPort(undefined, updateApproval);
    await useAccessControlStore.getState().activate(port, "owner-1");
    expect(await useAccessControlStore.getState().updateApproval(port, "never", "user", true)).toBe(true);
    expect(useAccessControlStore.getState().snapshot?.settings.defaultApprovalPolicy).toBe("never");
    const pending = useAccessControlStore.getState().updateApproval(port, "on_request", "user", false);
    useAccessControlStore.getState().suspend("owner-1");
    late.resolve(fixture());
    expect(await pending).toBe(false);
    expect(useAccessControlStore.getState().snapshot?.settings.defaultApprovalPolicy).toBe("never");
  });
});

function stubPort(
  load: AccessControlPort["load"] = vi.fn(async (_ownerUserId: string) => fixture()),
  updateApproval: AccessControlPort["updateApproval"] = vi.fn(async () => fixture()),
): AccessControlPort {
  return { load, updateApproval };
}

function fixture(): AccessControlSnapshot {
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
      permissionConfigurationError: "文件和网络隔离尚未提供。",
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

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}
