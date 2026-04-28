import { useCallback } from 'react';

import type ApiClient from '../../lib/api/client';

interface UseNotepadOpenNoteOptions {
  apiClient: ApiClient;
  ensureFolderExpanded: (folderPath: string) => void;
  setContent: (value: string) => void;
  setDirty: (value: boolean) => void;
  setError: (value: string | null) => void;
  setLoading: (value: boolean) => void;
  setSelectedFolder: (value: string) => void;
  setSelectedNoteId: (value: string) => void;
  setTagsText: (value: string) => void;
  setTitle: (value: string) => void;
}

export const useNotepadOpenNote = ({
  apiClient,
  ensureFolderExpanded,
  setContent,
  setDirty,
  setError,
  setLoading,
  setSelectedFolder,
  setSelectedNoteId,
  setTagsText,
  setTitle,
}: UseNotepadOpenNoteOptions) => useCallback(async (id: string) => {
  if (!id) {
    return;
  }
  setLoading(true);
  setError(null);
  try {
    const res = await apiClient.getNotepadNote(id);
    const note = res?.note;
    const noteFolder = String(note?.folder || '');
    setSelectedNoteId(String(note?.id || id));
    setSelectedFolder(noteFolder);
    setTitle(String(note?.title || ''));
    setTagsText(Array.isArray(note?.tags) ? note.tags.join(', ') : '');
    setContent(String(res?.content || ''));
    ensureFolderExpanded(noteFolder);
    setDirty(false);
  } catch (err) {
    setError(err instanceof Error ? err.message : '打开笔记失败');
  } finally {
    setLoading(false);
  }
}, [
  apiClient,
  ensureFolderExpanded,
  setContent,
  setDirty,
  setError,
  setLoading,
  setSelectedFolder,
  setSelectedNoteId,
  setTagsText,
  setTitle,
]);
