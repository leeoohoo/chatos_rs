import { useCallback } from 'react';

import type ApiClient from '../../lib/api/client';
import type { useDialogService } from '../ui/DialogProvider';
import {
  normalizeFolderPath,
  parseTags,
  type NoteMeta,
} from './utils';

interface UseNotepadCrudActionsOptions {
  apiClient: ApiClient;
  confirm: ReturnType<typeof useDialogService>['confirm'];
  content: string;
  ensureFolderExpanded: (folderPath: string) => void;
  loadFolders: () => Promise<void>;
  loadNotes: () => Promise<void>;
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
  ensureFolderExpanded,
  loadFolders,
  loadNotes,
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
      ? `在目录 "${baseFolder}" 下新建子目录（支持输入相对路径）`
      : '请输入新文件夹路径（例如 work/ideas）';
    const raw = await prompt({
      title: '新建目录',
      message: promptTitle,
      inputLabel: '目录路径',
      placeholder: baseFolder ? '例如 ideas/today' : '例如 work/ideas',
      defaultValue: '',
      confirmText: '创建',
      cancelText: '取消',
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
      await Promise.all([loadFolders(), loadNotes()]);
    } catch (err) {
      setError(err instanceof Error ? err.message : '创建文件夹失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, ensureFolderExpanded, loadFolders, loadNotes, prompt, selectedFolder, setError, setLoading, setSelectedFolder]);

  const createNote = useCallback(async (folderOverride?: string) => {
    const targetFolder = normalizeFolderPath(folderOverride ?? selectedFolder);
    const noteTitle = await prompt({
      title: '新建笔记',
      message: targetFolder
        ? `将在目录 "${targetFolder}" 下创建笔记`
        : '将在根目录下创建笔记',
      inputLabel: '笔记标题',
      placeholder: '请输入笔记标题',
      defaultValue: '',
      confirmText: '创建',
      cancelText: '取消',
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
      await loadNotes();
      const id = String(res?.note?.id || '');
      if (id) {
        await openNote(id);
      } else {
        resetEditor();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : '创建笔记失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, ensureFolderExpanded, loadNotes, openNote, prompt, resetEditor, selectedFolder, setError, setLoading, setSelectedFolder]);

  const saveNote = useCallback(async () => {
    if (!selectedNoteId) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      await apiClient.updateNotepadNote(selectedNoteId, {
        title: title.trim(),
        content,
        tags: parseTags(tagsText),
      });
      setDirty(false);
      await loadNotes();
    } catch (err) {
      setError(err instanceof Error ? err.message : '保存笔记失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, content, loadNotes, selectedNoteId, setDirty, setError, setLoading, tagsText, title]);

  const deleteNoteById = useCallback(async (noteId: string, titleHint?: string) => {
    const normalizedId = String(noteId || '').trim();
    if (!normalizedId) {
      return;
    }
    const confirmed = await confirm({
      title: '删除笔记',
      message: `确认删除笔记“${titleHint || '当前笔记'}”？此操作不可恢复。`,
      confirmText: '删除',
      cancelText: '取消',
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
      await loadNotes();
    } catch (err) {
      setError(err instanceof Error ? err.message : '删除笔记失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, confirm, loadNotes, resetEditor, selectedNoteId, setError, setLoading]);

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
      title: '删除目录',
      message: `确认删除目录“${folder}”吗？会同时删除该目录下所有笔记。`,
      confirmText: '删除',
      cancelText: '取消',
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

      await Promise.all([loadFolders(), loadNotes()]);
    } catch (err) {
      setError(err instanceof Error ? err.message : '删除目录失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, confirm, loadFolders, loadNotes, notes, resetEditor, selectedFolder, selectedNoteId, setError, setLoading, setSelectedFolder]);

  return {
    createFolder,
    createNote,
    saveNote,
    deleteNoteById,
    deleteNote,
    deleteFolder,
  };
};
