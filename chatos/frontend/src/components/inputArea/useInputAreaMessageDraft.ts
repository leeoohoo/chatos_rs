// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  useCallback,
  useRef,
  useState,
  type ChangeEvent,
  type KeyboardEvent,
} from 'react';

import type {
  InputAreaProps,
  PluginAgentSelectionPayload,
  PluginCommandInvocationPayload,
  Project,
} from '../../types';

interface UseInputAreaMessageDraftOptions {
  attachments: File[];
  clearAttachments: () => void;
  disabled: boolean;
  effectiveAllowAttachments: boolean;
  maxLength: number;
  onSend: InputAreaProps['onSend'];
  requireModelSelection: () => boolean;
  requireValidPluginSelection: () => boolean;
  pluginDeviceId: string | null;
  pluginWorkspaceId: string | null;
  selectedPluginIds: string[];
  pluginCommandInvocations: PluginCommandInvocationPayload[];
  pluginAgentSelection: PluginAgentSelectionPayload | null;
  commandMessageFallback: string;
  clearSelectedPlugins: () => void;
  selectedProjectId: string | null;
  selectedRuntimeProject: Project | null;
}

export const useInputAreaMessageDraft = ({
  attachments,
  clearAttachments,
  disabled,
  effectiveAllowAttachments,
  maxLength,
  onSend,
  requireModelSelection,
  requireValidPluginSelection,
  pluginDeviceId,
  pluginWorkspaceId,
  selectedPluginIds,
  pluginCommandInvocations,
  pluginAgentSelection,
  commandMessageFallback,
  clearSelectedPlugins,
  selectedProjectId,
  selectedRuntimeProject,
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
    clearSelectedPlugins();
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }
  }, [clearAttachments, clearSelectedPlugins]);

  const setMessageValue = useCallback((value: string) => {
    if (value.length <= maxLength) {
      setMessage(value);
      window.requestAnimationFrame(adjustTextareaHeight);
    }
  }, [adjustTextareaHeight, maxLength]);

  const handleInputChange = useCallback((event: ChangeEvent<HTMLTextAreaElement>) => {
    setMessageValue(event.target.value);
  }, [setMessageValue]);

  const handleSend = useCallback(() => {
    const trimmedMessage = message.trim();
    const content = trimmedMessage || commandMessageFallback;
    if (
      !content
      && (!effectiveAllowAttachments || attachments.length === 0)
      && pluginCommandInvocations.length === 0
    ) {
      return;
    }
    if (disabled) {
      return;
    }

    if (requireModelSelection()) {
      return;
    }
    if (requireValidPluginSelection()) {
      return;
    }

    const runtimeProjectId = selectedRuntimeProject?.id?.trim() || selectedProjectId?.trim() || '0';
    const runtimeProjectRoot = runtimeProjectId === '0'
      ? null
      : (selectedRuntimeProject?.rootPath || null);

    onSend(content, attachments, {
      projectId: runtimeProjectId,
      projectRoot: runtimeProjectRoot,
      pluginDeviceId: selectedPluginIds.length > 0 ? pluginDeviceId : null,
      pluginWorkspaceId: selectedPluginIds.length > 0 ? pluginWorkspaceId : null,
      selectedPluginIds,
      pluginCommandInvocations,
      pluginAgentSelection,
    });
    resetComposer();
  }, [
    attachments,
    disabled,
    effectiveAllowAttachments,
    message,
    onSend,
    commandMessageFallback,
    pluginCommandInvocations,
    pluginAgentSelection,
    requireModelSelection,
    requireValidPluginSelection,
    resetComposer,
    selectedProjectId,
    pluginDeviceId,
    pluginWorkspaceId,
    selectedPluginIds,
    selectedRuntimeProject,
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
    setMessageValue,
    handleKeyDown,
    handleSend,
    canSend: Boolean(
      message.trim()
      || attachments.length > 0
      || pluginCommandInvocations.length > 0
    ),
  };
};
