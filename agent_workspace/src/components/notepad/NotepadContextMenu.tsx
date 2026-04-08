import React from 'react';
import type { NoteMeta } from './utils';

export type ContextMenuTarget =
  | { type: 'folder'; folderPath: string }
  | { type: 'note'; note: NoteMeta };

export interface ContextMenuState {
  x: number;
  y: number;
  target: ContextMenuTarget;
}

interface NotepadContextMenuProps {
  contextMenu: ContextMenuState | null;
  contextMenuStyle?: React.CSSProperties;
  selectedNoteMeta: NoteMeta | null;
  onContextCreateFolder: () => void;
  onContextCreateNote: () => void;
  onContextCopyText: () => void;
  onContextCopyAsMd: () => void;
  onContextDelete: () => void;
  onContextDeleteSelectedNote: () => void;
}

export const NotepadContextMenu: React.FC<NotepadContextMenuProps> = ({
  contextMenu,
  contextMenuStyle,
  selectedNoteMeta,
  onContextCreateFolder,
  onContextCreateNote,
  onContextCopyText,
  onContextCopyAsMd,
  onContextDelete,
  onContextDeleteSelectedNote,
}) => {
  if (!contextMenu || !contextMenuStyle) {
    return null;
  }

  return (
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
        onClick={onContextCreateFolder}
        className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
      >
        新建目录
      </button>
      <button
        type="button"
        onClick={onContextCreateNote}
        className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
      >
        新建笔记
      </button>
      {(contextMenu.target.type === 'note' || selectedNoteMeta) && (
        <button
          type="button"
          onClick={onContextCopyText}
          className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
        >
          复制文本
        </button>
      )}
      {(contextMenu.target.type === 'note' || selectedNoteMeta) && (
        <button
          type="button"
          onClick={onContextCopyAsMd}
          className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
        >
          复制为.md 文件
        </button>
      )}
      <button
        type="button"
        onClick={onContextDelete}
        className="w-full text-left px-2 py-1.5 text-sm rounded text-destructive hover:bg-destructive/10"
      >
        {contextMenu.target.type === 'folder' ? '删除当前目录' : '删除当前笔记'}
      </button>
      {contextMenu.target.type === 'folder' && selectedNoteMeta && (
        <button
          type="button"
          onClick={onContextDeleteSelectedNote}
          className="w-full text-left px-2 py-1.5 text-sm rounded text-destructive hover:bg-destructive/10"
        >
          删除当前选中笔记
        </button>
      )}
    </div>
  );
};
