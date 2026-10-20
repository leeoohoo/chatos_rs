import type { AskUserPort } from "../../ask-user/ports/AskUserPort";
import type { ConversationAttachmentPort } from "../../conversation-attachments/ports/ConversationAttachmentPort";
import type { ConversationComposerPort } from "../../conversation-composer/ports/ConversationComposerPort";
import type { ConversationHistoryPort } from "../../conversation-history/ports/ConversationHistoryPort";
import { ConversationHistoryView } from "../../conversation-history/ui/ConversationHistoryView";
import type { ConversationRealtimePort } from "../../conversation-realtime/ports/ConversationRealtimePort";
import type { ConversationRuntimeSettingsPort } from "../../conversation-runtime-settings/ports/ConversationRuntimeSettingsPort";
import type { RunInspectorPort } from "../../run-inspector/ports/RunInspectorPort";
import type { WorkspaceResourceItem } from "../../workspace-index/domain/workspaceIndex";

const pluginInstallerContactId = "plugin-installer";

export function ContactConversationView({
  ownerUserId,
  contact,
  historyPort,
  composerPort,
  attachmentPort,
  realtimePort,
  runtimeSettingsPort,
  runInspectorPort,
  askUserPort,
}: {
  ownerUserId: string;
  contact: WorkspaceResourceItem;
  historyPort: ConversationHistoryPort;
  composerPort: ConversationComposerPort;
  attachmentPort: ConversationAttachmentPort;
  realtimePort: ConversationRealtimePort;
  runtimeSettingsPort: ConversationRuntimeSettingsPort;
  runInspectorPort: RunInspectorPort;
  askUserPort: AskUserPort;
}) {
  if (!contact.conversationId) {
    return (
      <div className="project-state">
        <div className="project-state__card">
          <div className="project-state__icon" aria-hidden="true">◇</div>
          <h2>还没有公开会话</h2>
          <p>发送第一条消息后，会话会显示在这里。</p>
        </div>
      </div>
    );
  }

  return (
    <div className="contact-conversation" aria-label={`${contact.title} 联系人会话`}>
      {contact.id === pluginInstallerContactId && (
        <div className="contact-conversation__installer-note" role="note">
          <span aria-hidden="true">◆</span>
          <p>直接告诉我想安装的 MCP、npm 包名或本地 Plugin 文件夹；安装能力只在此对话可用。</p>
        </div>
      )}
      <ConversationHistoryView
        askUserPort={askUserPort}
        attachmentPort={attachmentPort}
        composerPort={composerPort}
        conversationId={contact.conversationId}
        ownerUserId={ownerUserId}
        port={historyPort}
        realtimePort={realtimePort}
        runInspectorPort={runInspectorPort}
        runtimeSettingsPort={runtimeSettingsPort}
      />
    </div>
  );
}
