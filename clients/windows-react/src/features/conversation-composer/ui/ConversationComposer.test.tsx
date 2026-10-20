import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ConversationHistorySnapshot } from "../../conversation-history/domain/conversationHistory";
import { useConversationRuntimeSettingsStore } from "../../conversation-runtime-settings/application/conversationRuntimeSettingsStore";
import type { ConversationRuntimeSettingsPort } from "../../conversation-runtime-settings/ports/ConversationRuntimeSettingsPort";
import { useConversationComposerStore } from "../application/conversationComposerStore";
import type { ConversationComposerPort } from "../ports/ConversationComposerPort";
import { ConversationComposer } from "./ConversationComposer";

const history: ConversationHistorySnapshot = {
  conversation: {
    conversation_id: "conversation-1",
    owner_user_id: "owner-1",
    title: "Project",
    resource: null,
    version: 1,
    created_at_unix_ms: 1,
    updated_at_unix_ms: 1,
  },
  turns: [],
  messages: [],
  attachments: [],
  next_before_ordinal: null,
};

const runtimeSettingsPort: ConversationRuntimeSettingsPort = {
  load: async () => ({
    settings: {
      selectedModelId: "model-1",
      selectedThinkingLevel: "none",
      remoteConnectionId: null,
      reasoningEnabled: false,
      version: 1,
    },
    models: [{
      id: "model-1",
      displayName: "Model 1",
      modelName: "gpt-5",
      provider: "openai",
      defaultThinkingLevel: "none",
      supportsReasoning: false,
      thinkingLevels: [],
    }],
    remoteConnections: [],
  }),
  selectModel: vi.fn(),
  selectReasoningLevel: vi.fn(),
  selectRemoteConnection: vi.fn(),
};

describe("ConversationComposer capability selection", () => {
  afterEach(cleanup);

  beforeEach(() => {
    useConversationComposerStore.getState().reset("owner-1", "conversation-1");
    useConversationRuntimeSettingsStore.getState().reset();
  });

  it("selects the highlighted capability with Enter and exposes a removable chip", async () => {
    renderComposer();
    await screen.findByLabelText("模型");
    const input = screen.getByLabelText("消息");

    fireEvent.change(input, { target: { value: "请先分析 /需求" } });
    expect(screen.getByRole("listbox", { name: "能力建议" })).toBeVisible();
    expect(screen.getByRole("option", { name: /需求调研/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    fireEvent.keyDown(input, { key: "Enter" });

    expect(input).toHaveValue("请先分析 ");
    expect(screen.getByLabelText("已选择能力：需求调研")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "移除所选能力：需求调研" }));
    expect(screen.queryByLabelText("已选择能力：需求调研")).not.toBeInTheDocument();
  });

  it("dismisses suggestions with Escape and reopens them after the draft changes", async () => {
    renderComposer();
    await screen.findByLabelText("模型");
    const input = screen.getByLabelText("消息");

    fireEvent.change(input, { target: { value: "/" } });
    fireEvent.keyDown(input, { key: "Escape" });
    expect(screen.queryByRole("listbox", { name: "能力建议" })).not.toBeInTheDocument();
    fireEvent.change(input, { target: { value: "/需" } });
    expect(screen.getByRole("listbox", { name: "能力建议" })).toBeVisible();
  });

  it("forwards the selected capability in the next Turn submission", async () => {
    const sendDraft = vi.fn().mockResolvedValue({
      deliveryKind: "newTurn",
      turnId: "turn-1",
      userMessageId: "message-1",
    });
    renderComposer({ sendDraft });
    await screen.findByLabelText("模型");
    const input = screen.getByLabelText("消息");

    fireEvent.change(input, { target: { value: "请先分析 /" } });
    fireEvent.keyDown(input, { key: "Tab" });
    fireEvent.click(screen.getByRole("button", { name: "发送" }));

    await waitFor(() => expect(sendDraft).toHaveBeenCalledWith(expect.objectContaining({
      submission: {
        content: "请先分析",
        explicitCapabilitySelection: {
          kind: "requirement_survey",
          displayName: "需求调研",
        },
      },
    })));
  });
});

function renderComposer(overrides: Partial<ConversationComposerPort> = {}) {
  const port: ConversationComposerPort = {
    sendDraft: vi.fn().mockResolvedValue({
      deliveryKind: "newTurn",
      turnId: "turn-1",
      userMessageId: "message-1",
    }),
    ...overrides,
  };
  return render(
    <ConversationComposer
      conversationId="conversation-1"
      history={history}
      onAccepted={async () => undefined}
      ownerUserId="owner-1"
      port={port}
      runtimeSettingsPort={runtimeSettingsPort}
    />,
  );
}
