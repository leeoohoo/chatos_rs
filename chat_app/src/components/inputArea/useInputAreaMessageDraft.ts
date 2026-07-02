// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
  disabled: boolean;
  effectiveAllowAttachments: boolean;
  maxLength: number;
  onSend: InputAreaProps['onSend'];
  requireModelSelection: () => boolean;
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

    const runtimeProjectId = selectedRuntimeProject?.id?.trim() || selectedProjectId?.trim() || '0';
    const runtimeProjectRoot = runtimeProjectId === '0'
      ? null
      : (selectedRuntimeProject?.rootPath || null);

    onSend(trimmedMessage, attachments, {
      projectId: runtimeProjectId,
      projectRoot: runtimeProjectRoot,
    });
    resetComposer();
  }, [
    attachments,
    disabled,
    effectiveAllowAttachments,
    message,
    onSend,
    requireModelSelection,
    resetComposer,
    selectedProjectId,
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
    handleKeyDown,
    handleSend,
    canSend: Boolean(message.trim() || attachments.length > 0),
  };
};
