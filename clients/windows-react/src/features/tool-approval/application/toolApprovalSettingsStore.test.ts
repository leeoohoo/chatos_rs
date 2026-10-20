import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ToolApprovalSettings } from "../domain/toolApprovalSettings";
import type { ToolApprovalSettingsPort } from "../ports/ToolApprovalSettingsPort";
import { useToolApprovalSettingsStore } from "./toolApprovalSettingsStore";

describe("tool approval settings store", () => {
  beforeEach(() => useToolApprovalSettingsStore.getState().reset());

  it("keeps the last trusted settings when refresh fails", async () => {
    const snapshot = settings();
    const port = stubPort({
      loadSettings: vi.fn()
        .mockResolvedValueOnce(snapshot)
        .mockRejectedValueOnce(new Error("Host restarting")),
    });
    await useToolApprovalSettingsStore.getState().activate(port, "owner-1");
    await useToolApprovalSettingsStore.getState().load(port);
    expect(useToolApprovalSettingsStore.getState().settings).toEqual(snapshot);
    expect(useToolApprovalSettingsStore.getState().loadError).toBe("Host restarting");
  });

  it("rejects a late load after account replacement", async () => {
    const first = deferred<ToolApprovalSettings>();
    const port = stubPort({
      loadSettings: vi.fn(async (owner) => (
        owner === "owner-a" ? first.promise : settings({ defaultMode: "full_control" })
      )),
    });
    const stale = useToolApprovalSettingsStore.getState().activate(port, "owner-a");
    await useToolApprovalSettingsStore.getState().activate(port, "owner-b");
    first.resolve(settings());
    await stale;
    expect(useToolApprovalSettingsStore.getState().ownerUserId).toBe("owner-b");
    expect(useToolApprovalSettingsStore.getState().settings?.defaultMode).toBe("full_control");
  });

  it("sends explicit risk acknowledgement and applies authoritative mode", async () => {
    const initial = settings();
    const elevated = settings({ defaultMode: "full_control" });
    const port = stubPort({
      loadSettings: vi.fn(async () => initial),
      updateMode: vi.fn(async () => elevated),
    });
    await useToolApprovalSettingsStore.getState().activate(port, "owner-1");
    expect(await useToolApprovalSettingsStore.getState().updateMode(
      port,
      "full_control",
      true,
    )).toBe(true);
    expect(port.updateMode).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      mode: "full_control",
      riskAcknowledged: true,
    });
    expect(useToolApprovalSettingsStore.getState().settings).toEqual(elevated);
  });

  it("rejects a late model mutation after Host suspension", async () => {
    const late = deferred<ToolApprovalSettings>();
    const port = stubPort({
      loadSettings: vi.fn(async () => settings()),
      updateModel: vi.fn(async () => late.promise),
    });
    await useToolApprovalSettingsStore.getState().activate(port, "owner-1");
    const mutation = useToolApprovalSettingsStore.getState().updateModel(
      port,
      "model-1",
      "high",
    );
    useToolApprovalSettingsStore.getState().suspend("owner-1");
    late.resolve(settings({ selectedModelId: "model-1", selectedThinkingLevel: "high" }));
    expect(await mutation).toBe(false);
    expect(useToolApprovalSettingsStore.getState().settings?.selectedModelId).toBeNull();
  });

  it("does not let a refresh overtake an in-flight settings mutation", async () => {
    const mutation = deferred<ToolApprovalSettings>();
    const port = stubPort({
      loadSettings: vi.fn(async () => settings()),
      updateMode: vi.fn(async () => mutation.promise),
    });
    await useToolApprovalSettingsStore.getState().activate(port, "owner-1");
    const update = useToolApprovalSettingsStore.getState().updateMode(
      port,
      "full_control",
      true,
    );
    await useToolApprovalSettingsStore.getState().load(port);
    expect(port.loadSettings).toHaveBeenCalledTimes(1);
    mutation.resolve(settings({ defaultMode: "full_control" }));
    expect(await update).toBe(true);
    expect(useToolApprovalSettingsStore.getState().settings?.defaultMode).toBe("full_control");
  });
});

function settings(overrides: Partial<ToolApprovalSettings> = {}): ToolApprovalSettings {
  return {
    defaultMode: "request_approval",
    selectedModelId: null,
    selectedThinkingLevel: null,
    models: [{
      id: "model-1",
      displayName: "GPT",
      modelName: "gpt-test",
      provider: "openai",
      supportsReasoning: true,
      thinkingLevels: ["low", "medium", "high"],
      defaultThinkingLevel: "medium",
    }],
    automaticReviewerReady: false,
    automaticReviewerDetail: "自动审批暂时转交用户确认。",
    history: [],
    ...overrides,
  };
}

function stubPort(overrides: Partial<ToolApprovalSettingsPort> = {}): ToolApprovalSettingsPort {
  return {
    loadSettings: vi.fn(async () => settings()),
    updateMode: vi.fn(async () => { throw new Error("not implemented"); }),
    updateModel: vi.fn(async () => { throw new Error("not implemented"); }),
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}
