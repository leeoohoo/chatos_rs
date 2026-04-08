import React from 'react';
import { cn } from '../../lib/utils';
import type { FsEntry } from '../../types';
import type { RemoteEntry } from './types';

interface RemoteBrowserPaneProps {
  remotePath: string;
  remoteParent: string | null;
  loadingRemote: boolean;
  remoteEntries: RemoteEntry[];
  selectedRemote: RemoteEntry | null;
  transfering: boolean;
  remoteActionLoading: boolean;
  onCreateRemoteDirectory: () => void;
  onRenameRemoteEntry: () => void;
  onDeleteRemoteEntry: () => void;
  onLoadRemoteParent: () => void;
  onRefreshRemote: () => void;
  onSelectRemote: (entry: RemoteEntry) => void;
  onEnterRemoteDirectory: (entry: RemoteEntry) => void;
}

export const RemoteBrowserPane: React.FC<RemoteBrowserPaneProps> = ({
  remotePath,
  remoteParent,
  loadingRemote,
  remoteEntries,
  selectedRemote,
  transfering,
  remoteActionLoading,
  onCreateRemoteDirectory,
  onRenameRemoteEntry,
  onDeleteRemoteEntry,
  onLoadRemoteParent,
  onRefreshRemote,
  onSelectRemote,
  onEnterRemoteDirectory,
}) => (
  <div className="flex-1 min-w-0 flex flex-col">
    <div className="border-b border-border px-3 py-2 flex items-center justify-between gap-2">
      <div className="text-xs text-muted-foreground truncate">远端：{remotePath}</div>
      <div className="flex items-center gap-1">
        <button
          type="button"
          onClick={onCreateRemoteDirectory}
          disabled={transfering || remoteActionLoading}
          className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
        >
          新建目录
        </button>
        <button
          type="button"
          onClick={onRenameRemoteEntry}
          disabled={!selectedRemote || transfering || remoteActionLoading}
          className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
        >
          重命名
        </button>
        <button
          type="button"
          onClick={onDeleteRemoteEntry}
          disabled={!selectedRemote || transfering || remoteActionLoading}
          className="rounded border border-border px-2 py-1 text-[11px] text-destructive hover:bg-destructive/10 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          删除
        </button>
        <button
          type="button"
          onClick={onLoadRemoteParent}
          disabled={!remoteParent}
          className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
        >
          上级
        </button>
        <button
          type="button"
          onClick={onRefreshRemote}
          className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent"
        >
          刷新
        </button>
      </div>
    </div>
    <div className="flex-1 overflow-auto">
      {loadingRemote ? (
        <div className="px-3 py-4 text-xs text-muted-foreground">加载中...</div>
      ) : remoteEntries.length === 0 ? (
        <div className="px-3 py-4 text-xs text-muted-foreground">空目录</div>
      ) : (
        <div className="p-2 space-y-1">
          {remoteEntries.map((entry) => (
            <button
              key={entry.path}
              type="button"
              onClick={() => onSelectRemote(entry)}
              onDoubleClick={() => {
                if (entry.isDir) {
                  onEnterRemoteDirectory(entry);
                }
              }}
              className={cn(
                'w-full text-left rounded border px-2 py-1.5 text-xs',
                selectedRemote?.path === entry.path ? 'border-blue-500 bg-blue-500/10' : 'border-border hover:bg-accent',
              )}
            >
              <div className="truncate text-foreground">
                {entry.isDir ? '📁' : '📄'} {entry.name}
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  </div>
);

interface LocalBrowserPaneProps {
  localPath: string | null;
  localParent: string | null;
  loadingLocal: boolean;
  localRoots: FsEntry[];
  localEntries: FsEntry[];
  selectedLocal: FsEntry | null;
  onLoadLocalParent: () => void;
  onRefreshLocal: () => void;
  onOpenLocalRoot: (entry: FsEntry) => void;
  onSelectLocal: (entry: FsEntry) => void;
  onEnterLocalDirectory: (entry: FsEntry) => void;
}

export const LocalBrowserPane: React.FC<LocalBrowserPaneProps> = ({
  localPath,
  localParent,
  loadingLocal,
  localRoots,
  localEntries,
  selectedLocal,
  onLoadLocalParent,
  onRefreshLocal,
  onOpenLocalRoot,
  onSelectLocal,
  onEnterLocalDirectory,
}) => (
  <div className="flex-1 min-w-0 flex flex-col">
    <div className="border-b border-border px-3 py-2 flex items-center justify-between gap-2">
      <div className="text-xs text-muted-foreground truncate">本地：{localPath || '请选择根目录'}</div>
      <div className="flex items-center gap-1">
        <button
          type="button"
          onClick={onLoadLocalParent}
          disabled={!localParent && localPath !== null}
          className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
        >
          上级
        </button>
        <button
          type="button"
          onClick={onRefreshLocal}
          className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent"
        >
          刷新
        </button>
      </div>
    </div>
    <div className="flex-1 overflow-auto">
      {loadingLocal ? (
        <div className="px-3 py-4 text-xs text-muted-foreground">加载中...</div>
      ) : localPath === null ? (
        <div className="p-2 space-y-1">
          {localRoots.map((entry) => (
            <button
              key={entry.path}
              type="button"
              onClick={() => onOpenLocalRoot(entry)}
              className="w-full text-left rounded border border-border px-2 py-1.5 text-xs hover:bg-accent"
            >
              <div className="truncate text-foreground">📁 {entry.name || entry.path}</div>
            </button>
          ))}
        </div>
      ) : localEntries.length === 0 ? (
        <div className="px-3 py-4 text-xs text-muted-foreground">空目录</div>
      ) : (
        <div className="p-2 space-y-1">
          {localEntries.map((entry) => (
            <button
              key={entry.path}
              type="button"
              onClick={() => onSelectLocal(entry)}
              onDoubleClick={() => {
                if (entry.isDir) {
                  onEnterLocalDirectory(entry);
                }
              }}
              className={cn(
                'w-full text-left rounded border px-2 py-1.5 text-xs',
                selectedLocal?.path === entry.path ? 'border-blue-500 bg-blue-500/10' : 'border-border hover:bg-accent',
              )}
            >
              <div className="truncate text-foreground">
                {entry.isDir ? '📁' : '📄'} {entry.name}
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  </div>
);
