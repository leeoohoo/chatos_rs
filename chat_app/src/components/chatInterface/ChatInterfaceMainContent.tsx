import type { ComponentProps } from 'react';

import ChatConversationPane from './ChatConversationPane';
import { SessionList } from '../SessionList';
import ProjectExplorer from '../ProjectExplorer';
import TerminalView from '../TerminalView';
import RemoteTerminalView from '../RemoteTerminalView';
import RemoteSftpPanel from '../RemoteSftpPanel';
import type { Project } from '../../types';

interface ChatInterfaceMainContentProps {
  activePanel: string;
  sidebarOpen: boolean;
  summaryPaneSessionId: string | null;
  currentProject: Project | null;
  onToggleSidebar: () => void;
  onSelectSession: () => void;
  onToggleSessionSummary: (sessionId: string) => void;
  sessionListProps: ComponentProps<typeof SessionList>;
  conversationPaneProps: ComponentProps<typeof ChatConversationPane>;
}

export default function ChatInterfaceMainContent({
  activePanel,
  sidebarOpen,
  summaryPaneSessionId,
  currentProject,
  onToggleSidebar,
  onSelectSession,
  onToggleSessionSummary,
  sessionListProps,
  conversationPaneProps,
}: ChatInterfaceMainContentProps) {
  return (
    <div className="flex flex-1 min-h-0 overflow-hidden">
      <SessionList
        {...sessionListProps}
        collapsed={!sidebarOpen}
        onToggleCollapse={onToggleSidebar}
        onSelectSession={onSelectSession}
        onOpenSessionSummary={onToggleSessionSummary}
        activeSummarySessionId={summaryPaneSessionId}
      />

      <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
        {activePanel === 'project' ? (
          <ProjectExplorer project={currentProject} className="flex-1" />
        ) : activePanel === 'terminal' ? (
          <TerminalView className="flex-1" />
        ) : activePanel === 'remote_terminal' ? (
          <RemoteTerminalView className="flex-1" />
        ) : activePanel === 'remote_sftp' ? (
          <RemoteSftpPanel className="flex-1" />
        ) : (
          <ChatConversationPane {...conversationPaneProps} />
        )}
      </div>
    </div>
  );
}
