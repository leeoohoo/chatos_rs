import React, { Suspense, lazy } from 'react';

import ChatInterfaceErrorBanner from './chatInterface/ChatInterfaceErrorBanner';
import HeaderBar from './chatInterface/HeaderBar';
import ChatInterfaceMainContent from './chatInterface/ChatInterfaceMainContent';
import ChatInterfaceOverlays from './chatInterface/ChatInterfaceOverlays';
import { useChatInterfaceModel } from './chatInterface/useChatInterfaceModel';
import { cn } from '../lib/utils';
import type { ChatInterfaceProps } from '../types';

const SystemContextEditor = lazy(() => import('./SystemContextEditor'));

export const ChatInterface: React.FC<ChatInterfaceProps> = ({
  className,
  onMessageSend,
  customRenderer,
}) => {
  const {
    user,
    logout,
    error,
    clearError,
    headerTitle,
    sidebarOpen,
    toggleSidebar,
    currentProject,
    activePanel,
    showSystemContextEditor,
    setShowSystemContextEditor,
    showMcpManager,
    setShowMcpManager,
    showAiModelManager,
    setShowAiModelManager,
    showNotepadPanel,
    setShowNotepadPanel,
    showUserSettings,
    setShowUserSettings,
    showApplicationsPanel,
    setShowApplicationsPanel,
    summaryPaneSessionId,
    runtimeContextOpen,
    runtimeContextSessionId,
    handleOpenRuntimeContext,
    handleClearSummaryPaneSelection,
    handleToggleSessionSummary,
    sessionListProps,
    conversationPaneProps,
    uiPromptHistoryProps,
    runtimeContextProps,
  } = useChatInterfaceModel({
    onMessageSend,
    customRenderer,
  });

  if (showSystemContextEditor) {
    return (
      <Suspense fallback={(
        <div className="flex h-screen items-center justify-center bg-background text-sm text-muted-foreground">
          系统提示词工作区加载中...
        </div>
      )}>
        <SystemContextEditor onClose={() => setShowSystemContextEditor(false)} />
      </Suspense>
    );
  }

  return (
    <div className={cn(
      'flex flex-col h-screen bg-background text-foreground',
      className,
    )}>
      <HeaderBar
        headerTitle={headerTitle}
        sidebarOpen={sidebarOpen}
        onToggleSidebar={toggleSidebar}
        onOpenNotepad={() => setShowNotepadPanel(true)}
        onOpenApplications={() => setShowApplicationsPanel(true)}
        onOpenMcpManager={() => setShowMcpManager(true)}
        onOpenAiModelManager={() => setShowAiModelManager(true)}
        onOpenSystemContextEditor={() => setShowSystemContextEditor(true)}
        onOpenUserSettings={() => setShowUserSettings(true)}
        onLogout={logout}
        user={user}
      />

      <ChatInterfaceErrorBanner error={error} onClear={clearError} />

      <ChatInterfaceMainContent
        activePanel={activePanel}
        sidebarOpen={sidebarOpen}
        summaryPaneSessionId={summaryPaneSessionId}
        runtimeContextOpen={runtimeContextOpen}
        runtimeContextSessionId={runtimeContextSessionId}
        currentProject={currentProject}
        onToggleSidebar={toggleSidebar}
        onSelectSession={handleClearSummaryPaneSelection}
        onToggleSessionSummary={handleToggleSessionSummary}
        onOpenSessionRuntimeContext={handleOpenRuntimeContext}
        sessionListProps={sessionListProps}
        conversationPaneProps={conversationPaneProps}
      />

      <ChatInterfaceOverlays
        uiPromptHistoryProps={uiPromptHistoryProps}
        runtimeContextProps={runtimeContextProps}
        showMcpManager={showMcpManager}
        setShowMcpManager={setShowMcpManager}
        showNotepadPanel={showNotepadPanel}
        setShowNotepadPanel={setShowNotepadPanel}
        showAiModelManager={showAiModelManager}
        setShowAiModelManager={setShowAiModelManager}
        showUserSettings={showUserSettings}
        setShowUserSettings={setShowUserSettings}
        showApplicationsPanel={showApplicationsPanel}
        setShowApplicationsPanel={setShowApplicationsPanel}
      />
    </div>
  );
};

export default ChatInterface;
