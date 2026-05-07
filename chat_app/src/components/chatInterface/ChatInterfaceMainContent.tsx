import { Suspense, lazy, type ComponentProps } from 'react';

import ChatConversationPane from './ChatConversationPane';
import { SessionList } from '../SessionList';
import type { Project } from '../../types';

const ProjectExplorer = lazy(() => import('../ProjectExplorer'));
const TerminalView = lazy(() => import('../TerminalView'));
const RemoteTerminalView = lazy(() => import('../RemoteTerminalView'));
const RemoteSftpPanel = lazy(() => import('../RemoteSftpPanel'));

interface ChatInterfaceMainContentProps {
  activePanel: string;
  sidebarOpen: boolean;
  summaryPaneSessionId: string | null;
  runtimeContextOpen: boolean;
  runtimeContextSessionId: string | null;
  currentProject: Project | null;
  onToggleSidebar: () => void;
  onSelectSession: () => void;
  onToggleSessionSummary: (sessionId: string) => void;
  onOpenSessionRuntimeContext: (sessionId: string) => void;
  sessionListProps: ComponentProps<typeof SessionList>;
  conversationPaneProps: ComponentProps<typeof ChatConversationPane>;
}

const LazyPanelFallback = () => (
  <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
    面板加载中...
  </div>
);

export default function ChatInterfaceMainContent({
  activePanel,
  sidebarOpen,
  summaryPaneSessionId,
  runtimeContextOpen,
  runtimeContextSessionId,
  currentProject,
  onToggleSidebar,
  onSelectSession,
  onToggleSessionSummary,
  onOpenSessionRuntimeContext,
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
        onOpenSessionRuntimeContext={onOpenSessionRuntimeContext}
        activeSummarySessionId={summaryPaneSessionId}
        activeRuntimeContextSessionId={runtimeContextOpen ? runtimeContextSessionId : null}
      />

      <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
        <Suspense fallback={<LazyPanelFallback />}>
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
        </Suspense>
      </div>
    </div>
  );
}
