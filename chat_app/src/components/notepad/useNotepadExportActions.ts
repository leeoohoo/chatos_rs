import { useCallback } from 'react';

import {
  copyTextToClipboard,
  downloadMarkdownFile,
} from './controllerHelpers';
import type { NoteDetail, NoteMeta } from './utils';

interface UseNotepadExportActionsOptions {
  getCachedNoteDetail: (noteId: string) => NoteDetail | null;
  loadNoteDetail: (noteId: string, options?: { force?: boolean }) => Promise<NoteDetail>;
  selectedNoteId: string;
  title: string;
  content: string;
  setError: (message: string | null) => void;
}

export const useNotepadExportActions = ({
  getCachedNoteDetail,
  loadNoteDetail,
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

    const cachedDetail = getCachedNoteDetail(targetNoteId);
    if (cachedDetail) {
      return {
        title: String(cachedDetail.note.title || fallbackTitle || 'Untitled'),
        content: String(cachedDetail.content || ''),
      };
    }

    const remoteDetail = await loadNoteDetail(targetNoteId);
    return {
      title: String(remoteDetail.note.title || fallbackTitle || 'Untitled'),
      content: String(remoteDetail.content || ''),
    };
  }, [content, getCachedNoteDetail, loadNoteDetail, selectedNoteId, title]);

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
