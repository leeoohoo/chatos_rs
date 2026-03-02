import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { apiClient as globalApiClient } from '../lib/api/client';
import { useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import { MarkdownRenderer } from './MarkdownRenderer';

interface NotepadPanelProps {
  isOpen: boolean;
  onClose: () => void;
  projectId?: string | null;
}

interface NoteMeta {
  id: string;
  title: string;
  folder: string;
  tags: string[];
  updated_at: string;
}

type ViewMode = 'edit' | 'preview' | 'split';

interface FolderNode {
  name: string;
  path: string;
  folders: FolderNode[];
  notes: NoteMeta[];
}

type ContextMenuTarget =
  | { type: 'folder'; folderPath: string }
  | { type: 'note'; note: NoteMeta };

interface ContextMenuState {
  x: number;
  y: number;
  target: ContextMenuTarget;
}

const createFolderNode = (name: string, path: string): FolderNode => ({
  name,
  path,
  folders: [],
  notes: [],
});

const parseTags = (raw: string): string[] => (
  raw
    .split(',')
    .map((item) => item.trim())
    .filter((item) => item.length > 0)
);

const formatTime = (raw: string | undefined): string => {
  if (!raw) return '';
  const date = new Date(raw);
  if (Number.isNaN(date.getTime())) return raw;
  return date.toLocaleString();
};

const normalizeFolderPath = (raw: string | undefined | null): string => {
  const input = String(raw || '');
  return input.trim().replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
};

const sanitizeFileName = (raw: string): string => {
  const cleaned = String(raw || '')
    .replace(/[\\/:*?"<>|]+/g, '_')
    .replace(/\s+/g, ' ')
    .trim();
  return cleaned || 'note';
};

const noteUpdatedAtTs = (note: NoteMeta): number => {
  const value = Date.parse(note.updated_at || '');
  return Number.isNaN(value) ? 0 : value;
};

const buildFolderTree = (folders: string[], notes: NoteMeta[]): FolderNode => {
  const root = createFolderNode('', '');
  const nodeMap = new Map<string, FolderNode>();
  nodeMap.set('', root);

  const ensureNode = (rawPath: string): FolderNode => {
    const normalized = rawPath.trim().replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
    if (!normalized) {
      return root;
    }
    const cached = nodeMap.get(normalized);
    if (cached) {
      return cached;
    }

    const parts = normalized.split('/').filter((item) => item.trim().length > 0);
    let currentPath = '';
    let parentNode = root;
    for (const part of parts) {
      currentPath = currentPath ? `${currentPath}/${part}` : part;
      let currentNode = nodeMap.get(currentPath);
      if (!currentNode) {
        currentNode = createFolderNode(part, currentPath);
        parentNode.folders.push(currentNode);
        nodeMap.set(currentPath, currentNode);
      }
      parentNode = currentNode;
    }
    return parentNode;
  };

  for (const folder of folders) {
    ensureNode(folder);
  }

  for (const note of notes) {
    const folderNode = ensureNode(note.folder || '');
    folderNode.notes.push(note);
  }

  const sortNode = (node: FolderNode) => {
    node.folders.sort((left, right) => left.name.localeCompare(right.name, 'zh-Hans-CN'));
    node.notes.sort((left, right) => {
      const delta = noteUpdatedAtTs(right) - noteUpdatedAtTs(left);
      if (delta !== 0) {
        return delta;
      }
      return (left.title || '').localeCompare(right.title || '', 'zh-Hans-CN');
    });
    for (const child of node.folders) {
      sortNode(child);
    }
  };

  sortNode(root);
  return root;
};

const NotepadPanel: React.FC<NotepadPanelProps> = ({ isOpen, onClose, projectId }) => {
  const apiClientFromContext = useChatApiClientFromContext();
  const apiClient = useMemo(() => apiClientFromContext || globalApiClient, [apiClientFromContext]);

  const [folders, setFolders] = useState<string[]>([]);
  const [notes, setNotes] = useState<NoteMeta[]>([]);
  const [selectedFolder, setSelectedFolder] = useState<string>('');
  const [selectedNoteId, setSelectedNoteId] = useState<string>('');
  const [title, setTitle] = useState('');
  const [tagsText, setTagsText] = useState('');
  const [content, setContent] = useState('');
  const [searchQuery, setSearchQuery] = useState('');
  const [viewMode, setViewMode] = useState<ViewMode>('split');
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set(['']));
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);

  const scopedProjectId = projectId || undefined;
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

  const ensureFolderExpanded = useCallback((folderPath: string) => {
    const normalized = normalizeFolderPath(folderPath);
    setExpandedFolders((prev) => {
      const next = new Set(prev);
      next.add('');
      if (!normalized) {
        return next;
      }
      let current = '';
      const parts = normalized.split('/').filter((item) => item.trim().length > 0);
      for (const part of parts) {
        current = current ? `${current}/${part}` : part;
        next.add(current);
      }
      return next;
    });
  }, []);

  const resetEditor = useCallback(() => {
    setSelectedNoteId('');
    setTitle('');
    setTagsText('');
    setContent('');
    setDirty(false);
  }, []);

  const loadFolders = useCallback(async () => {
    const res = await apiClient.listNotepadFolders(scopedProjectId);
    const list = Array.isArray(res?.folders) ? res.folders : [];
    const normalized = list
      .map((item: any) => String(item || '').trim())
      .filter((item: string) => item.length > 0);
    setFolders(['', ...normalized]);
  }, [apiClient, scopedProjectId]);

  const loadNotes = useCallback(async () => {
    const res = await apiClient.listNotepadNotes({
      project_id: scopedProjectId,
      recursive: true,
      query: searchQuery || undefined,
      limit: 500,
    });
    const list = Array.isArray(res?.notes) ? res.notes : [];
    setNotes(list);
  }, [apiClient, scopedProjectId, searchQuery]);

  const refreshAll = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      await apiClient.notepadInit(scopedProjectId);
      await Promise.all([loadFolders(), loadNotes()]);
    } catch (err: any) {
      setError(err?.message || '加载记事本失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, loadFolders, loadNotes, scopedProjectId]);

  const openNote = useCallback(async (id: string) => {
    if (!id) return;
    setLoading(true);
    setError(null);
    try {
      const res = await apiClient.getNotepadNote(id, scopedProjectId);
      const note = res?.note || {};
      const noteFolder = String(note.folder || '');
      setSelectedNoteId(String(note.id || id));
      setSelectedFolder(noteFolder);
      setTitle(String(note.title || ''));
      setTagsText(Array.isArray(note.tags) ? note.tags.join(', ') : '');
      setContent(String(res?.content || ''));
      ensureFolderExpanded(noteFolder);
      setDirty(false);
    } catch (err: any) {
      setError(err?.message || '打开笔记失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, ensureFolderExpanded, scopedProjectId]);

  useEffect(() => {
    if (!isOpen) return;
    void refreshAll();
  }, [isOpen, refreshAll]);

  useEffect(() => {
    if (!isOpen) return;
    void loadNotes();
  }, [isOpen, loadNotes]);

  useEffect(() => {
    if (!selectedFolder) return;
    ensureFolderExpanded(selectedFolder);
  }, [ensureFolderExpanded, selectedFolder]);

  useEffect(() => {
    if (!contextMenu) return undefined;

    const closeMenu = () => setContextMenu(null);
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        closeMenu();
      }
    };

    window.addEventListener('click', closeMenu);
    window.addEventListener('resize', closeMenu);
    window.addEventListener('scroll', closeMenu, true);
    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('click', closeMenu);
      window.removeEventListener('resize', closeMenu);
      window.removeEventListener('scroll', closeMenu, true);
      window.removeEventListener('keydown', onKeyDown);
    };
  }, [contextMenu]);

  const toggleFolderExpanded = useCallback((folderPath: string) => {
    const normalized = normalizeFolderPath(folderPath);
    setExpandedFolders((prev) => {
      const next = new Set(prev);
      if (next.has(normalized)) {
        next.delete(normalized);
      } else {
        next.add(normalized);
      }
      return next;
    });
  }, []);

  const handleCreateFolder = useCallback(async (parentFolder?: string) => {
    const baseFolder = normalizeFolderPath(parentFolder ?? selectedFolder);
    const promptTitle = baseFolder
      ? `在目录 "${baseFolder}" 下新建子目录（支持输入相对路径）`
      : '请输入新文件夹路径（例如 work/ideas）';
    const raw = window.prompt(promptTitle, '');
    if (raw === null) return;
    const input = normalizeFolderPath(raw);
    if (!input) return;

    const folder = baseFolder && !input.startsWith(`${baseFolder}/`) && input !== baseFolder
      ? `${baseFolder}/${input}`
      : input;
    setLoading(true);
    setError(null);
    try {
      await apiClient.createNotepadFolder({
        folder,
        project_id: scopedProjectId,
      });
      setSelectedFolder(folder);
      ensureFolderExpanded(folder);
      await Promise.all([loadFolders(), loadNotes()]);
    } catch (err: any) {
      setError(err?.message || '创建文件夹失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, ensureFolderExpanded, loadFolders, loadNotes, scopedProjectId, selectedFolder]);

  const handleCreateNote = useCallback(async (folderOverride?: string) => {
    const targetFolder = normalizeFolderPath(folderOverride ?? selectedFolder);
    const noteTitle = window.prompt('请输入笔记标题', '');
    if (noteTitle === null) return;
    setLoading(true);
    setError(null);
    try {
      const res = await apiClient.createNotepadNote({
        project_id: scopedProjectId,
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
    } catch (err: any) {
      setError(err?.message || '创建笔记失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, ensureFolderExpanded, loadNotes, openNote, resetEditor, scopedProjectId, selectedFolder]);

  const handleSaveNote = useCallback(async () => {
    if (!selectedNoteId) return;
    setLoading(true);
    setError(null);
    try {
      await apiClient.updateNotepadNote(selectedNoteId, {
        project_id: scopedProjectId,
        title: title.trim(),
        content,
        tags: parseTags(tagsText),
      });
      setDirty(false);
      await loadNotes();
    } catch (err: any) {
      setError(err?.message || '保存笔记失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, content, loadNotes, scopedProjectId, selectedNoteId, tagsText, title]);

  const resolveNotePayload = useCallback(async (note?: NoteMeta | null): Promise<{ title: string; content: string }> => {
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

    const res = await apiClient.getNotepadNote(targetNoteId, scopedProjectId);
    const remoteNote = res?.note || {};
    return {
      title: String(remoteNote.title || fallbackTitle || 'Untitled'),
      content: String(res?.content || ''),
    };
  }, [apiClient, content, scopedProjectId, selectedNoteId, title]);

  const copyTextToClipboard = useCallback(async (text: string) => {
    if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return;
    }

    if (typeof document !== 'undefined') {
      const textarea = document.createElement('textarea');
      textarea.value = text;
      textarea.style.position = 'fixed';
      textarea.style.opacity = '0';
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand('copy');
      document.body.removeChild(textarea);
      return;
    }

    throw new Error('clipboard is not available');
  }, []);

  const handleCopyText = useCallback(async (note?: NoteMeta | null) => {
    try {
      const payload = await resolveNotePayload(note);
      await copyTextToClipboard(payload.content || '');
    } catch (err: any) {
      setError(err?.message || '复制文本失败');
    }
  }, [copyTextToClipboard, resolveNotePayload]);

  const handleCopyAsMdFile = useCallback(async (note?: NoteMeta | null) => {
    try {
      const payload = await resolveNotePayload(note);
      if (typeof document === 'undefined') {
        throw new Error('document is not available');
      }
      const filename = `${sanitizeFileName(payload.title)}.md`;
      const blob = new Blob([payload.content || ''], { type: 'text/markdown;charset=utf-8' });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = filename;
      anchor.style.display = 'none';
      document.body.appendChild(anchor);
      anchor.click();
      document.body.removeChild(anchor);
      URL.revokeObjectURL(url);
    } catch (err: any) {
      setError(err?.message || '导出 .md 失败');
    }
  }, [resolveNotePayload]);

  const handleDeleteNoteById = useCallback(async (noteId: string, titleHint?: string) => {
    const normalizedId = String(noteId || '').trim();
    if (!normalizedId) return;
    const confirmed = window.confirm(`确认删除笔记“${titleHint || '当前笔记'}”？此操作不可恢复。`);
    if (!confirmed) return;

    setLoading(true);
    setError(null);
    try {
      await apiClient.deleteNotepadNote(normalizedId, scopedProjectId);
      if (selectedNoteId === normalizedId) {
        resetEditor();
      }
      await loadNotes();
    } catch (err: any) {
      setError(err?.message || '删除笔记失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, loadNotes, resetEditor, scopedProjectId, selectedNoteId]);

  const handleDeleteNote = useCallback(async () => {
    if (!selectedNoteId) return;
    const target = notes.find((item) => item.id === selectedNoteId);
    await handleDeleteNoteById(selectedNoteId, target?.title || undefined);
  }, [handleDeleteNoteById, notes, selectedNoteId]);

  const handleDeleteFolder = useCallback(async (folderPath?: string) => {
    const folder = normalizeFolderPath(folderPath ?? selectedFolder);
    if (!folder) {
      return;
    }

    const confirmed = window.confirm(`确认删除目录“${folder}”吗？会同时删除该目录下所有笔记。`);
    if (!confirmed) {
      return;
    }

    setLoading(true);
    setError(null);
    try {
      await apiClient.deleteNotepadFolder({
        folder,
        recursive: true,
        project_id: scopedProjectId,
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
    } catch (err: any) {
      setError(err?.message || '删除目录失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, loadFolders, loadNotes, notes, resetEditor, scopedProjectId, selectedFolder, selectedNoteId]);

  const openContextMenu = useCallback((event: React.MouseEvent, target: ContextMenuTarget) => {
    event.preventDefault();
    event.stopPropagation();
    setContextMenu({
      x: event.clientX,
      y: event.clientY,
      target,
    });
  }, []);

  const handleContextCreateFolder = useCallback(async () => {
    if (!contextMenu) return;
    const baseFolder = contextMenu.target.type === 'folder'
      ? contextMenu.target.folderPath
      : contextMenu.target.note.folder;
    setContextMenu(null);
    await handleCreateFolder(baseFolder);
  }, [contextMenu, handleCreateFolder]);

  const handleContextCreateNote = useCallback(async () => {
    if (!contextMenu) return;
    const folder = contextMenu.target.type === 'folder'
      ? contextMenu.target.folderPath
      : contextMenu.target.note.folder;
    setContextMenu(null);
    await handleCreateNote(folder);
  }, [contextMenu, handleCreateNote]);

  const handleContextDelete = useCallback(async () => {
    if (!contextMenu) return;
    const target = contextMenu.target;
    setContextMenu(null);
    if (target.type === 'folder') {
      await handleDeleteFolder(target.folderPath);
      return;
    }
    await handleDeleteNoteById(target.note.id, target.note.title || undefined);
  }, [contextMenu, handleDeleteFolder, handleDeleteNoteById]);

  const handleContextDeleteSelectedNote = useCallback(async () => {
    if (!selectedNoteMeta) return;
    setContextMenu(null);
    await handleDeleteNoteById(selectedNoteMeta.id, selectedNoteMeta.title || undefined);
  }, [handleDeleteNoteById, selectedNoteMeta]);

  const handleContextCopyText = useCallback(async () => {
    if (!contextMenu) return;
    const targetNote = contextMenu.target.type === 'note'
      ? contextMenu.target.note
      : selectedNoteMeta;
    setContextMenu(null);
    if (!targetNote) {
      setError('请先选中笔记');
      return;
    }
    await handleCopyText(targetNote);
  }, [contextMenu, handleCopyText, selectedNoteMeta]);

  const handleContextCopyAsMd = useCallback(async () => {
    if (!contextMenu) return;
    const targetNote = contextMenu.target.type === 'note'
      ? contextMenu.target.note
      : selectedNoteMeta;
    setContextMenu(null);
    if (!targetNote) {
      setError('请先选中笔记');
      return;
    }
    await handleCopyAsMdFile(targetNote);
  }, [contextMenu, handleCopyAsMdFile, selectedNoteMeta]);

  const contextMenuStyle = useMemo(() => {
    if (!contextMenu) {
      return undefined;
    }
    const maxX = typeof window !== 'undefined' ? window.innerWidth - 220 : contextMenu.x;
    const maxY = typeof window !== 'undefined' ? window.innerHeight - 190 : contextMenu.y;
    return {
      left: `${Math.max(8, Math.min(contextMenu.x, maxX))}px`,
      top: `${Math.max(8, Math.min(contextMenu.y, maxY))}px`,
    };
  }, [contextMenu]);

  if (!isOpen) return null;

  return (
    <>
      <div className="fixed inset-0 bg-black/50 z-40" onClick={onClose} />
      <div className="fixed inset-x-10 top-10 bottom-10 bg-card z-50 rounded-lg border border-border shadow-xl flex overflow-hidden">
        <div className="w-[320px] border-r border-border flex flex-col">
          <div className="px-4 py-3 border-b border-border flex items-center justify-between">
            <div className="text-sm font-semibold text-foreground">记事本</div>
            <button
              type="button"
              onClick={onClose}
              className="px-2 py-1 text-xs rounded border border-border hover:bg-accent"
            >
              关闭
            </button>
          </div>

          <div className="p-3 border-b border-border space-y-2">
            <div className="flex gap-2">
              <button
                type="button"
                onClick={() => { void handleCreateFolder(); }}
                className="flex-1 px-2 py-1.5 text-xs rounded border border-border hover:bg-accent"
              >
                新建文件夹
              </button>
              <button
                type="button"
                onClick={() => { void handleCreateNote(); }}
                className="flex-1 px-2 py-1.5 text-xs rounded bg-indigo-600 text-white hover:bg-indigo-700"
              >
                新建笔记
              </button>
            </div>
            <input
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              placeholder="搜索标题/文件夹"
              className="w-full h-9 rounded border border-input bg-background px-2 text-sm"
            />
            <div className="text-[11px] text-muted-foreground truncate" title={selectedFolder || 'root'}>
              当前目录：{selectedFolder || 'root'}
            </div>
          </div>

          <div className="flex-1 overflow-y-auto p-2">
            {loading && notes.length === 0 ? (
              <div className="text-xs text-muted-foreground p-2">加载中...</div>
            ) : notes.length === 0 && availableFolders.length === 0 ? (
              <div className="text-xs text-muted-foreground p-2">暂无笔记</div>
            ) : (
              <div className="space-y-0.5">
                {folderTree.folders.map((folder) => {
                  const renderFolder = (node: FolderNode, depth: number): React.ReactNode => {
                    const folderKey = node.path || '__root__';
                    const expanded = expandedFolders.has(node.path);
                    const hasChildren = node.folders.length > 0 || node.notes.length > 0;
                    const indent = 8 + depth * 14;

                    return (
                      <div key={folderKey}>
                        <div
                          className={`group flex items-center gap-1 rounded px-1 py-1 ${
                            selectedFolder === node.path
                              ? 'bg-indigo-500/10 text-indigo-600 dark:text-indigo-300'
                              : 'hover:bg-accent'
                          }`}
                          style={{ paddingLeft: `${indent}px` }}
                          onContextMenu={(event) => {
                            setSelectedFolder(node.path);
                            ensureFolderExpanded(node.path);
                            openContextMenu(event, { type: 'folder', folderPath: node.path });
                          }}
                        >
                          <button
                            type="button"
                            onClick={(event) => {
                              event.stopPropagation();
                              if (hasChildren) {
                                toggleFolderExpanded(node.path);
                              }
                            }}
                            className="w-4 h-4 text-[10px] text-muted-foreground hover:text-foreground"
                            title={hasChildren ? (expanded ? '收起目录' : '展开目录') : '空目录'}
                          >
                            {hasChildren ? (expanded ? '▾' : '▸') : '·'}
                          </button>
                          <button
                            type="button"
                            onClick={() => {
                              setSelectedFolder(node.path);
                              ensureFolderExpanded(node.path);
                            }}
                            className="flex-1 min-w-0 text-left text-sm truncate"
                            title={node.path}
                          >
                            {node.name}
                          </button>
                        </div>
                        {expanded && (
                          <>
                            {node.folders.map((child) => renderFolder(child, depth + 1))}
                            {node.notes.map((note) => (
                              <button
                                key={note.id}
                                type="button"
                                onClick={() => { void openNote(note.id); }}
                                onContextMenu={(event) => {
                                  openContextMenu(event, { type: 'note', note });
                                }}
                                className={`w-full text-left rounded px-2 py-1.5 ${
                                  selectedNoteId === note.id
                                    ? 'bg-indigo-500/10 border border-indigo-500/50'
                                    : 'hover:bg-accent border border-transparent'
                                }`}
                                style={{ paddingLeft: `${indent + 18}px` }}
                                title={note.title || 'Untitled'}
                              >
                                <div className="text-sm text-foreground truncate">📄 {note.title || 'Untitled'}</div>
                                <div className="text-[10px] text-muted-foreground truncate">
                                  {note.updated_at ? formatTime(note.updated_at) : ''}
                                </div>
                              </button>
                            ))}
                          </>
                        )}
                      </div>
                    );
                  };

                  return renderFolder(folder, 0);
                })}

                {folderTree.notes.map((note) => (
                  <button
                    key={note.id}
                    type="button"
                    onClick={() => { void openNote(note.id); }}
                    onContextMenu={(event) => {
                      openContextMenu(event, { type: 'note', note });
                    }}
                    className={`w-full text-left rounded px-2 py-1.5 ${
                      selectedNoteId === note.id
                        ? 'bg-indigo-500/10 border border-indigo-500/50'
                        : 'hover:bg-accent border border-transparent'
                    }`}
                    style={{ paddingLeft: '26px' }}
                    title={note.title || 'Untitled'}
                  >
                    <div className="text-sm text-foreground truncate">📄 {note.title || 'Untitled'}</div>
                    <div className="text-[10px] text-muted-foreground truncate">
                      {note.updated_at ? formatTime(note.updated_at) : ''}
                    </div>
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>

        <div className="flex-1 flex flex-col min-w-0">
          <div className="px-4 py-3 border-b border-border flex items-center justify-between">
            <div className="text-sm text-foreground font-medium">
              {selectedNoteId ? '编辑笔记' : '请选择或创建笔记'}
            </div>
            <div className="flex items-center gap-2">
              <div className="flex items-center rounded border border-border overflow-hidden">
                <button
                  type="button"
                  onClick={() => setViewMode('edit')}
                  className={`px-2 py-1 text-xs ${
                    viewMode === 'edit' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/70'
                  }`}
                >
                  编辑
                </button>
                <button
                  type="button"
                  onClick={() => setViewMode('preview')}
                  className={`px-2 py-1 text-xs border-l border-border ${
                    viewMode === 'preview' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/70'
                  }`}
                >
                  预览
                </button>
                <button
                  type="button"
                  onClick={() => setViewMode('split')}
                  className={`px-2 py-1 text-xs border-l border-border ${
                    viewMode === 'split' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/70'
                  }`}
                >
                  分栏
                </button>
              </div>
              <button
                type="button"
                onClick={() => { void refreshAll(); }}
                className="px-3 py-1.5 text-xs rounded border border-border hover:bg-accent"
              >
                刷新
              </button>
              <button
                type="button"
                onClick={() => { void handleCopyText(selectedNoteMeta); }}
                disabled={!selectedNoteId}
                className="px-3 py-1.5 text-xs rounded border border-border hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                复制文本
              </button>
              <button
                type="button"
                onClick={() => { void handleCopyAsMdFile(selectedNoteMeta); }}
                disabled={!selectedNoteId}
                className="px-3 py-1.5 text-xs rounded border border-border hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                复制为.md
              </button>
              <button
                type="button"
                onClick={() => { void handleSaveNote(); }}
                disabled={!selectedNoteId || !dirty}
                className="px-3 py-1.5 text-xs rounded bg-emerald-600 text-white hover:bg-emerald-700 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                保存
              </button>
              <button
                type="button"
                onClick={() => { void handleDeleteNote(); }}
                disabled={!selectedNoteId}
                className="px-3 py-1.5 text-xs rounded bg-destructive text-destructive-foreground hover:bg-destructive/90 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                删除
              </button>
            </div>
          </div>

          {error ? (
            <div className="mx-4 mt-3 px-3 py-2 text-xs rounded border border-destructive/30 bg-destructive/10 text-destructive">
              {error}
            </div>
          ) : null}

          {selectedNoteId ? (
            <div className="flex-1 min-h-0 flex flex-col p-4 gap-3">
              <input
                value={title}
                onChange={(event) => {
                  setTitle(event.target.value);
                  setDirty(true);
                }}
                placeholder="标题"
                className="h-10 rounded border border-input bg-background px-3 text-sm"
              />
              <input
                value={tagsText}
                onChange={(event) => {
                  setTagsText(event.target.value);
                  setDirty(true);
                }}
                placeholder="标签（用逗号分隔）"
                className="h-10 rounded border border-input bg-background px-3 text-sm"
              />
              <div className={`flex-1 min-h-0 ${viewMode === 'split' ? 'grid grid-cols-2 gap-3' : 'flex'}`}>
                {(viewMode === 'edit' || viewMode === 'split') && (
                  <textarea
                    value={content}
                    onChange={(event) => {
                      setContent(event.target.value);
                      setDirty(true);
                    }}
                    placeholder="Markdown 内容"
                    className={`min-h-0 rounded border border-input bg-background p-3 text-sm leading-6 resize-none ${
                      viewMode === 'split' ? 'h-full w-full' : 'flex-1 w-full'
                    }`}
                  />
                )}
                {(viewMode === 'preview' || viewMode === 'split') && (
                  <div className={`min-h-0 rounded border border-input bg-background p-3 overflow-y-auto ${
                    viewMode === 'split' ? 'h-full w-full' : 'flex-1 w-full'
                  }`}>
                    <MarkdownRenderer content={content || '（空内容）'} />
                  </div>
                )}
              </div>
            </div>
          ) : (
            <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
              在左侧选择笔记，或点击“新建笔记”。
            </div>
          )}
        </div>
      </div>

      {contextMenu && contextMenuStyle && (
        <div
          className="fixed z-[80] w-56 rounded-md border border-border bg-popover text-popover-foreground shadow-lg p-1"
          style={contextMenuStyle}
          onClick={(event) => event.stopPropagation()}
          onContextMenu={(event) => event.preventDefault()}
        >
          <div className="px-2 py-1 text-[11px] text-muted-foreground truncate">
            {contextMenu.target.type === 'folder'
              ? `目录：${contextMenu.target.folderPath || 'root'}`
              : `笔记：${contextMenu.target.note.title || 'Untitled'}`}
          </div>
          <button
            type="button"
            onClick={() => { void handleContextCreateFolder(); }}
            className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
          >
            新建目录
          </button>
          <button
            type="button"
            onClick={() => { void handleContextCreateNote(); }}
            className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
          >
            新建笔记
          </button>
          {(contextMenu.target.type === 'note' || selectedNoteMeta) && (
            <button
              type="button"
              onClick={() => { void handleContextCopyText(); }}
              className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
            >
              复制文本
            </button>
          )}
          {(contextMenu.target.type === 'note' || selectedNoteMeta) && (
            <button
              type="button"
              onClick={() => { void handleContextCopyAsMd(); }}
              className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
            >
              复制为.md 文件
            </button>
          )}
          <button
            type="button"
            onClick={() => { void handleContextDelete(); }}
            className="w-full text-left px-2 py-1.5 text-sm rounded text-destructive hover:bg-destructive/10"
          >
            {contextMenu.target.type === 'folder' ? '删除当前目录' : '删除当前笔记'}
          </button>
          {contextMenu.target.type === 'folder' && selectedNoteMeta && (
            <button
              type="button"
              onClick={() => { void handleContextDeleteSelectedNote(); }}
              className="w-full text-left px-2 py-1.5 text-sm rounded text-destructive hover:bg-destructive/10"
            >
              删除当前选中笔记
            </button>
          )}
        </div>
      )}
    </>
  );
};

export default NotepadPanel;
