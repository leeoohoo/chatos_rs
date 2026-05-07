import { useCallback, useMemo, useRef, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import { normalizeNoteDetail, normalizeNoteMeta } from './controllerHelpers';
import {
  buildFolderTree,
  normalizeFolderPath,
  type NoteDetail,
  type NoteMeta,
} from './utils';

interface UseNotepadDataOptions {
  apiClient: ApiClient;
  searchQuery: string;
  selectedNoteId: string;
}

interface LoadResourceOptions {
  force?: boolean;
}

type NotesCacheEntry = {
  notes: NoteMeta[];
  stale: boolean;
};

type NoteDetailCacheEntry = {
  detail: NoteDetail;
  stale: boolean;
};

interface NotepadClientCacheState {
  notepadInitReady: boolean;
  notepadInitInflight: Promise<void> | null;
  foldersCache: string[];
  foldersStale: boolean;
  foldersInflight: Promise<string[]> | null;
  notesCache: Map<string, NotesCacheEntry>;
  notesInflight: Map<string, Promise<NoteMeta[]>>;
  noteDetailsCache: Map<string, NoteDetailCacheEntry>;
  noteDetailsInflight: Map<string, Promise<NoteDetail>>;
}

const EMPTY_FOLDERS = [''];

const notepadClientCaches = new WeakMap<ApiClient, NotepadClientCacheState>();

const normalizeFolders = (folders: string[]): string[] => {
  const normalized = folders
    .map((item) => String(item || '').trim())
    .filter((item) => item.length > 0);
  return ['', ...normalized];
};

const buildNotesCacheKey = (searchQuery: string): string => searchQuery.trim();

const sortNotesByUpdatedAtDesc = (notes: NoteMeta[]): NoteMeta[] => {
  const next = [...notes];
  next.sort((left, right) => {
    const leftTs = Number.isNaN(Date.parse(left.updated_at || '')) ? 0 : Date.parse(left.updated_at || '');
    const rightTs = Number.isNaN(Date.parse(right.updated_at || '')) ? 0 : Date.parse(right.updated_at || '');
    const delta = rightTs - leftTs;
    if (delta !== 0) {
      return delta;
    }
    return left.title.localeCompare(right.title, 'zh-Hans-CN');
  });
  return next;
};

const matchesNotesQuery = (note: NoteMeta, searchQuery: string): boolean => {
  const normalizedQuery = searchQuery.trim().toLowerCase();
  if (!normalizedQuery) {
    return true;
  }
  return note.title.toLowerCase().includes(normalizedQuery)
    || note.folder.toLowerCase().includes(normalizedQuery);
};

const collectFolderAncestors = (folderPath: string): string[] => {
  const normalized = normalizeFolderPath(folderPath);
  if (!normalized) {
    return [];
  }
  const segments = normalized.split('/').filter((item) => item.trim().length > 0);
  const folders: string[] = [];
  let current = '';
  for (const segment of segments) {
    current = current ? `${current}/${segment}` : segment;
    folders.push(current);
  }
  return folders;
};

const removeFolderAndDescendants = (folders: string[], folderPath: string): string[] => {
  const normalizedTarget = normalizeFolderPath(folderPath);
  if (!normalizedTarget) {
    return folders;
  }
  const prefix = `${normalizedTarget}/`;
  return folders.filter((folder) => folder !== normalizedTarget && !folder.startsWith(prefix));
};

const renameFolderAndDescendants = (folderPath: string, fromPath: string, toPath: string): string => {
  const normalizedFolder = normalizeFolderPath(folderPath);
  const normalizedFrom = normalizeFolderPath(fromPath);
  const normalizedTo = normalizeFolderPath(toPath);
  if (!normalizedFrom || !normalizedTo) {
    return normalizedFolder;
  }
  if (normalizedFolder === normalizedFrom) {
    return normalizedTo;
  }
  const prefix = `${normalizedFrom}/`;
  if (normalizedFolder.startsWith(prefix)) {
    const suffix = normalizedFolder.slice(prefix.length);
    return suffix ? `${normalizedTo}/${suffix}` : normalizedTo;
  }
  return normalizedFolder;
};

const getOrCreateClientCacheState = (apiClient: ApiClient): NotepadClientCacheState => {
  const existing = notepadClientCaches.get(apiClient);
  if (existing) {
    return existing;
  }

  const nextState: NotepadClientCacheState = {
    notepadInitReady: false,
    notepadInitInflight: null,
    foldersCache: EMPTY_FOLDERS,
    foldersStale: true,
    foldersInflight: null,
    notesCache: new Map(),
    notesInflight: new Map(),
    noteDetailsCache: new Map(),
    noteDetailsInflight: new Map(),
  };
  notepadClientCaches.set(apiClient, nextState);
  return nextState;
};

export const useNotepadData = ({
  apiClient,
  searchQuery,
  selectedNoteId,
}: UseNotepadDataOptions) => {
  const clientCacheState = getOrCreateClientCacheState(apiClient);
  const notesCacheKey = buildNotesCacheKey(searchQuery);
  const [folders, setFolders] = useState<string[]>(clientCacheState.foldersCache);
  const [notes, setNotes] = useState<NoteMeta[]>(() => clientCacheState.notesCache.get(notesCacheKey)?.notes || []);
  const selectedNotesCacheKeyRef = useRef(notesCacheKey);

  selectedNotesCacheKeyRef.current = notesCacheKey;

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

  const loadFolders = useCallback(async (options?: LoadResourceOptions) => {
    const cacheState = getOrCreateClientCacheState(apiClient);
    if (!options?.force && !cacheState.foldersStale) {
      setFolders(cacheState.foldersCache);
      return;
    }

    if (!cacheState.foldersInflight) {
      cacheState.foldersInflight = apiClient.listNotepadFolders()
        .then((res) => {
          const list = Array.isArray(res?.folders) ? res.folders : [];
          const normalized = normalizeFolders(list);
          cacheState.foldersCache = normalized;
          cacheState.foldersStale = false;
          return normalized;
        })
        .finally(() => {
          cacheState.foldersInflight = null;
        });
    }

    const normalized = await cacheState.foldersInflight;
    setFolders(normalized);
  }, [apiClient]);

  const loadNotes = useCallback(async (options?: LoadResourceOptions) => {
    const cacheState = getOrCreateClientCacheState(apiClient);
    const cacheKey = buildNotesCacheKey(searchQuery);
    const cached = cacheState.notesCache.get(cacheKey);
    if (!options?.force && cached && !cached.stale) {
      setNotes(cached.notes);
      return;
    }

    let inflight = cacheState.notesInflight.get(cacheKey);
    if (!inflight) {
      inflight = apiClient.listNotepadNotes({
        recursive: true,
        query: searchQuery || undefined,
        limit: 500,
      })
        .then((res) => {
          const list = Array.isArray(res?.notes) ? res.notes : [];
          const normalized = list.map(normalizeNoteMeta);
          cacheState.notesCache.set(cacheKey, {
            notes: normalized,
            stale: false,
          });
          return normalized;
        })
        .finally(() => {
          cacheState.notesInflight.delete(cacheKey);
        });
      cacheState.notesInflight.set(cacheKey, inflight);
    }

    const normalized = await inflight;
    if (selectedNotesCacheKeyRef.current === cacheKey) {
      setNotes(normalized);
    }
  }, [apiClient, searchQuery]);

  const hydrateFolders = useCallback(() => {
    setFolders(getOrCreateClientCacheState(apiClient).foldersCache);
  }, [apiClient]);

  const hydrateNotes = useCallback(() => {
    const cacheState = getOrCreateClientCacheState(apiClient);
    setNotes(cacheState.notesCache.get(buildNotesCacheKey(searchQuery))?.notes || []);
  }, [apiClient, searchQuery]);

  const ensureInit = useCallback(async (options?: LoadResourceOptions) => {
    const cacheState = getOrCreateClientCacheState(apiClient);
    if (cacheState.notepadInitReady && !options?.force) {
      return;
    }
    if (!cacheState.notepadInitInflight) {
      cacheState.notepadInitInflight = apiClient.notepadInit()
        .then(() => {
          cacheState.notepadInitReady = true;
        })
        .finally(() => {
          cacheState.notepadInitInflight = null;
        });
    }
    await cacheState.notepadInitInflight;
    cacheState.notepadInitReady = true;
  }, [apiClient]);

  const markFoldersStale = useCallback(() => {
    getOrCreateClientCacheState(apiClient).foldersStale = true;
  }, [apiClient]);

  const markNotesStale = useCallback((targetSearchQuery?: string) => {
    const cacheState = getOrCreateClientCacheState(apiClient);
    if (typeof targetSearchQuery === 'string') {
      const cacheKey = buildNotesCacheKey(targetSearchQuery);
      const cached = cacheState.notesCache.get(cacheKey);
      if (cached) {
        cacheState.notesCache.set(cacheKey, {
          ...cached,
          stale: true,
        });
      }
      return;
    }

    cacheState.notesCache.forEach((entry, key) => {
      cacheState.notesCache.set(key, {
        ...entry,
        stale: true,
      });
    });
  }, [apiClient]);

  const upsertCachedNote = useCallback((note: NoteMeta) => {
    const cacheState = getOrCreateClientCacheState(apiClient);
    cacheState.notesCache.forEach((entry, key) => {
      const filtered = entry.notes.filter((item) => item.id !== note.id);
      if (!matchesNotesQuery(note, key)) {
        cacheState.notesCache.set(key, {
          ...entry,
          notes: filtered,
        });
        return;
      }
      cacheState.notesCache.set(key, {
        stale: entry.stale,
        notes: sortNotesByUpdatedAtDesc([...filtered, note]),
      });
    });

    const nextFolders = new Set(cacheState.foldersCache);
    for (const folder of collectFolderAncestors(note.folder)) {
      nextFolders.add(folder);
    }
    cacheState.foldersCache = normalizeFolders([...nextFolders]);
    cacheState.foldersStale = false;

    const currentKey = selectedNotesCacheKeyRef.current;
    const currentNotes = cacheState.notesCache.get(currentKey)?.notes || [];
    setFolders(cacheState.foldersCache);
    setNotes(currentNotes);
  }, [apiClient]);

  const loadNoteDetail = useCallback(async (noteId: string, options?: LoadResourceOptions): Promise<NoteDetail> => {
    const normalizedId = String(noteId || '').trim();
    if (!normalizedId) {
      throw new Error('note id is required');
    }

    const cacheState = getOrCreateClientCacheState(apiClient);
    const cached = cacheState.noteDetailsCache.get(normalizedId);
    if (!options?.force && cached && !cached.stale) {
      return cached.detail;
    }

    let inflight = cacheState.noteDetailsInflight.get(normalizedId);
    if (!inflight) {
      inflight = apiClient.getNotepadNote(normalizedId)
        .then((response) => {
          const detail = normalizeNoteDetail(response, normalizedId);
          cacheState.noteDetailsCache.set(normalizedId, {
            detail,
            stale: false,
          });
          upsertCachedNote(detail.note);
          return detail;
        })
        .finally(() => {
          cacheState.noteDetailsInflight.delete(normalizedId);
        });
      cacheState.noteDetailsInflight.set(normalizedId, inflight);
    }

    return inflight;
  }, [apiClient, upsertCachedNote]);

  const getCachedNoteDetail = useCallback((noteId: string): NoteDetail | null => {
    const normalizedId = String(noteId || '').trim();
    if (!normalizedId) {
      return null;
    }
    return getOrCreateClientCacheState(apiClient).noteDetailsCache.get(normalizedId)?.detail || null;
  }, [apiClient]);

  const markNoteDetailStale = useCallback((noteId: string) => {
    const normalizedId = String(noteId || '').trim();
    if (!normalizedId) {
      return;
    }
    const cacheState = getOrCreateClientCacheState(apiClient);
    const cached = cacheState.noteDetailsCache.get(normalizedId);
    if (!cached) {
      return;
    }
    cacheState.noteDetailsCache.set(normalizedId, {
      ...cached,
      stale: true,
    });
  }, [apiClient]);

  const upsertCachedNoteDetail = useCallback((detail: NoteDetail) => {
    const normalizedId = String(detail.note.id || '').trim();
    if (!normalizedId) {
      return;
    }
    const cacheState = getOrCreateClientCacheState(apiClient);
    cacheState.noteDetailsCache.set(normalizedId, {
      detail,
      stale: false,
    });
    upsertCachedNote(detail.note);
  }, [apiClient, upsertCachedNote]);

  const removeCachedNote = useCallback((noteId: string) => {
    const normalizedId = String(noteId || '').trim();
    if (!normalizedId) {
      return;
    }
    const cacheState = getOrCreateClientCacheState(apiClient);
    cacheState.noteDetailsCache.delete(normalizedId);
    cacheState.noteDetailsInflight.delete(normalizedId);
    cacheState.notesCache.forEach((entry, key) => {
      cacheState.notesCache.set(key, {
        ...entry,
        notes: entry.notes.filter((item) => item.id !== normalizedId),
      });
    });
    const currentKey = selectedNotesCacheKeyRef.current;
    setNotes(cacheState.notesCache.get(currentKey)?.notes || []);
  }, [apiClient]);

  const applyFolderToCache = useCallback((folderPath: string) => {
    const normalized = normalizeFolderPath(folderPath);
    if (!normalized) {
      return;
    }
    const cacheState = getOrCreateClientCacheState(apiClient);
    const nextFolders = new Set(cacheState.foldersCache);
    for (const folder of collectFolderAncestors(normalized)) {
      nextFolders.add(folder);
    }
    cacheState.foldersCache = normalizeFolders([...nextFolders]);
    cacheState.foldersStale = false;
    setFolders(cacheState.foldersCache);
  }, [apiClient]);

  const removeFolderFromCache = useCallback((folderPath: string) => {
    const normalized = normalizeFolderPath(folderPath);
    if (!normalized) {
      return;
    }
    const cacheState = getOrCreateClientCacheState(apiClient);
    cacheState.foldersCache = removeFolderAndDescendants(cacheState.foldersCache, normalized);
    cacheState.foldersStale = false;
    const deletedNoteIds: string[] = [];
    cacheState.notesCache.forEach((entry, key) => {
      cacheState.notesCache.set(key, {
        ...entry,
        notes: entry.notes.filter((note) => {
          const noteFolder = normalizeFolderPath(note.folder);
          if (noteFolder === normalized || noteFolder.startsWith(`${normalized}/`)) {
            deletedNoteIds.push(note.id);
          }
          return noteFolder !== normalized && !noteFolder.startsWith(`${normalized}/`);
        }),
      });
    });
    for (const noteId of deletedNoteIds) {
      cacheState.noteDetailsCache.delete(noteId);
      cacheState.noteDetailsInflight.delete(noteId);
    }
    const currentKey = selectedNotesCacheKeyRef.current;
    setFolders(cacheState.foldersCache);
    setNotes(cacheState.notesCache.get(currentKey)?.notes || []);
  }, [apiClient]);

  const renameFolderInCache = useCallback((fromPath: string, toPath: string) => {
    const normalizedFrom = normalizeFolderPath(fromPath);
    const normalizedTo = normalizeFolderPath(toPath);
    if (!normalizedFrom || !normalizedTo || normalizedFrom === normalizedTo) {
      return;
    }
    const cacheState = getOrCreateClientCacheState(apiClient);
    const nextFolders = new Set<string>();
    cacheState.foldersCache.forEach((folder) => {
      const renamed = renameFolderAndDescendants(folder, normalizedFrom, normalizedTo);
      if (renamed) {
        nextFolders.add(renamed);
      }
    });
    for (const folder of collectFolderAncestors(normalizedTo)) {
      nextFolders.add(folder);
    }
    cacheState.foldersCache = normalizeFolders([...nextFolders]);
    cacheState.foldersStale = false;

    cacheState.notesCache.forEach((entry, key) => {
      const renamedNotes = entry.notes.map((note) => {
        const nextFolder = renameFolderAndDescendants(note.folder, normalizedFrom, normalizedTo);
        if (nextFolder === note.folder) {
          return note;
        }
        return {
          ...note,
          folder: nextFolder,
        };
      });
      cacheState.notesCache.set(key, {
        ...entry,
        notes: sortNotesByUpdatedAtDesc(renamedNotes),
      });
    });

    cacheState.noteDetailsCache.forEach((entry, key) => {
      const nextFolder = renameFolderAndDescendants(entry.detail.note.folder, normalizedFrom, normalizedTo);
      if (nextFolder === entry.detail.note.folder) {
        return;
      }
      cacheState.noteDetailsCache.set(key, {
        ...entry,
        detail: {
          ...entry.detail,
          note: {
            ...entry.detail.note,
            folder: nextFolder,
          },
        },
      });
    });

    const currentKey = selectedNotesCacheKeyRef.current;
    setFolders(cacheState.foldersCache);
    setNotes(cacheState.notesCache.get(currentKey)?.notes || []);
  }, [apiClient]);

  return {
    folders,
    setFolders,
    notes,
    setNotes,
    availableFolders,
    folderTree,
    selectedNoteMeta,
    ensureInit,
    hydrateFolders,
    hydrateNotes,
    loadFolders,
    loadNotes,
    markFoldersStale,
    markNotesStale,
    loadNoteDetail,
    getCachedNoteDetail,
    markNoteDetailStale,
    upsertCachedNoteDetail,
    upsertCachedNote,
    removeCachedNote,
    applyFolderToCache,
    removeFolderFromCache,
    renameFolderInCache,
  };
};
