// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { useI18n } from '../i18n/I18nProvider';
import type { ChatStore as SessionListStoreHook } from '../lib/store/createChatStoreWithBackend';
import { cn } from '../lib/utils';
import { SessionListDialogs } from './sessionList/SessionListDialogs';
import {
  ProjectSection,
  RemoteSection,
  SessionSection,
  TerminalSection,
} from './sessionList/Sections';
import {
  formatTimeAgo,
  getSessionStatus,
} from './sessionList/helpers';
import { useSessionListController } from './sessionList/useSessionListController';

interface SessionListProps {
  isOpen?: boolean;
  onClose?: () => void;
  collapsed?: boolean;
  onToggleCollapse?: () => void;
  className?: string;
  store?: SessionListStoreHook;
  onSelectSession?: (sessionId: string) => void;
  onOpenSessionSummary?: (sessionId: string) => void;
  onOpenSessionRuntimeContext?: (sessionId: string) => void;
  activeSummarySessionId?: string | null;
  activeRuntimeContextSessionId?: string | null;
}

export const SessionList: React.FC<SessionListProps> = (props) => {
  const {
    isOpen = true,
    collapsed,
    className,
    store,
    onSelectSession,
    onOpenSessionSummary,
    onOpenSessionRuntimeContext,
    activeSummarySessionId,
    activeRuntimeContextSessionId,
  } = props;
  const { t, locale } = useI18n();
  const isCollapsed = collapsed ?? !isOpen;
  const controller = useSessionListController({
    store,
    activeSummarySessionId,
    onOpenSessionSummary,
    onOpenSessionRuntimeContext,
    isCollapsed,
  });
  const formatTimeAgoForLocale = React.useCallback(
    (date: string | Date | undefined | null) => formatTimeAgo(date, t, locale),
    [locale, t],
  );

  return (
    <div
      className={cn(
        'flex flex-col h-full bg-card transition-all duration-200 overflow-hidden',
        isCollapsed ? 'w-0' : 'w-64 sm:w-72 border-r border-border',
        className
      )}
    >
      {/* 联系人与项目列表 */}
      {!isCollapsed && (
        <div className="flex-1 flex flex-col overflow-hidden">
          <SessionSection
            expanded={controller.sectionExpansion.sessionsExpanded}
            sessions={controller.contactSessionState.displaySessions}
            currentSessionId={controller.contactSessionState.currentDisplaySessionId}
            summarySessionId={controller.contactSessionState.activeSummaryDisplaySessionId}
            runtimeContextSessionId={activeRuntimeContextSessionId}
            displaySessionRuntimeIdMap={controller.contactSessionState.displaySessionRuntimeIdMap}
            taskRunnerEnabledBySessionId={controller.contactSessionState.taskRunnerEnabledBySessionId}
            sessionChatState={controller.sessionChatState}
            hasMore={false}
            isRefreshing={controller.isRefreshing}
            isLoadingMore={false}
            onToggle={controller.sectionExpansion.handleToggleSessionsSection}
            onRefresh={controller.sessionListActions.handleRefreshSessions}
            onCreateSession={() => {
              void controller.contactSessionCreator.openCreateSessionModal();
            }}
            onSelectSession={(sessionId) => {
              void controller.sessionListActions.handleSelectSession(sessionId).then((selectedSessionId) => {
                if (selectedSessionId) {
                  onSelectSession?.(selectedSessionId);
                }
              });
            }}
            onOpenSummary={controller.sessionListActions.handleOpenSummary}
            onOpenRuntimeContext={controller.sessionListActions.handleOpenRuntimeContext}
            onOpenTaskRunnerConfig={controller.openTaskRunnerConfig}
            onDeleteSession={controller.deleteActions.handleDeleteSession}
            onLoadMore={() => {}}
            onToggleActionMenu={controller.inlineActionMenus.toggleActionMenu}
            closeActionMenus={() => controller.inlineActionMenus.closeActionMenus()}
            formatTimeAgo={formatTimeAgoForLocale}
            getSessionStatus={getSessionStatus}
          />

          <div className="my-2 border-t border-border" />

          <ProjectSection
            expanded={controller.sectionExpansion.projectsExpanded}
            projects={controller.projects}
            currentProjectId={controller.currentProject?.id}
            canCreate
            onToggle={controller.sectionExpansion.handleToggleProjectsSection}
            onCreate={controller.sessionListActions.openProjectModal}
            onSelect={(projectId) => {
              void controller.sessionListActions.handleSelectProject(projectId);
            }}
            onArchive={(projectId) => {
              void controller.deleteActions.handleArchiveProject(projectId);
            }}
            onToggleActionMenu={controller.inlineActionMenus.toggleActionMenu}
            closeActionMenus={() => controller.inlineActionMenus.closeActionMenus()}
          />

          {controller.workspaceResourceVisibility.showTerminalSection && (
            <>
              <div className="my-2 border-t border-border" />

              <TerminalSection
                expanded={controller.sectionExpansion.terminalsExpanded}
                terminals={controller.terminals}
                currentTerminalId={controller.currentTerminal?.id}
                isRefreshing={controller.isRefreshingTerminals}
                onToggle={controller.sectionExpansion.handleToggleTerminalsSection}
                onRefresh={controller.sessionListActions.handleRefreshTerminals}
                onCreate={controller.sessionListActions.openTerminalModal}
                onSelect={(terminalId) => {
                  void controller.sessionListActions.handleSelectTerminal(terminalId);
                }}
                onDelete={controller.deleteActions.handleDeleteTerminal}
                onToggleActionMenu={controller.inlineActionMenus.toggleActionMenu}
                closeActionMenus={() => controller.inlineActionMenus.closeActionMenus()}
                formatTimeAgo={formatTimeAgoForLocale}
              />
            </>
          )}

          {controller.workspaceResourceVisibility.showRemoteSection && (
            <>
              <div className="my-2 border-t border-border" />

              <RemoteSection
                expanded={controller.sectionExpansion.remoteExpanded}
                remoteConnections={controller.remoteConnections}
                currentRemoteConnectionId={controller.currentRemoteConnection?.id}
                isRefreshing={controller.isRefreshingRemote}
                onToggle={controller.sectionExpansion.handleToggleRemoteSection}
                onRefresh={controller.sessionListActions.handleRefreshRemote}
                onCreate={controller.sessionListActions.openRemoteModal}
                onSelect={(connectionId) => {
                  void controller.sessionListActions.handleSelectRemoteConnection(connectionId);
                }}
                onOpenSftp={(connectionId) => {
                  void controller.sessionListActions.handleOpenRemoteSftp(connectionId);
                }}
                onEdit={(connection) => {
                  controller.localFsPickers.setKeyFilePickerOpen(false);
                  controller.remoteForm.openEditRemoteModal(connection);
                }}
                onTest={controller.remoteForm.handleQuickTestRemoteConnection}
                onDelete={controller.deleteActions.handleDeleteRemoteConnection}
                onToggleActionMenu={controller.inlineActionMenus.toggleActionMenu}
                closeActionMenus={() => controller.inlineActionMenus.closeActionMenus()}
                formatTimeAgo={formatTimeAgoForLocale}
              />
            </>
          )}
        </div>
      )}
      <SessionListDialogs
        createContactModalOpen={controller.contactSessionCreator.createContactModalOpen}
        agents={controller.agents || []}
        existingContactAgentIds={controller.existingContactAgentIds}
        selectedContactAgentId={controller.contactSessionCreator.selectedContactAgentId}
        contactError={controller.contactSessionCreator.contactError}
        closeCreateSessionModal={controller.contactSessionCreator.closeCreateSessionModal}
        setSelectedContactAgentId={controller.contactSessionCreator.setSelectedContactAgentId}
        setContactError={controller.contactSessionCreator.setContactError}
        handleCreateContactSession={controller.contactSessionCreator.handleCreateContactSession}
        taskRunnerContact={controller.taskRunnerContact}
        taskRunnerAgentAccounts={controller.taskRunnerAgentAccounts}
        taskRunnerAgentAccountsLoading={controller.taskRunnerAgentAccountsLoading}
        taskRunnerError={controller.taskRunnerError}
        taskRunnerSaving={controller.taskRunnerSaving}
        closeTaskRunnerConfig={controller.closeTaskRunnerConfig}
        saveTaskRunnerConfig={controller.saveTaskRunnerConfig}
        projectModalOpen={controller.projectModalOpen}
        projectRoot={controller.projectRoot}
        cloudProjectName={controller.cloudProjectName}
        cloudProjectGitUrl={controller.cloudProjectGitUrl}
        cloudProjectZipFile={controller.cloudProjectZipFile}
        projectError={controller.projectError}
        projectSourceMode={controller.projectSourceMode}
        allowLocalProjectCreation={controller.allowLocalProjectCreation}
        localConnectorWorkspaces={controller.localConnectorWorkspaces}
        localConnectorLoading={controller.localConnectorLoading}
        localConnectorError={controller.localConnectorError}
        localConnectorDirectoryPath={controller.localConnectorDirectoryPath}
        localConnectorDirectoryParent={controller.localConnectorDirectoryParent}
        localConnectorDirectoryEntries={controller.localConnectorDirectoryEntries}
        localConnectorDirectoryLoading={controller.localConnectorDirectoryLoading}
        localConnectorDirectoryError={controller.localConnectorDirectoryError}
        selectedLocalConnectorDirectoryPath={controller.selectedLocalConnectorDirectoryPath}
        selectedLocalConnectorWorkspaceId={controller.selectedLocalConnectorWorkspaceId}
        setProjectModalOpen={controller.setProjectModalOpen}
        setProjectSourceMode={controller.setProjectSourceMode}
        setProjectRoot={controller.setProjectRoot}
        setCloudProjectName={controller.setCloudProjectName}
        setCloudProjectGitUrl={controller.setCloudProjectGitUrl}
        setCloudProjectZipFile={controller.setCloudProjectZipFile}
        openDirPickerForProject={() => {
          void controller.localFsPickers.openDirPicker('project');
        }}
        refreshLocalConnectorWorkspaces={controller.refreshLocalConnectorWorkspaces}
        setSelectedLocalConnectorWorkspaceId={controller.setSelectedLocalConnectorWorkspaceId}
        browseLocalConnectorDirectory={controller.browseLocalConnectorDirectory}
        setSelectedLocalConnectorDirectoryPath={controller.setSelectedLocalConnectorDirectoryPath}
        createLocalConnectorDirectory={controller.createLocalConnectorDirectory}
        handleCreateProject={controller.sessionListActions.handleCreateProject}
        terminalModalOpen={controller.terminalModalOpen}
        terminalError={controller.terminalError}
        terminalExecuting={controller.terminalExecuting}
        setTerminalModalOpen={controller.setTerminalModalOpen}
        handleCreateTerminal={controller.sessionListActions.handleCreateTerminal}
        remoteModalOpen={controller.remoteForm.remoteModalOpen}
        editingRemoteConnectionId={controller.remoteForm.editingRemoteConnectionId}
        remoteConnections={controller.remoteConnections}
        remoteName={controller.remoteForm.remoteName}
        remoteHost={controller.remoteForm.remoteHost}
        remotePort={controller.remoteForm.remotePort}
        remoteUsername={controller.remoteForm.remoteUsername}
        remoteAuthType={controller.remoteForm.remoteAuthType}
        remotePassword={controller.remoteForm.remotePassword}
        remotePrivateKeyPath={controller.remoteForm.remotePrivateKeyPath}
        remoteCertificatePath={controller.remoteForm.remoteCertificatePath}
        remoteDefaultPath={controller.remoteForm.remoteDefaultPath}
        remoteHostKeyPolicy={controller.remoteForm.remoteHostKeyPolicy}
        remoteJumpEnabled={controller.remoteForm.remoteJumpEnabled}
        remoteJumpMode={controller.remoteForm.remoteJumpMode}
        remoteJumpConnectionId={controller.remoteForm.remoteJumpConnectionId}
        remoteJumpHost={controller.remoteForm.remoteJumpHost}
        remoteJumpPort={controller.remoteForm.remoteJumpPort}
        remoteJumpUsername={controller.remoteForm.remoteJumpUsername}
        remoteJumpPrivateKeyPath={controller.remoteForm.remoteJumpPrivateKeyPath}
        remoteJumpCertificatePath={controller.remoteForm.remoteJumpCertificatePath}
        remoteJumpPassword={controller.remoteForm.remoteJumpPassword}
        remoteError={controller.remoteForm.remoteError}
        remoteErrorAction={controller.remoteForm.remoteErrorAction}
        remoteSuccess={controller.remoteForm.remoteSuccess}
        remoteTesting={controller.remoteForm.remoteTesting}
        remoteSaving={controller.remoteForm.remoteSaving}
        remoteVerificationModalOpen={controller.remoteForm.remoteVerificationModalOpen}
        remoteVerificationPrompt={controller.remoteForm.remoteVerificationPrompt}
        remoteVerificationCode={controller.remoteForm.remoteVerificationCode}
        setRemoteModalOpen={controller.remoteForm.setRemoteModalOpen}
        setRemoteName={controller.remoteForm.setRemoteName}
        setRemoteHost={controller.remoteForm.setRemoteHost}
        setRemotePort={controller.remoteForm.setRemotePort}
        setRemoteUsername={controller.remoteForm.setRemoteUsername}
        setRemoteAuthType={controller.remoteForm.setRemoteAuthType}
        setRemotePassword={controller.remoteForm.setRemotePassword}
        setRemotePrivateKeyPath={controller.remoteForm.setRemotePrivateKeyPath}
        setRemoteCertificatePath={controller.remoteForm.setRemoteCertificatePath}
        setRemoteDefaultPath={controller.remoteForm.setRemoteDefaultPath}
        setRemoteHostKeyPolicy={controller.remoteForm.setRemoteHostKeyPolicy}
        setRemoteJumpEnabled={controller.remoteForm.setRemoteJumpEnabled}
        setRemoteJumpMode={controller.remoteForm.setRemoteJumpMode}
        setRemoteJumpConnectionId={controller.remoteForm.setRemoteJumpConnectionId}
        setRemoteJumpHost={controller.remoteForm.setRemoteJumpHost}
        setRemoteJumpPort={controller.remoteForm.setRemoteJumpPort}
        setRemoteJumpUsername={controller.remoteForm.setRemoteJumpUsername}
        setRemoteJumpPrivateKeyPath={controller.remoteForm.setRemoteJumpPrivateKeyPath}
        setRemoteJumpCertificatePath={controller.remoteForm.setRemoteJumpCertificatePath}
        setRemoteJumpPassword={controller.remoteForm.setRemoteJumpPassword}
        setRemoteVerificationCode={controller.remoteForm.setRemoteVerificationCode}
        setRemoteVerificationModalOpen={controller.remoteForm.setRemoteVerificationModalOpen}
        openKeyFilePicker={controller.localFsPickers.openKeyFilePicker}
        handleTestRemoteConnection={controller.remoteForm.handleTestRemoteConnection}
        handleSaveRemoteConnection={controller.remoteForm.handleSaveRemoteConnection}
        handleSubmitRemoteVerification={controller.remoteForm.handleSubmitRemoteVerification}
        keyFilePickerOpen={controller.localFsPickers.keyFilePickerOpen}
        keyFilePickerTitle={controller.localFsPickers.keyFilePickerTitle}
        keyFilePickerPath={controller.localFsPickers.keyFilePickerPath}
        keyFilePickerParent={controller.localFsPickers.keyFilePickerParent}
        keyFilePickerLoading={controller.localFsPickers.keyFilePickerLoading}
        keyFilePickerItems={controller.localFsPickers.keyFilePickerItems}
        keyFilePickerError={controller.localFsPickers.keyFilePickerError}
        closeKeyFilePicker={controller.localFsPickers.closeKeyFilePicker}
        loadKeyFileEntries={controller.localFsPickers.loadKeyFileEntries}
        applySelectedKeyFile={controller.localFsPickers.applySelectedKeyFile}
        dirPickerOpen={controller.localFsPickers.dirPickerOpen}
        dirPickerPath={controller.localFsPickers.dirPickerPath}
        dirPickerParent={controller.localFsPickers.dirPickerParent}
        dirPickerWritable={controller.localFsPickers.dirPickerWritable}
        dirPickerLoading={controller.localFsPickers.dirPickerLoading}
        dirPickerItems={controller.localFsPickers.dirPickerItems}
        dirPickerError={controller.localFsPickers.dirPickerError}
        showHiddenDirs={controller.localFsPickers.showHiddenDirs}
        dirPickerCreateModalOpen={controller.localFsPickers.dirPickerCreateModalOpen}
        dirPickerNewFolderName={controller.localFsPickers.dirPickerNewFolderName}
        dirPickerCreatingFolder={controller.localFsPickers.dirPickerCreatingFolder}
        closeDirPicker={controller.localFsPickers.closeDirPicker}
        chooseDir={controller.localFsPickers.chooseDir}
        openCreateDirModal={controller.localFsPickers.openCreateDirModal}
        setShowHiddenDirs={controller.localFsPickers.setShowHiddenDirs}
        loadDirEntries={controller.localFsPickers.loadDirEntries}
        setDirPickerCreateModalOpen={controller.localFsPickers.setDirPickerCreateModalOpen}
        setDirPickerNewFolderName={controller.localFsPickers.setDirPickerNewFolderName}
        createDirInPicker={controller.localFsPickers.createDirInPicker}
      />
    </div>
  );
};
