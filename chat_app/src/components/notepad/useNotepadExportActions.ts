import { useCallback } from 'react';

import type { TranslateFn } from '../../i18n/I18nProvider';
import {
  copyTextToClipboard,
  downloadMarkdownFile,
} from './controllerHelpers';
import type { NoteDetail, NoteMeta } from './utils';

interface UseNotepadExportActionsOptions {
  getCachedNoteDetail: (noteId: string) => NoteDetail | null;
  loadNoteDetail: (noteId: string, options?: { force?: boolean }) => Promise<NoteDetail>;
  selectedNoteId: string;
  t: TranslateFn;
  title: string;
  content: string;
  setError: (message: string | null) => void;
}

export const useNotepadExportActions = ({
  getCachedNoteDetail,
  loadNoteDetail,
  selectedNoteId,
  t,
  title,
  content,
  setError,
}: UseNotepadExportActionsOptions) => {
  const resolveNotePayload = useCallback(async (
    note?: NoteMeta | null,
  ): Promise<{ title: string; content: string }> => {
    const targetNoteId = String(note?.id || selectedNoteId || '').trim();
    const untitled = t('notepad.tree.noteUntitled');
    const fallbackTitle = String(note?.title || title || untitled).trim() || untitled;

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
        title: String(cachedDetail.note.title || fallbackTitle || untitled),
        content: String(cachedDetail.content || ''),
      };
    }

    const remoteDetail = await loadNoteDetail(targetNoteId);
    return {
      title: String(remoteDetail.note.title || fallbackTitle || untitled),
      content: String(remoteDetail.content || ''),
    };
  }, [content, getCachedNoteDetail, loadNoteDetail, selectedNoteId, t, title]);

  const copyText = useCallback(async (note?: NoteMeta | null) => {
    try {
      const payload = await resolveNotePayload(note);
      await copyTextToClipboard(payload.content || '');
    } catch (err) {
      setError(err instanceof Error ? err.message : t('notepad.error.copyText'));
    }
  }, [resolveNotePayload, setError, t]);

  const copyAsMd = useCallback(async (note?: NoteMeta | null) => {
    try {
      const payload = await resolveNotePayload(note);
      downloadMarkdownFile(payload.title, payload.content || '');
    } catch (err) {
      setError(err instanceof Error ? err.message : t('notepad.error.copyAsMd'));
    }
  }, [resolveNotePayload, setError, t]);

  return {
    copyText,
    copyAsMd,
  };
};
