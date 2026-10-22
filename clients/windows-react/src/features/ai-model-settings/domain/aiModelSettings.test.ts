import { describe, expect, it } from "vitest";
import { aiModelSettingsSnapshot } from "../../../test/aiModelSettingsFixture";
import { configurationDraft, selectableModels, validateConfigurationDraft } from "./aiModelSettings";

describe("AI model settings domain", () => {
  it("only exposes enabled credentialed models as defaults", () => {
    const source = aiModelSettingsSnapshot();
    source.models.push({ ...source.models[0]!, id: "disabled", enabled: false });
    source.models.push({ ...source.models[0]!, id: "missing-key", hasApiKey: false });
    expect(selectableModels(source.models).map((model) => model.id)).toEqual(["model-1"]);
  });

  it("matches macOS validation bounds for retries, temperature, and tokens", () => {
    const source = aiModelSettingsSnapshot();
    const draft = configurationDraft(source);
    draft.modelDrafts["model-1"]!.temperature = "2.1";
    expect(validateConfigurationDraft(draft, source.models)).toContain("Temperature");
    draft.modelDrafts["model-1"]!.temperature = "1";
    draft.modelDrafts["model-1"]!.maxOutputTokens = "0";
    expect(validateConfigurationDraft(draft, source.models)).toContain("Max Tokens");
    draft.modelDrafts["model-1"]!.maxOutputTokens = "4096";
    expect(validateConfigurationDraft(draft, source.models)).toBeNull();
  });
});
