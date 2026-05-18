import React from 'react';

import type { FsEntry } from '../../types';

export interface MoveConflictState {
  sourcePath: string;
  targetDirPath: string;
  sourceName: string;
  renameTo: string;
}

interface MoveConflictModalProps {
  moveConflict: MoveConflictState | null;
  actionLoading: boolean;
  onCancel: () => void;
  onRenameChange: (value: string) => void;
  onOverwrite: () => void;
  onRename: () => void;
}

export const MoveConflictModal: React.FC<MoveConflictModalProps> = ({
  moveConflict,
  actionLoading,
  onCancel,
  onRenameChange,
  onOverwrite,
  onRename,
}) => {
  if (!moveConflict) {
    return null;
  }

  return (
    <div
      className="fixed inset-0 z-[90] bg-black/35 flex items-center justify-center p-4"
      onClick={() => {
        if (!actionLoading) {
          onCancel();
        }
      }}
    >
      <div
        className="w-full max-w-md rounded-lg border border-border bg-card p-4 shadow-xl"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="text-sm font-medium text-foreground">目标目录存在同名项</div>
        <div className="mt-2 text-xs text-muted-foreground">
          将 {moveConflict.sourceName} 移动到目标目录时发生冲突，请选择处理方式。
        </div>
        <div className="mt-3 space-y-1.5">
          <label className="text-xs text-muted-foreground">重命名后移动</label>
          <input
            value={moveConflict.renameTo}
            onChange={(event) => onRenameChange(event.target.value)}
            className="w-full h-9 rounded border border-input bg-background px-2 text-sm"
            placeholder="请输入新名称"
          />
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            onClick={onCancel}
            disabled={actionLoading}
            className="px-3 py-1.5 text-xs rounded border border-border hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
          >
            取消
          </button>
          <button
            type="button"
            onClick={onOverwrite}
            disabled={actionLoading}
            className="px-3 py-1.5 text-xs rounded border border-amber-500/50 text-amber-700 hover:bg-amber-500/10 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            覆盖后移动
          </button>
          <button
            type="button"
            onClick={onRename}
            disabled={actionLoading}
            className="px-3 py-1.5 text-xs rounded bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            重命名后移动
          </button>
        </div>
      </div>
    </div>
  );
};

interface EntryContextMenuState {
  x: number;
  y: number;
  entry: FsEntry;
}

interface EntryContextMenuProps {
  contextMenu: EntryContextMenuState | null;
  contextMenuStyle: React.CSSProperties | undefined;
  isContextRootEntry: boolean;
  onCreateDirectory: (path: string) => void;
  onCreateFile: (path: string) => void;
  onDownload: (entry: FsEntry) => void;
  onDelete: (entry: FsEntry) => void;
}

export const EntryContextMenu: React.FC<EntryContextMenuProps> = ({
  contextMenu,
  contextMenuStyle,
  isContextRootEntry,
  onCreateDirectory,
  onCreateFile,
  onDownload,
  onDelete,
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
        {contextMenu.entry.isDir ? '目录' : '文件'}：{contextMenu.entry.path}
      </div>
      {contextMenu.entry.isDir && (
        <button
          type="button"
          onClick={() => onCreateDirectory(contextMenu.entry.path)}
          className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
        >
          新建目录
        </button>
      )}
      {contextMenu.entry.isDir && (
        <button
          type="button"
          onClick={() => onCreateFile(contextMenu.entry.path)}
          className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
        >
          新建文件
        </button>
      )}
      <button
        type="button"
        onClick={() => onDownload(contextMenu.entry)}
        className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
      >
        下载
      </button>
      <button
        type="button"
        onClick={() => onDelete(contextMenu.entry)}
        disabled={isContextRootEntry}
        className="w-full text-left px-2 py-1.5 text-sm rounded text-destructive hover:bg-destructive/10 disabled:opacity-50 disabled:cursor-not-allowed"
      >
        删除
      </button>
    </div>
  );
};
