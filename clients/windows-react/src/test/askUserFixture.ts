import type { AskUserPrompt } from "../features/ask-user/domain/askUser";

export function sampleAskUserPrompt(): AskUserPrompt {
  return {
    id: "local-ask:run-1",
    sessionId: "conversation-1",
    turnId: "turn-1",
    toolCallId: "call-1",
    kind: "mixed",
    status: "pending",
    title: "Need input",
    message: "Please choose",
    allowsCancel: true,
    timeoutMilliseconds: null,
    fields: [
      { key: "name", label: "Name", defaultValue: "", isRequired: true, isMultiline: false, isSecret: false },
      { key: "password", label: "Password", defaultValue: "", isRequired: false, isMultiline: false, isSecret: true },
    ],
    choice: {
      allowsMultiple: true,
      options: [{ value: "one", label: "One" }, { value: "two", label: "Two" }],
      defaultSelection: [],
      minimumSelectionCount: 1,
      maximumSelectionCount: 1,
    },
    createdAtUnixMs: 10,
    updatedAtUnixMs: 20,
  };
}
