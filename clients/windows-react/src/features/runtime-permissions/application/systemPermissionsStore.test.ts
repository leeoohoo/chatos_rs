import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SystemPermissions } from "../domain/systemPermissions";
import type { SystemPermissionsPort } from "../ports/SystemPermissionsPort";
import { useSystemPermissionsStore } from "./systemPermissionsStore";

describe("system permissions store", () => {
  beforeEach(() => useSystemPermissionsStore.getState().reset());

  it("keeps the last trusted snapshot when refresh fails", async () => {
    const snapshot = permissions();
    const port = stubPort({
      load: vi.fn().mockResolvedValueOnce(snapshot).mockRejectedValueOnce(new Error("Host restarting")),
    });
    await useSystemPermissionsStore.getState().activate(port, "owner-1");
    await useSystemPermissionsStore.getState().refresh(port);
    expect(useSystemPermissionsStore.getState().snapshot).toEqual(snapshot);
    expect(useSystemPermissionsStore.getState().loadError).toBe("Host restarting");
  });

  it("rejects a late load after account replacement", async () => {
    const first = deferred<SystemPermissions>();
    const port = stubPort({
      load: vi.fn(async (owner) => owner === "owner-a" ? first.promise : permissions("Windows B")),
    });
    const stale = useSystemPermissionsStore.getState().activate(port, "owner-a");
    await useSystemPermissionsStore.getState().activate(port, "owner-b");
    first.resolve(permissions("Windows A"));
    await stale;
    expect(useSystemPermissionsStore.getState().ownerUserId).toBe("owner-b");
    expect(useSystemPermissionsStore.getState().snapshot?.platformLabel).toBe("Windows B");
  });

  it("clears the previous owner when activation receives an empty account", async () => {
    const port = stubPort();
    await useSystemPermissionsStore.getState().activate(port, "owner-1");
    await useSystemPermissionsStore.getState().activate(port, "   ");
    expect(useSystemPermissionsStore.getState().ownerUserId).toBeNull();
    expect(useSystemPermissionsStore.getState().snapshot).toBeNull();
  });

  it("requests only a published actionable permission", async () => {
    const requestable = permissions("Windows", true);
    const ready = permissions();
    const port = stubPort({
      load: vi.fn(async () => requestable),
      request: vi.fn(async () => ready),
    });
    await useSystemPermissionsStore.getState().activate(port, "owner-1");
    expect(await useSystemPermissionsStore.getState().request(port, "screen_capture")).toBe(true);
    expect(port.request).toHaveBeenCalledWith("owner-1", "screen_capture");
    expect(useSystemPermissionsStore.getState().snapshot).toEqual(ready);
    expect(await useSystemPermissionsStore.getState().request(port, "missing")).toBe(false);
  });

  it("rejects a late request after Host suspension", async () => {
    const late = deferred<SystemPermissions>();
    const port = stubPort({
      load: vi.fn(async () => permissions("Windows", true)),
      request: vi.fn(async () => late.promise),
    });
    await useSystemPermissionsStore.getState().activate(port, "owner-1");
    const request = useSystemPermissionsStore.getState().request(port, "screen_capture");
    useSystemPermissionsStore.getState().suspend("owner-1");
    late.resolve(permissions());
    expect(await request).toBe(false);
    expect(useSystemPermissionsStore.getState().snapshot?.items[0].status).toBe("action_required");
  });
});

function permissions(platformLabel = "Windows", requestable = false): SystemPermissions {
  return {
    platform: "windows",
    platformLabel,
    items: [{
      id: "screen_capture",
      label: "屏幕采集",
      summary: "屏幕采集能力",
      status: requestable ? "action_required" : "ready",
      statusLabel: requestable ? "需要授权" : "已就绪",
      required: false,
      canRequest: requestable,
      requestLabel: "授权引导",
      note: "测试权限。",
      lastError: null,
    }],
  };
}

function stubPort(overrides: Partial<SystemPermissionsPort> = {}): SystemPermissionsPort {
  return {
    load: vi.fn(async () => permissions()),
    request: vi.fn(async () => { throw new Error("not implemented"); }),
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}
