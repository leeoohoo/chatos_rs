import { useCallback } from 'react';

import type ApiClient from '../../lib/api/client';
import {
  copyTextToClipboard,
  downloadMarkdownFile,
} from './controllerHelpers';
import type { NoteMeta } from './utils';

interface UseNotepadExportActionsOptions {
  apiClient: ApiClient;
  selectedNoteId: string;
  title: string;
  content: string;
  setError: (message: string | null) => void;
}

export const useNotepadExportActions = ({
  apiClient,
  selectedNoteId,
  title,
  content,
  setError,
}: UseNotepadExportActionsOptions) => {
  const resolveNotePayload = useCallback(async (
    note?: NoteMeta | null,
  ): Promise<{ title: string; content: string }> => {
    const targetNoteId = String(note?.id || selectedNoteId || '').trim();
    const fallbackTitle = String(note?.title || title || 'Untitled').trim() || 'Untitled';

    if (targetNoteId && targetNoteId === selectedNoteId) {
      return {
        title: title.trim() || fallbackTitle,
        content,
      };
    }

    if (!targetNoteId) {
      return {
        title: fallbackTitle,
        content,
      };
    }

    const res = await apiClient.getNotepadNote(targetNoteId);
    const remoteNote = res?.note;
    return {
      title: String(remoteNote?.title || fallbackTitle || 'Untitled'),
      content: String(res?.content || ''),
    };
  }, [apiClient, content, selectedNoteId, title]);

  const copyText = useCallback(async (note?: NoteMeta | null) => {
    try {
      const payload = await resolveNotePayload(note);
      await copyTextToClipboard(payload.content || '');
    } catch (err) {
      setError(err instanceof Error ? err.message : '复制文本失败');
    }
  }, [resolveNotePayload, setError]);

  const copyAsMd = useCallback(async (note?: NoteMeta | null) => {
    try {
      const payload = await resolveNotePayload(note);
      downloadMarkdownFile(payload.title, payload.content || '');
    } catch (err) {
      setError(err instanceof Error ? err.message : '导出 .md 失败');
    }
  }, [resolveNotePayload, setError]);

  return {
    copyText,
    copyAsMd,
  };
};
