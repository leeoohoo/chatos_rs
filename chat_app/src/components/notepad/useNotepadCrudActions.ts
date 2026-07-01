// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback } from 'react';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type ApiClient from '../../lib/api/client';
import type { useDialogService } from '../ui/DialogProvider';
import { normalizeNoteMeta } from './controllerHelpers';
import {
  type NoteDetail,
  normalizeFolderPath,
  parseTags,
  type NoteMeta,
} from './utils';

interface UseNotepadCrudActionsOptions {
  apiClient: ApiClient;
  confirm: ReturnType<typeof useDialogService>['confirm'];
  content: string;
  t: TranslateFn;
  ensureFolderExpanded: (folderPath: string) => void;
  loadNotes: (options?: { force?: boolean }) => Promise<void>;
  markNotesStale: () => void;
  upsertCachedNote: (note: NoteMeta) => void;
  upsertCachedNoteDetail: (detail: NoteDetail) => void;
  removeCachedNote: (noteId: string) => void;
  applyFolderToCache: (folderPath: string) => void;
  removeFolderFromCache: (folderPath: string) => void;
  notes: NoteMeta[];
  openNote: (id: string) => Promise<void>;
  prompt: ReturnType<typeof useDialogService>['prompt'];
  resetEditor: () => void;
  selectedFolder: string;
  selectedNoteId: string;
  setDirty: (value: boolean) => void;
  setError: (value: string | null) => void;
  setLoading: (value: boolean) => void;
  setSelectedFolder: (value: string) => void;
  tagsText: string;
  title: string;
}

