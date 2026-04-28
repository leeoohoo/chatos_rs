import { useCallback, useMemo, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import { normalizeNoteMeta } from './controllerHelpers';
import {
  buildFolderTree,
  type NoteMeta,
} from './utils';

interface UseNotepadDataOptions {
  apiClient: ApiClient;
  searchQuery: string;
  selectedNoteId: string;
}

export const useNotepadData = ({
  apiClient,
  searchQuery,
  selectedNoteId,
}: UseNotepadDataOptions) => {
  const [folders, setFolders] = useState<string[]>([]);
  const [notes, setNotes] = useState<NoteMeta[]>([]);

  const availableFolders = useMemo(
    () => folders.filter((item) => item.trim().length > 0),
    [folders]
  );
  const folderTree = useMemo(
    () => buildFolderTree(availableFolders, notes),
    [availableFolders, notes]
  );
  const selectedNoteMeta = useMemo(
    () => notes.find((item) => item.id === selectedNoteId) || null,
    [notes, selectedNoteId]
  );

  const loadFolders = useCallback(async () => {
    const res = await apiClient.listNotepadFolders();
    const list = Array.isArray(res?.folders) ? res.folders : [];
    const normalized = list
      .map((item) => String(item || '').trim())
      .filter((item) => item.length > 0);
    setFolders(['', ...normalized]);
  }, [apiClient]);

  const loadNotes = useCallback(async () => {
    const res = await apiClient.listNotepadNotes({
      recursive: true,
      query: searchQuery || undefined,
      limit: 500,
    });
    const list = Array.isArray(res?.notes) ? res.notes : [];
    setNotes(list.map(normalizeNoteMeta));
  }, [apiClient, searchQuery]);

  return {
    folders,
    setFolders,
    notes,
    setNotes,
    availableFolders,
    folderTree,
    selectedNoteMeta,
    loadFolders,
    loadNotes,
  };
};
