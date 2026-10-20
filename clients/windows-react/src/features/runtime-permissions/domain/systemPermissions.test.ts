import { describe, expect, it } from "vitest";
import {
  isPermissionReady,
  permissionTone,
  systemPermissionsSchema,
} from "./systemPermissions";

describe("system permissions domain", () => {
  it("accepts the bounded platform projection and rejects duplicate identities", () => {
    const snapshot = permissions();
    expect(systemPermissionsSchema.parse(snapshot)).toEqual(snapshot);
    expect(() => systemPermissionsSchema.parse({
      ...snapshot,
      items: [snapshot.items[0], snapshot.items[0]],
    })).toThrow("系统权限标识不能重复");
    expect(() => systemPermissionsSchema.parse({ ...snapshot, secret: "must-not-cross" })).toThrow();
  });

  it("matches the macOS ready and attention status semantics", () => {
    expect(isPermissionReady("ready")).toBe(true);
    expect(isPermissionReady("on_demand")).toBe(true);
    expect(isPermissionReady("missing_dependency")).toBe(false);
    expect(permissionTone("action_required")).toBe("attention");
  });
});

function permissions() {
  return {
    platform: "windows",
    platformLabel: "Windows",
    items: [{
      id: "terminal",
      label: "本机终端执行",
      summary: "由原生层执行",
      status: "ready" as const,
      statusLabel: "已就绪",
      required: false,
      canRequest: false,
      requestLabel: "无需操作",
      note: "仍受审批约束。",
      lastError: null,
    }],
  };
}
