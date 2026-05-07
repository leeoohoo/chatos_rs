import { useCallback } from 'react';

import type { NoteDetail } from './utils';

interface UseNotepadOpenNoteOptions {
  ensureFolderExpanded: (folderPath: string) => void;
  loadNoteDetail: (noteId: string, options?: { force?: boolean }) => Promise<NoteDetail>;
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
  ensureFolderExpanded,
  loadNoteDetail,
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
    const detail = await loadNoteDetail(id);
    const note = detail.note;
    const noteFolder = String(note.folder || '');
    setSelectedNoteId(String(note.id || id));
    setSelectedFolder(noteFolder);
    setTitle(String(note.title || ''));
    setTagsText(Array.isArray(note.tags) ? note.tags.join(', ') : '');
    setContent(String(detail.content || ''));
    ensureFolderExpanded(noteFolder);
    setDirty(false);
  } catch (err) {
    setError(err instanceof Error ? err.message : '打开笔记失败');
  } finally {
    setLoading(false);
  }
}, [
  ensureFolderExpanded,
  loadNoteDetail,
  setContent,
  setDirty,
  setError,
  setLoading,
  setSelectedFolder,
  setSelectedNoteId,
  setTagsText,
  setTitle,
]);
