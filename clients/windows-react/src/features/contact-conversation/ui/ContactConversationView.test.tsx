import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AskUserPort } from "../../ask-user/ports/AskUserPort";
import type { ConversationAttachmentPort } from "../../conversation-attachments/ports/ConversationAttachmentPort";
import type { ConversationComposerPort } from "../../conversation-composer/ports/ConversationComposerPort";
import type { ConversationHistoryPort } from "../../conversation-history/ports/ConversationHistoryPort";
import type { ConversationRealtimePort } from "../../conversation-realtime/ports/ConversationRealtimePort";
import type { ConversationRuntimeSettingsPort } from "../../conversation-runtime-settings/ports/ConversationRuntimeSettingsPort";
import type { RunInspectorPort } from "../../run-inspector/ports/RunInspectorPort";
import type { WorkspaceResourceItem } from "../../workspace-index/domain/workspaceIndex";
import { ContactConversationView } from "./ContactConversationView";

vi.mock("../../conversation-history/ui/ConversationHistoryView", () => ({
  ConversationHistoryView: ({ ownerUserId, conversationId }: { ownerUserId: string; conversationId: string }) => (
    <div data-testid="conversation-surface">{ownerUserId}:{conversationId}</div>
  ),
}));

describe("ContactConversationView", () => {
  afterEach(cleanup);

  it("reuses the admitted Conversation surface for a contact", () => {
    renderView({ id: "jiguli", title: "叽咕狸", subtitle: null, conversationId: "conversation-1" });

    expect(screen.getByLabelText("叽咕狸 联系人会话")).toBeInTheDocument();
    expect(screen.getByTestId("conversation-surface")).toHaveTextContent("owner-1:conversation-1");
    expect(screen.queryByRole("note")).not.toBeInTheDocument();
  });

  it("shows the macOS capability-isolation notice only for the plugin installer", () => {
    renderView({ id: "plugin-installer", title: "插件安装助手", subtitle: null, conversationId: "conversation-2" });

    expect(screen.getByRole("note")).toHaveTextContent("安装能力只在此对话可用");
    expect(screen.getByTestId("conversation-surface")).toHaveTextContent("owner-1:conversation-2");
  });

  it("shows the macOS empty state when no public conversation exists", () => {
    renderView({ id: "contact-3", title: "联系人", subtitle: null, conversationId: null });

    expect(screen.getByRole("heading", { name: "还没有公开会话" })).toBeInTheDocument();
    expect(screen.queryByTestId("conversation-surface")).not.toBeInTheDocument();
  });
});

const ports = {
  historyPort: {} as ConversationHistoryPort,
  composerPort: {} as ConversationComposerPort,
  attachmentPort: {} as ConversationAttachmentPort,
  realtimePort: {} as ConversationRealtimePort,
  runtimeSettingsPort: {} as ConversationRuntimeSettingsPort,
  runInspectorPort: {} as RunInspectorPort,
  askUserPort: {} as AskUserPort,
};

function renderView(contact: WorkspaceResourceItem) {
  return render(<ContactConversationView ownerUserId="owner-1" contact={contact} {...ports} />);
}
