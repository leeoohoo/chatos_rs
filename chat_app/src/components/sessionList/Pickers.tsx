import React from 'react';

import type { FsEntry } from '../../types';
import type { DirPickerTarget } from './helpers';

interface KeyFilePickerDialogProps {
  isOpen: boolean;
  title: string;
  currentPath: string | null;
  parentPath: string | null;
  loading: boolean;
  items: FsEntry[];
  error: string | null;
  onClose: () => void;
  onBack: () => void;
  onRefresh: () => void;
  onEntryClick: (entry: FsEntry) => void;
  onSelectFile: (path: string) => void;
}

export const KeyFilePickerDialog: React.FC<KeyFilePickerDialogProps> = ({
  isOpen,
  title,
  currentPath,
  parentPath,
  loading,
  items,
  error,
  onClose,
  onBack,
  onRefresh,
  onEntryClick,
  onSelectFile,
}) => {
  if (!isOpen) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center">
      <div className="fixed inset-0 bg-black/50" onClick={onClose} />
      <div className="relative bg-card border border-border rounded-lg shadow-xl w-[680px] max-h-[82vh] p-6 flex flex-col">
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-lg font-semibold text-foreground">{title}</h3>
          <button
            onClick={onClose}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
          >
            <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <div className="text-xs text-muted-foreground break-all">
          当前路径：<span className="text-foreground">{currentPath || '请选择磁盘/目录'}</span>
        </div>
        <div className="mt-3 flex items-center gap-2">
          <button
            type="button"
            onClick={onBack}
            disabled={!parentPath}
            className="px-3 py-1.5 rounded bg-muted text-muted-foreground hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
          >
            返回上级
          </button>
          <button
            type="button"
            onClick={onRefresh}
            className="px-3 py-1.5 rounded bg-muted text-muted-foreground hover:bg-accent"
          >
            刷新
          </button>
        </div>
        <div className="mt-3 flex-1 overflow-y-auto border border-border rounded">
          {loading && <div className="p-4 text-sm text-muted-foreground">加载中...</div>}
          {!loading && items.length === 0 && (
            <div className="p-4 text-sm text-muted-foreground">没有可用文件</div>
          )}
          {!loading && items.length > 0 && (
            <div className="divide-y divide-border">
              {items.map((entry) => (
                <div
                  key={entry.path}
                  className="px-4 py-2 hover:bg-accent flex items-center justify-between gap-3"
                >
                  <button
                    type="button"
                    onClick={() => onEntryClick(entry)}
                    className="flex-1 text-left"
                  >
                    <span className="text-foreground truncate block">
                      {entry.isDir ? '📁' : '🔑'} {entry.name || entry.path}
                    </span>
                    <span className="text-[11px] text-muted-foreground truncate block">{entry.path}</span>
                  </button>
                  {!entry.isDir && (
                    <button
                      type="button"
                      onClick={() => onSelectFile(entry.path)}
                      className="px-2.5 py-1 rounded border border-border text-xs text-foreground hover:bg-accent"
                    >
                      选择
                    </button>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
        {error && <div className="mt-2 text-xs text-destructive">{error}</div>}
      </div>
    </div>
  );
};

interface DirPickerDialogProps {
  isOpen: boolean;
  target: DirPickerTarget;
  currentPath: string | null;
  parentPath: string | null;
  loading: boolean;
  items: FsEntry[];
  error: string | null;
  showHiddenDirs: boolean;
  createModalOpen: boolean;
  newFolderName: string;
  creatingFolder: boolean;
  onClose: () => void;
  onBack: () => void;
  onChooseCurrent: () => void;
  onOpenCreateModal: () => void;
  onToggleHiddenDirs: () => void;
  onOpenEntry: (path: string) => void;
  onCreateModalClose: () => void;
  onNewFolderNameChange: (value: string) => void;
  onCreateDir: () => void;
}

export const DirPickerDialog: React.FC<DirPickerDialogProps> = ({
  isOpen,
  target,
  currentPath,
  parentPath,
  loading,
  items,
  error,
  showHiddenDirs,
  createModalOpen,
  newFolderName,
  creatingFolder,
  onClose,
  onBack,
  onChooseCurrent,
  onOpenCreateModal,
  onToggleHiddenDirs,
  onOpenEntry,
  onCreateModalClose,
  onNewFolderNameChange,
  onCreateDir,
}) => {
  if (!isOpen) {
    return null;
  }

  const canCreate = target === 'project';

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="fixed inset-0 bg-black/50" onClick={onClose} />
      <div className="relative bg-card border border-border rounded-lg shadow-xl w-[640px] max-h-[80vh] p-6 flex flex-col">
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-lg font-semibold text-foreground">
            {target === 'terminal' ? '选择终端目录' : '选择项目目录'}
          </h3>
          <button onClick={onClose} className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors">
            <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <div className="text-xs text-muted-foreground break-all">
          当前路径：<span className="text-foreground">{currentPath || '请选择盘符/目录'}</span>
        </div>
        <div className="mt-3 flex items-center gap-2">
          <button
            type="button"
            onClick={onBack}
            disabled={!parentPath}
            className="px-3 py-1.5 rounded bg-muted text-muted-foreground hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
          >
            返回上级
          </button>
          <button
            type="button"
            onClick={onChooseCurrent}
            disabled={!currentPath}
            className="px-3 py-1.5 rounded bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            选择当前目录
          </button>
          {canCreate && (
            <button
              type="button"
              onClick={onOpenCreateModal}
              disabled={!currentPath || creatingFolder}
              className="px-3 py-1.5 rounded bg-emerald-600 text-white hover:bg-emerald-700 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {creatingFolder ? '新建中...' : '新建目录'}
            </button>
          )}
          {canCreate && (
            <button
              type="button"
              onClick={onToggleHiddenDirs}
              className="px-3 py-1.5 rounded bg-muted text-muted-foreground hover:bg-accent"
            >
              {showHiddenDirs ? '不显示隐藏目录' : '显示隐藏目录'}
            </button>
          )}
        </div>
        <div className="mt-3 flex-1 overflow-y-auto border border-border rounded">
          {loading && <div className="p-4 text-sm text-muted-foreground">加载中...</div>}
          {!loading && items.length === 0 && (
            <div className="p-4 text-sm text-muted-foreground">没有可用目录</div>
          )}
          {!loading && items.length > 0 && (
            <div className="divide-y divide-border">
              {items.map((entry) => (
                <button
                  key={entry.path}
                  type="button"
                  onClick={() => onOpenEntry(entry.path)}
                  className="w-full text-left px-4 py-2 hover:bg-accent flex items-center gap-2"
                >
                  <span className="text-foreground">{entry.name}</span>
                </button>
              ))}
            </div>
          )}
        </div>
        {error && !createModalOpen && (
          <div className="mt-2 text-xs text-red-500">{error}</div>
        )}

        {createModalOpen && (
          <div className="absolute inset-0 z-10 flex items-center justify-center">
            <div className="absolute inset-0 bg-black/40" onClick={() => !creatingFolder && onCreateModalClose()} />
            <div className="relative w-[420px] max-w-[90%] rounded-lg border border-border bg-card p-4 shadow-xl">
              <div className="text-sm font-medium text-foreground mb-2">新建目录</div>
              <div className="text-xs text-muted-foreground mb-3 break-all">
                当前路径：<span className="text-foreground">{currentPath || '-'}</span>
              </div>
              <input
                autoFocus
                value={newFolderName}
                onChange={(e) => onNewFolderNameChange(e.target.value)}
                placeholder="请输入新目录名称"
                className="w-full px-3 py-2 rounded border border-border bg-background text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-ring"
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    onCreateDir();
                  } else if (e.key === 'Escape' && !creatingFolder) {
                    e.preventDefault();
                    onCreateModalClose();
                  }
                }}
              />
              {error && <div className="mt-2 text-xs text-red-500">{error}</div>}
              <div className="mt-4 flex justify-end gap-2">
                <button
                  type="button"
                  onClick={onCreateModalClose}
                  disabled={creatingFolder}
                  className="px-3 py-1.5 rounded bg-muted text-muted-foreground hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  取消
                </button>
                <button
                  type="button"
                  onClick={onCreateDir}
                  disabled={creatingFolder}
                  className="px-3 py-1.5 rounded bg-emerald-600 text-white hover:bg-emerald-700 disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  {creatingFolder ? '新建中...' : '确定'}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