export const useNotepadCrudActions = ({
  apiClient,
  confirm,
  content,
  t,
  ensureFolderExpanded,
  loadNotes,
  markNotesStale,
  upsertCachedNote,
  upsertCachedNoteDetail,
  removeCachedNote,
  applyFolderToCache,
  removeFolderFromCache,
  notes,
  openNote,
  prompt,
  resetEditor,
  selectedFolder,
  selectedNoteId,
  setDirty,
  setError,
  setLoading,
  setSelectedFolder,
  tagsText,
  title,
}: UseNotepadCrudActionsOptions) => {
  const createFolder = useCallback(async (parentFolder?: string) => {
    const baseFolder = normalizeFolderPath(parentFolder ?? selectedFolder);
    const promptTitle = baseFolder
      ? t('notepad.prompt.createFolder.messageChild', { folder: baseFolder })
      : t('notepad.prompt.createFolder.messageRoot');
    const raw = await prompt({
      title: t('notepad.prompt.createFolder.title'),
      message: promptTitle,
      inputLabel: t('notepad.prompt.createFolder.inputLabel'),
      placeholder: baseFolder ? t('notepad.prompt.createFolder.placeholderChild') : t('notepad.prompt.createFolder.placeholderRoot'),
      defaultValue: '',
      confirmText: t('applications.form.submitCreate'),
      cancelText: t('common.cancel'),
      type: 'info',
    });
    if (raw === null) {
      return;
    }
    const input = normalizeFolderPath(raw);
    if (!input) {
      return;
    }

    const folder = baseFolder && !input.startsWith(`${baseFolder}/`) && input !== baseFolder
      ? `${baseFolder}/${input}`
      : input;
    setLoading(true);
    setError(null);
    try {
      await apiClient.createNotepadFolder({ folder });
      setSelectedFolder(folder);
      ensureFolderExpanded(folder);
      applyFolderToCache(folder);
    } catch (err) {
      setError(err instanceof Error ? err.message : t('notepad.error.createFolder'));
    } finally {
      setLoading(false);
    }
  }, [apiClient, applyFolderToCache, ensureFolderExpanded, prompt, selectedFolder, setError, setLoading, setSelectedFolder, t]);

  const createNote = useCallback(async (folderOverride?: string) => {
    const targetFolder = normalizeFolderPath(folderOverride ?? selectedFolder);
    const noteTitle = await prompt({
      title: t('notepad.prompt.createNote.title'),
      message: targetFolder
        ? t('notepad.prompt.createNote.messageFolder', { folder: targetFolder })
        : t('notepad.prompt.createNote.messageRoot'),
      inputLabel: t('notepad.prompt.createNote.inputLabel'),
      placeholder: t('notepad.prompt.createNote.placeholder'),
      defaultValue: '',
      confirmText: t('applications.form.submitCreate'),
      cancelText: t('common.cancel'),
      type: 'info',
    });
    if (noteTitle === null) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const res = await apiClient.createNotepadNote({
        folder: targetFolder,
        title: noteTitle.trim(),
      });
      if (targetFolder) {
        setSelectedFolder(targetFolder);
        ensureFolderExpanded(targetFolder);
      }
      const nextNote = normalizeNoteMeta(res?.note || {
        id: String(res?.note?.id || ''),
        title: noteTitle.trim(),
        folder: targetFolder,
        tags: [],
        created_at: '',
        updated_at: '',
        file: '',
      });
      upsertCachedNote(nextNote);
      const id = String(res?.note?.id || '');
      if (id) {
        await openNote(id);
      } else {
        resetEditor();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : t('notepad.error.createNote'));
    } finally {
      setLoading(false);
    }
  }, [apiClient, ensureFolderExpanded, openNote, prompt, resetEditor, selectedFolder, setError, setLoading, setSelectedFolder, t, upsertCachedNote]);

  const saveNote = useCallback(async () => {
    if (!selectedNoteId) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const res = await apiClient.updateNotepadNote(selectedNoteId, {
        title: title.trim(),
        content,
        tags: parseTags(tagsText),
      });
      setDirty(false);
      if (res?.note) {
        upsertCachedNote(normalizeNoteMeta(res.note));
        upsertCachedNoteDetail({
          note: normalizeNoteMeta(res.note),
          content,
        });
      } else {
        markNotesStale();
        await loadNotes({ force: true });
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : t('notepad.error.save'));
    } finally {
      setLoading(false);
    }
  }, [apiClient, content, loadNotes, markNotesStale, selectedNoteId, setDirty, setError, setLoading, t, tagsText, title, upsertCachedNote, upsertCachedNoteDetail]);

  const deleteNoteById = useCallback(async (noteId: string, titleHint?: string) => {
    const normalizedId = String(noteId || '').trim();
    if (!normalizedId) {
      return;
    }
    const confirmed = await confirm({
      title: t('notepad.confirm.deleteNote.title'),
      message: t('notepad.confirm.deleteNote.message', {
        name: titleHint || t('notepad.editor.titleEditing'),
      }),
      confirmText: t('aiModelManager.action.delete'),
      cancelText: t('common.cancel'),
      type: 'danger',
    });
    if (!confirmed) {
      return;
    }

    setLoading(true);
    setError(null);
    try {
      await apiClient.deleteNotepadNote(normalizedId);
      if (selectedNoteId === normalizedId) {
        resetEditor();
      }
      removeCachedNote(normalizedId);
    } catch (err) {
      setError(err instanceof Error ? err.message : t('notepad.error.deleteNote'));
    } finally {
      setLoading(false);
    }
  }, [apiClient, confirm, removeCachedNote, resetEditor, selectedNoteId, setError, setLoading, t]);

  const deleteNote = useCallback(async () => {
    if (!selectedNoteId) {
      return;
    }
    const target = notes.find((item) => item.id === selectedNoteId);
    await deleteNoteById(selectedNoteId, target?.title || undefined);
  }, [deleteNoteById, notes, selectedNoteId]);

  const deleteFolder = useCallback(async (folderPath?: string) => {
    const folder = normalizeFolderPath(folderPath ?? selectedFolder);
    if (!folder) {
      return;
    }

    const confirmed = await confirm({
      title: t('notepad.confirm.deleteFolder.title'),
      message: t('notepad.confirm.deleteFolder.message', { name: folder }),
      confirmText: t('aiModelManager.action.delete'),
      cancelText: t('common.cancel'),
      type: 'danger',
    });
    if (!confirmed) {
      return;
    }

    setLoading(true);
    setError(null);
    try {
      await apiClient.deleteNotepadFolder({
        folder,
        recursive: true,
      });

      if (selectedFolder === folder || selectedFolder.startsWith(`${folder}/`)) {
        setSelectedFolder('');
      }

      const selectedNote = notes.find((item) => item.id === selectedNoteId);
      const selectedNoteFolder = normalizeFolderPath(selectedNote?.folder);
      if (selectedNote && (selectedNoteFolder === folder || selectedNoteFolder.startsWith(`${folder}/`))) {
        resetEditor();
      }

      removeFolderFromCache(folder);
    } catch (err) {
      setError(err instanceof Error ? err.message : t('notepad.error.deleteFolder'));
    } finally {
      setLoading(false);
    }
  }, [apiClient, confirm, notes, removeFolderFromCache, resetEditor, selectedFolder, selectedNoteId, setError, setLoading, setSelectedFolder, t]);

  return {
    createFolder,
    createNote,
    saveNote,
    deleteNoteById,
    deleteNote,
    deleteFolder,
  };
};
