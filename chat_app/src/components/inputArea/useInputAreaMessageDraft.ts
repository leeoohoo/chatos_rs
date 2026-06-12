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
  maxLength: number;
  normalizedWorkspaceRoot: string | null;
  onSend: InputAreaProps['onSend'];
  requireModelSelection: () => boolean;
  selectedRuntimeProject: Project | null;
  selectedModelId: string | null;
  effectiveModelName: string | null;
  effectiveThinkingLevel: string | null;
}

export const useInputAreaMessageDraft = ({
  attachments,
  clearAttachments,
  currentRemoteConnectionId,
  disabled,
  effectiveAllowAttachments,
  maxLength,
  normalizedWorkspaceRoot,
  onSend,
  requireModelSelection,
  selectedRuntimeProject,
  selectedModelId,
  effectiveModelName,
  effectiveThinkingLevel,
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

    if (requireModelSelection()) {
      return;
    }

    const runtimeProjectId = selectedRuntimeProject?.id?.trim() || '0';
    const runtimeProjectRoot = runtimeProjectId === '0'
      ? null
      : (selectedRuntimeProject?.rootPath || null);
    const runtimeWorkspaceRoot = normalizedWorkspaceRoot || null;

    onSend(trimmedMessage, attachments, {
      modelConfigId: selectedModelId,
      modelName: effectiveModelName,
      thinkingLevel: effectiveThinkingLevel,
      remoteConnectionId: currentRemoteConnectionId,
      projectId: runtimeProjectId,
      projectRoot: runtimeProjectRoot,
      workspaceRoot: runtimeWorkspaceRoot,
    });
    resetComposer();
  }, [
    attachments,
    currentRemoteConnectionId,
    disabled,
    effectiveAllowAttachments,
    message,
    normalizedWorkspaceRoot,
    onSend,
    requireModelSelection,
    resetComposer,
    selectedRuntimeProject,
    selectedModelId,
    effectiveModelName,
    effectiveThinkingLevel,
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
