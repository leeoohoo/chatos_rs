import React from 'react';
import { NotepadTree } from './NotepadTree';
import type { FolderNode, NoteMeta } from './utils';

interface NotepadSidebarProps {
  onClose: () => void;
  onCreateFolder: () => void;
  onCreateNote: () => void;
  searchQuery: string;
  onSearchQueryChange: (value: string) => void;
  selectedFolder: string;
  loading: boolean;
  notesCount: number;
  availableFoldersCount: number;
  folderTree: FolderNode;
  selectedNoteId: string;
  expandedFolders: Set<string>;
  onToggleFolderExpanded: (folderPath: string) => void;
  onSelectFolder: (folderPath: string) => void;
  onOpenNote: (noteId: string) => void;
  onFolderContextMenu: (event: React.MouseEvent, folderPath: string) => void;
  onNoteContextMenu: (event: React.MouseEvent, note: NoteMeta) => void;
}

export const NotepadSidebar: React.FC<NotepadSidebarProps> = ({
  onClose,
  onCreateFolder,
  onCreateNote,
  searchQuery,
  onSearchQueryChange,
  selectedFolder,
  loading,
  notesCount,
  availableFoldersCount,
  folderTree,
  selectedNoteId,
  expandedFolders,
  onToggleFolderExpanded,
  onSelectFolder,
  onOpenNote,
  onFolderContextMenu,
  onNoteContextMenu,
}) => (
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
          onClick={onCreateFolder}
          className="flex-1 px-2 py-1.5 text-xs rounded border border-border hover:bg-accent"
        >
          新建文件夹
        </button>
        <button
          type="button"
          onClick={onCreateNote}
          className="flex-1 px-2 py-1.5 text-xs rounded bg-indigo-600 text-white hover:bg-indigo-700"
        >
          新建笔记
        </button>
      </div>
      <input
        value={searchQuery}
        onChange={(event) => onSearchQueryChange(event.target.value)}
        placeholder="搜索标题/文件夹"
        className="w-full h-9 rounded border border-input bg-background px-2 text-sm"
      />
      <div className="text-[11px] text-muted-foreground truncate" title={selectedFolder || 'root'}>
        当前目录：{selectedFolder || 'root'}
      </div>
    </div>

    <div className="flex-1 overflow-y-auto p-2">
      {loading && notesCount === 0 ? (
        <div className="text-xs text-muted-foreground p-2">加载中...</div>
      ) : notesCount === 0 && availableFoldersCount === 0 ? (
        <div className="text-xs text-muted-foreground p-2">暂无笔记</div>
      ) : (
        <div className="space-y-0.5">
          <NotepadTree
            folderTree={folderTree}
            selectedFolder={selectedFolder}
            selectedNoteId={selectedNoteId}
            expandedFolders={expandedFolders}
            onToggleFolderExpanded={onToggleFolderExpanded}
            onSelectFolder={onSelectFolder}
            onOpenNote={onOpenNote}
            onFolderContextMenu={onFolderContextMenu}
            onNoteContextMenu={onNoteContextMenu}
          />
        </div>
      )}
    </div>
  </div>
);
