// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useMemo } from 'react';

import ConversationAskUserPromptPanel from '../../chatInterface/ConversationAskUserPromptPanel';
import { buildSupportedFileTypes, resolveModelSupportFlags } from '../../chatInterface/viewHelpers';
import { TeamMemberWorkspaceComposer } from './TeamMemberWorkspaceComposer';
import { TeamMemberWorkspaceContent } from './TeamMemberWorkspaceContent';
import type { TeamMemberWorkspaceProps } from './TeamMemberWorkspaceTypes';

const TeamMemberWorkspace: React.FC<TeamMemberWorkspaceProps> = ({
  project,
  selectedContact,
  selectedProjectSession,
  isSelectedSessionActive,
  sessionSummaryPaneVisible,
  summaryItems,
  summaryLoading,
  summaryError,
  clearingSummaries,
  deletingSummaryId,
  messages,
  hasMoreMessages,
  anchorMessageId,
  anchorRequestKey,
  onAnchorClear,
  selectedModelId,
  selectedModelName,
  selectedThinkingLevel,
  aiModelConfigs,
  supportsReasoning,
  reasoningEnabled,
  planModeEnabled,
  reviewRepairRunning,
  availableRemoteConnections,
  currentRemoteConnectionId,
  onRemoteConnectionChange,
  onLoadMore,
  onClearSummaries,
  onRefreshSummaries,
  onCloseSummary,
  onDeleteSummary,
  onSend,
  onModelChange,
  onModelNameChange,
  onThinkingLevelChange,
  onModelRuntimeChange,
  onReasoningToggle,
  onPlanModeToggle,
}) => {
  const { supportsImages } = useMemo(
    () => resolveModelSupportFlags(selectedModelId, aiModelConfigs),
    [aiModelConfigs, selectedModelId],
  );

  const supportedFileTypes = useMemo(
    () => buildSupportedFileTypes(supportsImages),
    [supportsImages],
  );

  return (
    <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
      <div className="flex-1 overflow-hidden">
        <TeamMemberWorkspaceContent
          selectedContact={selectedContact}
          selectedProjectSession={selectedProjectSession}
          isSelectedSessionActive={isSelectedSessionActive}
          sessionSummaryPaneVisible={sessionSummaryPaneVisible}
          summaryItems={summaryItems}
          summaryLoading={summaryLoading}
          summaryError={summaryError}
          clearingSummaries={clearingSummaries}
          deletingSummaryId={deletingSummaryId}
          messages={messages}
          hasMoreMessages={hasMoreMessages}
          anchorMessageId={anchorMessageId}
          anchorRequestKey={anchorRequestKey}
          onAnchorClear={onAnchorClear}
          onLoadMore={onLoadMore}
          onClearSummaries={onClearSummaries}
          onRefreshSummaries={onRefreshSummaries}
          onCloseSummary={onCloseSummary}
          onDeleteSummary={onDeleteSummary}
        />
      </div>

      <ConversationAskUserPromptPanel
        sessionId={
          selectedContact && selectedProjectSession && isSelectedSessionActive
            ? selectedProjectSession.id
            : null
        }
        projectId={project.id || null}
      />

      <TeamMemberWorkspaceComposer
        project={project}
        selectedContact={selectedContact}
        selectedProjectSession={selectedProjectSession}
        isSelectedSessionActive={isSelectedSessionActive}
        selectedModelId={selectedModelId}
        selectedModelName={selectedModelName}
        selectedThinkingLevel={selectedThinkingLevel}
        aiModelConfigs={aiModelConfigs}
        supportsReasoning={supportsReasoning}
        reasoningEnabled={reasoningEnabled}
        planModeEnabled={planModeEnabled}
        reviewRepairRunning={reviewRepairRunning}
        availableRemoteConnections={availableRemoteConnections}
        currentRemoteConnectionId={currentRemoteConnectionId}
        onRemoteConnectionChange={onRemoteConnectionChange}
        onSend={onSend}
        onModelChange={onModelChange}
        onModelNameChange={onModelNameChange}
        onThinkingLevelChange={onThinkingLevelChange}
        onModelRuntimeChange={onModelRuntimeChange}
        onReasoningToggle={onReasoningToggle}
        onPlanModeToggle={onPlanModeToggle}
        supportedFileTypes={supportedFileTypes}
      />
    </div>
  );
};

export default TeamMemberWorkspace;
