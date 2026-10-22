import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AiModelSettingsSnapshot } from "../domain/aiModelSettings";
import { aiModelSettingsPort, aiModelSettingsSnapshot } from "../../../test/aiModelSettingsFixture";
import { useAiModelSettingsStore } from "./aiModelSettingsStore";

describe("AI model settings store", () => {
  beforeEach(() => useAiModelSettingsStore.getState().reset());

  it("saves a validated owner-scoped draft and accepts native truth", async () => {
    const port = aiModelSettingsPort();
    await useAiModelSettingsStore.getState().load(port, "owner-1");
    useAiModelSettingsStore.getState().updateModel("model-1", { temperature: "1.2" });
    await expect(useAiModelSettingsStore.getState().save(port)).resolves.toBe(true);
    expect(port.save).toHaveBeenCalledWith("owner-1", expect.objectContaining({
      modelDrafts: expect.objectContaining({ "model-1": expect.objectContaining({ temperature: "1.2" }) }),
    }));
    expect(useAiModelSettingsStore.getState().notice).toContain("已保存");
  });

  it("ignores a late catalog after account teardown", async () => {
    let release!: (value: AiModelSettingsSnapshot) => void;
    const port = aiModelSettingsPort({ load: vi.fn(async () => new Promise<AiModelSettingsSnapshot>((resolve) => { release = resolve; })) });
    const pending = useAiModelSettingsStore.getState().load(port, "owner-1");
    useAiModelSettingsStore.getState().reset();
    release(aiModelSettingsSnapshot());
    await pending;
    expect(useAiModelSettingsStore.getState().ownerUserId).toBeNull();
    expect(useAiModelSettingsStore.getState().snapshot).toBeNull();
  });
});
