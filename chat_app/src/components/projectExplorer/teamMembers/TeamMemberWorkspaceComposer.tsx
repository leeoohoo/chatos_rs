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
  | 'availableRemoteConnections'
  | 'currentRemoteConnectionId'
  | 'onRemoteConnectionChange'
  | 'onSend'
  | 'onModelChange'
  | 'onModelNameChange'
  | 'onThinkingLevelChange'
  | 'onModelRuntimeChange'
  | 'onReasoningToggle'
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
  availableRemoteConnections,
  currentRemoteConnectionId,
  onRemoteConnectionChange,
  onSend,
  onModelChange,
  onModelNameChange,
  onThinkingLevelChange,
  onModelRuntimeChange,
  onReasoningToggle,
  supportedFileTypes,
}) => {
  if (!selectedContact || !selectedProjectSession) {
    return null;
  }

  return (
    <ChatComposerPanel
      onSend={onSend}
      inputDisabled={!isSelectedSessionActive}
      supportedFileTypes={supportedFileTypes}
      reasoningSupported={supportsReasoning}
      reasoningEnabled={reasoningEnabled}
      onReasoningToggle={onReasoningToggle}
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
