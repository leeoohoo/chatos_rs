import { describe, expect, it } from "vitest";
import {
  effectiveThinkingLevel,
  selectedModel,
  type ConversationRuntimeSettingsSnapshot,
} from "./conversationRuntimeSettings";

const snapshot: ConversationRuntimeSettingsSnapshot = {
  settings: {
    selectedModelId: "model-1",
    selectedThinkingLevel: "high",
    remoteConnectionId: null,
    reasoningEnabled: true,
    version: 1,
  },
  models: [{
    id: "model-1",
    displayName: "GPT",
    modelName: "gpt-5",
    provider: "openai",
    defaultThinkingLevel: "medium",
    supportsReasoning: true,
    thinkingLevels: ["none", "medium", "high"],
  }],
  remoteConnections: [],
};

describe("conversation runtime settings projection", () => {
  it("uses the selected supported reasoning level", () => {
    expect(selectedModel(snapshot)?.id).toBe("model-1");
    expect(effectiveThinkingLevel(snapshot)).toBe("high");
  });

  it("projects disabled reasoning as none", () => {
    expect(effectiveThinkingLevel({
      ...snapshot,
      settings: { ...snapshot.settings, reasoningEnabled: false },
    })).toBe("none");
  });
});
