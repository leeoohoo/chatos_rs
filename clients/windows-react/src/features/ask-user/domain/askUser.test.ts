import { describe, expect, it } from "vitest";
import { sampleAskUserPrompt } from "../../../test/askUserFixture";
import {
  askUserSubmission,
  clearAskUserSecrets,
  createAskUserForm,
  isAskUserFormValid,
  promptsForTurn,
  toggleAskUserSelection,
  unattachedPendingPrompts,
} from "./askUser";

describe("Ask User domain", () => {
  it("validates trimmed required fields and choice bounds", () => {
    const prompt = sampleAskUserPrompt();
    let form = createAskUserForm(prompt);
    expect(isAskUserFormValid(prompt, form)).toBe(false);
    form = { ...form, values: { ...form.values, name: " Ada " } };
    form = toggleAskUserSelection(form, prompt.choice!, "one");
    expect(isAskUserFormValid(prompt, form)).toBe(true);
    expect(askUserSubmission(prompt, form)).toEqual({
      values: { name: " Ada ", password: "" },
      selection: ["one"],
    });
  });

  it("uses single selection and refuses additions beyond the maximum", () => {
    const prompt = sampleAskUserPrompt();
    let form = createAskUserForm(prompt);
    form = toggleAskUserSelection(form, prompt.choice!, "one");
    form = toggleAskUserSelection(form, prompt.choice!, "two");
    expect([...form.selection]).toEqual(["one"]);
    const single = { ...prompt, choice: { ...prompt.choice!, allowsMultiple: false } };
    form = toggleAskUserSelection(form, single.choice!, "two");
    expect([...form.selection]).toEqual(["two"]);
    expect(askUserSubmission(single, form).selection).toBe("two");
  });

  it("clears secret values as soon as a prompt resolves", () => {
    const prompt = sampleAskUserPrompt();
    const form = createAskUserForm(prompt);
    form.values.password = "must disappear";
    expect(clearAskUserSecrets(form, prompt).values.password).toBe("");
  });

  it("sorts attached prompts and keeps unloaded pending prompts visible", () => {
    const later = { ...sampleAskUserPrompt(), id: "local-ask:run-2", createdAtUnixMs: 20 };
    const earlier = { ...sampleAskUserPrompt(), id: "local-ask:run-1", createdAtUnixMs: 10 };
    const unattached = { ...later, id: "local-ask:run-3", turnId: "turn-unloaded" };
    expect(promptsForTurn([later, earlier], "turn-1").map((item) => item.id)).toEqual([
      "local-ask:run-1",
      "local-ask:run-2",
    ]);
    expect(unattachedPendingPrompts([earlier, unattached], new Set(["turn-1"]))).toEqual([unattached]);
  });
});
