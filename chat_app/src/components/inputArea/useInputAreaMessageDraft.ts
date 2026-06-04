import {
  useCallback,
  useRef,
  useState,
  type ChangeEvent,
  type KeyboardEvent,
} from 'react';

import type { InputAreaProps, Project } from '../../types';

interface UseInputAreaMessageDraftOptions {
  attachments: File[];
  clearAttachments: () => void;
  currentRemoteConnectionId: string | null;
  disabled: boolean;
  effectiveAllowAttachments: boolean;
  isGuidingMode: boolean;
  maxLength: number;
  mcpEnabled: boolean;
  autoCreateTask: boolean;
  normalizedWorkspaceRoot: string | null;
  onGuide?: InputAreaProps['onGuide'];
  onSend: InputAreaProps['onSend'];
  requireModelSelection: () => boolean;
  sanitizedEnabledMcpIds: string[];
  selectedRuntimeProject: Project | null;
  selectedModelId: string | null;
  selectedSkillIds: string[];
  effectiveModelName: string | null;
  effectiveThinkingLevel: string | null;
  skillsEnabled: boolean;
}

export const useInputAreaMessageDraft = ({
  attachments,
  clearAttachments,
  currentRemoteConnectionId,
  disabled,
  effectiveAllowAttachments,
  isGuidingMode,
  maxLength,
  mcpEnabled,
  autoCreateTask,
  normalizedWorkspaceRoot,
  onGuide,
  onSend,
  requireModelSelection,
  sanitizedEnabledMcpIds,
  selectedRuntimeProject,
  selectedModelId,
  selectedSkillIds,
  effectiveModelName,
  effectiveThinkingLevel,
  skillsEnabled,
}: UseInputAreaMessageDraftOptions) => {
  const [message, setMessage] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const adjustTextareaHeight = useCallback(() => {
    const textarea = textareaRef.current;
    if (!textarea) {
      return;
    }

    textarea.style.height = 'auto';
    const scrollHeight = textarea.scrollHeight;
    textarea.style.height = `${Math.min(scrollHeight, 200)}px`;
  }, []);

  const resetComposer = useCallback(() => {
    setMessage('');
    clearAttachments();
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }
  }, [clearAttachments]);

  const handleInputChange = useCallback((event: ChangeEvent<HTMLTextAreaElement>) => {
    const value = event.target.value;
    if (value.length <= maxLength) {
      setMessage(value);
      adjustTextareaHeight();
    }
  }, [adjustTextareaHeight, maxLength]);

  const handleSend = useCallback(() => {
    const trimmedMessage = message.trim();
    if (!trimmedMessage && (!effectiveAllowAttachments || attachments.length === 0)) {
      return;
    }
    if (disabled) {
      return;
    }

    if (isGuidingMode) {
      if (!trimmedMessage && attachments.length === 0) {
        return;
      }
      onGuide?.(trimmedMessage, attachments);
      resetComposer();
      return;
    }

    if (requireModelSelection()) {
      return;
    }

    const runtimeProjectId = selectedRuntimeProject?.id?.trim() || '0';
    const runtimeProjectRoot = runtimeProjectId === '0'
      ? null
      : (selectedRuntimeProject?.rootPath || null);
    const runtimeWorkspaceRoot = normalizedWorkspaceRoot || null;

    onSend(trimmedMessage, attachments, {
      mcpEnabled,
      enabledMcpIds: sanitizedEnabledMcpIds,
      autoCreateTask,
      modelConfigId: selectedModelId,
      modelName: effectiveModelName,
      thinkingLevel: effectiveThinkingLevel,
      remoteConnectionId: currentRemoteConnectionId,
      projectId: runtimeProjectId,
      projectRoot: runtimeProjectRoot,
      workspaceRoot: runtimeWorkspaceRoot,
      skillsEnabled,
      selectedSkillIds: skillsEnabled ? selectedSkillIds : [],
    });
    resetComposer();
  }, [
    attachments,
    currentRemoteConnectionId,
    disabled,
    effectiveAllowAttachments,
    isGuidingMode,
    mcpEnabled,
    autoCreateTask,
    message,
    normalizedWorkspaceRoot,
    onGuide,
    onSend,
    requireModelSelection,
    resetComposer,
    sanitizedEnabledMcpIds,
    selectedRuntimeProject,
    selectedModelId,
    selectedSkillIds,
    effectiveModelName,
    effectiveThinkingLevel,
    skillsEnabled,
  ]);

  const handleKeyDown = useCallback((event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      handleSend();
    }
  }, [handleSend]);

  return {
    message,
    textareaRef,
    handleInputChange,
    handleKeyDown,
    handleSend,
    canSend: Boolean(message.trim() || attachments.length > 0),
  };
};
