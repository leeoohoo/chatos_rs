import { beforeEach, describe, expect, it } from "vitest";
import type { ConversationRuntimeSettingsSnapshot } from "../domain/conversationRuntimeSettings";
import type { ConversationRuntimeSettingsPort } from "../ports/ConversationRuntimeSettingsPort";
import { useConversationRuntimeSettingsStore } from "./conversationRuntimeSettingsStore";

const snapshot: ConversationRuntimeSettingsSnapshot = {
  settings: {
    selectedModelId: "model-1",
    selectedThinkingLevel: "medium",
    remoteConnectionId: null,
    reasoningEnabled: true,
    version: 1,
  },
  models: [
    {
      id: "model-1",
      displayName: "Model 1",
      modelName: "gpt-5",
      provider: "openai",
      defaultThinkingLevel: "medium",
      supportsReasoning: true,
      thinkingLevels: ["none", "medium", "high"],
    },
    {
      id: "model-2",
      displayName: "Model 2",
      modelName: "gpt-6",
      provider: "openai",
      defaultThinkingLevel: "high",
      supportsReasoning: true,
      thinkingLevels: ["none", "medium", "high"],
    },
  ],
  remoteConnections: [{ id: "remote-1", name: "Server" }],
};

describe("conversation runtime settings store", () => {
  beforeEach(() => useConversationRuntimeSettingsStore.getState().reset());

  it("loads one conversation snapshot", async () => {
    const port = portWith({ load: async () => snapshot });
    await useConversationRuntimeSettingsStore.getState().load(
      port,
      "owner-1",
      "conversation-1",
    );
    expect(useConversationRuntimeSettingsStore.getState()).toMatchObject({
      snapshot,
      isLoading: false,
      error: null,
    });
  });

  it("rejects a late load after reset", async () => {
    let resolve!: (value: ConversationRuntimeSettingsSnapshot) => void;
    const port = portWith({ load: () => new Promise((accepted) => { resolve = accepted; }) });
    const pending = useConversationRuntimeSettingsStore.getState().load(
      port,
      "owner-1",
      "conversation-1",
    );
    useConversationRuntimeSettingsStore.getState().reset();
    resolve(snapshot);
    await pending;
    expect(useConversationRuntimeSettingsStore.getState().snapshot).toBeNull();
  });

  it("keeps only the latest optimistic mutation response", async () => {
    let resolveFirst!: (value: ConversationRuntimeSettingsSnapshot) => void;
    let resolveSecond!: (value: ConversationRuntimeSettingsSnapshot) => void;
    const port = portWith({
      load: async () => snapshot,
      selectModel: () => new Promise((accepted) => { resolveFirst = accepted; }),
      selectRemoteConnection: () => new Promise((accepted) => { resolveSecond = accepted; }),
    });
    await useConversationRuntimeSettingsStore.getState().load(port, "owner-1", "conversation-1");
    const first = useConversationRuntimeSettingsStore.getState().selectModel(port, "model-2");
    const second = useConversationRuntimeSettingsStore.getState()
      .selectRemoteConnection(port, "remote-1");
    resolveFirst({
      ...snapshot,
      settings: { ...snapshot.settings, selectedModelId: "model-2", version: 2 },
    });
    await first;
    expect(useConversationRuntimeSettingsStore.getState().snapshot?.settings).toMatchObject({
      selectedModelId: "model-2",
      remoteConnectionId: "remote-1",
    });
    resolveSecond({
      ...snapshot,
      settings: {
        ...snapshot.settings,
        selectedModelId: "model-2",
        remoteConnectionId: "remote-1",
        version: 3,
      },
    });
    await second;
    expect(useConversationRuntimeSettingsStore.getState().snapshot?.settings).toMatchObject({
      selectedModelId: "model-2",
      remoteConnectionId: "remote-1",
      version: 3,
    });
  });
});

function portWith(
  overrides: Partial<ConversationRuntimeSettingsPort>,
): ConversationRuntimeSettingsPort {
  return {
    load: async () => snapshot,
    selectModel: async () => snapshot,
    selectReasoningLevel: async () => snapshot,
    selectRemoteConnection: async () => snapshot,
    ...overrides,
  };
}
