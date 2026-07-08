// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import ChatComposerPanel from '../../chatInterface/ChatComposerPanel';
import type { TeamMemberWorkspaceProps } from './TeamMemberWorkspaceTypes';

type TeamMemberWorkspaceComposerProps = Pick<
  TeamMemberWorkspaceProps,
  | 'project'
  | 'selectedContact'
  | 'selectedProjectSession'
  | 'isSelectedSessionActive'
  | 'selectedModelId'
  | 'selectedModelName'
  | 'selectedThinkingLevel'
  | 'aiModelConfigs'
  | 'supportsReasoning'
  | 'reasoningEnabled'
  | 'planModeEnabled'
  | 'reviewRepairRunning'
  | 'availableRemoteConnections'
  | 'currentRemoteConnectionId'
  | 'onRemoteConnectionChange'
  | 'onSend'
  | 'onModelChange'
  | 'onModelNameChange'
  | 'onThinkingLevelChange'
  | 'onModelRuntimeChange'
  | 'onReasoningToggle'
  | 'onPlanModeToggle'
> & {
  supportedFileTypes: React.ComponentProps<typeof ChatComposerPanel>['supportedFileTypes'];
};

export const TeamMemberWorkspaceComposer: React.FC<TeamMemberWorkspaceComposerProps> = ({
  project,
  selectedContact,
  selectedProjectSession,
  isSelectedSessionActive,
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
  onSend,
  onModelChange,
  onModelNameChange,
  onThinkingLevelChange,
  onModelRuntimeChange,
  onReasoningToggle,
  onPlanModeToggle,
  supportedFileTypes,
}) => {
  if (!selectedContact) {
    return null;
  }

  return (
    <ChatComposerPanel
      onSend={onSend}
      inputDisabled={selectedProjectSession ? (!isSelectedSessionActive || reviewRepairRunning) : reviewRepairRunning}
      supportedFileTypes={supportedFileTypes}
      reasoningSupported={supportsReasoning}
      reasoningEnabled={reasoningEnabled}
      onReasoningToggle={onReasoningToggle}
      planModeAvailable={true}
      planModeEnabled={planModeEnabled}
      onPlanModeToggle={onPlanModeToggle}
      selectedModelId={selectedModelId}
      selectedModelName={selectedModelName}
      selectedThinkingLevel={selectedThinkingLevel}
      availableModels={aiModelConfigs}
      onModelChange={onModelChange}
      onModelNameChange={onModelNameChange}
      onThinkingLevelChange={onThinkingLevelChange}
      onModelRuntimeChange={onModelRuntimeChange}
      availableProjects={[project]}
      currentProject={project}
      selectedProjectId={project.id}
      onProjectChange={() => {}}
      showProjectSelector={false}
      showProjectFileButton={false}
      showWorkspaceRootPicker={false}
      availableRemoteConnections={availableRemoteConnections}
      currentRemoteConnectionId={currentRemoteConnectionId}
      onRemoteConnectionChange={onRemoteConnectionChange}
    />
  );
};
