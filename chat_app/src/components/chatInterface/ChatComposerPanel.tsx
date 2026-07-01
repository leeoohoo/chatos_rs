// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { InputArea } from '../InputArea';
import type {
  AiModelConfig,
  Project,
  RemoteConnection,
  SendMessageHandler,
} from '../../types';
import { useI18n } from '../../i18n/I18nProvider';

interface ChatComposerPanelProps {
  onSend: SendMessageHandler;
  inputDisabled: boolean;
  supportedFileTypes: string[];
  reasoningSupported: boolean;
  reasoningEnabled: boolean;
  onReasoningToggle: (enabled: boolean) => void;
  planModeAvailable?: boolean;
  planModeEnabled: boolean;
  onPlanModeToggle: (enabled: boolean) => void;
  selectedModelId: string | null;
  selectedModelName?: string | null;
  selectedThinkingLevel?: string | null;
  availableModels: AiModelConfig[];
  onModelChange: (modelId: string | null) => void;
  onModelNameChange?: (modelName: string | null) => void;
  onThinkingLevelChange?: (level: string | null) => void;
  onModelRuntimeChange?: (selection: {
    selectedModelId?: string | null;
    selectedModelName?: string | null;
    selectedThinkingLevel?: string | null;
  }) => void;
  availableProjects: Project[];
  currentProject: Project | null;
  selectedProjectId: string | null;
  onProjectChange: (projectId: string | null) => void;
  showProjectSelector?: boolean;
  showProjectFileButton?: boolean;
  workspaceRoot?: string | null;
  onWorkspaceRootChange?: (path: string | null) => void;
  currentRemoteConnectionId?: string | null;
  availableRemoteConnections?: RemoteConnection[];
  onRemoteConnectionChange?: (connectionId: string | null) => void;
  showWorkspaceRootPicker?: boolean;
}

const ChatComposerPanel: React.FC<ChatComposerPanelProps> = ({
  onSend,
  inputDisabled,
  supportedFileTypes,
  reasoningSupported,
  reasoningEnabled,
  onReasoningToggle,
  planModeAvailable = false,
  planModeEnabled,
  onPlanModeToggle,
  selectedModelId,
  selectedModelName = null,
  selectedThinkingLevel = null,
  availableModels,
  onModelChange,
  onModelNameChange,
  onThinkingLevelChange,
  onModelRuntimeChange,
  availableProjects,
  currentProject,
  selectedProjectId,
  onProjectChange,
  showProjectSelector = true,
  showProjectFileButton = true,
  workspaceRoot = null,
  onWorkspaceRootChange,
  currentRemoteConnectionId = null,
  availableRemoteConnections = [],
  onRemoteConnectionChange,
  showWorkspaceRootPicker = false,
}) => {
  const { t } = useI18n();

  return (
  <div className="border-t border-border">
    <InputArea
      onSend={onSend}
      disabled={inputDisabled}
      placeholder={t('chat.inputPlaceholder')}
      allowAttachments={true}
      supportedFileTypes={supportedFileTypes}
      reasoningSupported={reasoningSupported}
      reasoningEnabled={reasoningEnabled}
      onReasoningToggle={onReasoningToggle}
      planModeAvailable={planModeAvailable}
      planModeEnabled={planModeAvailable && planModeEnabled}
      onPlanModeToggle={onPlanModeToggle}
      showModelSelector={true}
      selectedModelId={selectedModelId}
      selectedModelName={selectedModelName}
      selectedThinkingLevel={selectedThinkingLevel}
      availableModels={availableModels}
      onModelChange={onModelChange}
      onModelNameChange={onModelNameChange}
      onThinkingLevelChange={onThinkingLevelChange}
      onModelRuntimeChange={onModelRuntimeChange}
      availableProjects={availableProjects}
      currentProject={currentProject}
      selectedProjectId={selectedProjectId}
      onProjectChange={onProjectChange}
      showProjectSelector={showProjectSelector}
      showProjectFileButton={showProjectFileButton}
      workspaceRoot={workspaceRoot}
      onWorkspaceRootChange={onWorkspaceRootChange}
      currentRemoteConnectionId={currentRemoteConnectionId}
      availableRemoteConnections={availableRemoteConnections}
      onRemoteConnectionChange={onRemoteConnectionChange}
      showWorkspaceRootPicker={showWorkspaceRootPicker}
    />
  </div>
  );
};

export default ChatComposerPanel;
