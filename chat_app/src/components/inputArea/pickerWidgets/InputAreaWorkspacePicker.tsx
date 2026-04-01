import React from 'react';

import { cn } from '../../../lib/utils';
import type { FsEntry } from '../../../types';

interface InputAreaWorkspacePickerProps {
  showWorkspaceRootPicker: boolean;
  workspacePickerRef: React.RefObject<HTMLDivElement>;
  disabled: boolean;
  isStreaming: boolean;
  isStopping: boolean;
  onToggleWorkspacePicker: () => void;
  normalizedWorkspaceRoot: string | null;
  workspaceRootDisplayName: string;
  workspacePickerOpen: boolean;
  workspacePath: string | null;
  workspaceParent: string | null;
  workspaceLoading: boolean;
  workspaceEntries: FsEntry[];
  workspaceRoots: FsEntry[];
  onLoadWorkspaceDirectories: (nextPath?: string | null) => void;
  onSelectWorkspaceRoot: (path: string | null) => void;
}

export const InputAreaWorkspacePicker: React.FC<InputAreaWorkspacePickerProps> = ({
  showWorkspaceRootPicker,
  workspacePickerRef,
  disabled,
  isStreaming,
  isStopping,
  onToggleWorkspacePicker,
  normalizedWorkspaceRoot,
  workspaceRootDisplayName,
  workspacePickerOpen,
  workspacePath,
  workspaceParent,
  workspaceLoading,
  workspaceEntries,
  workspaceRoots,
  onLoadWorkspaceDirectories,
  onSelectWorkspaceRoot,
}) => {
  if (!showWorkspaceRootPicker) {
    return null;
  }

  return (
    <div className="relative flex-shrink-0" ref={workspacePickerRef}>
      <button
        type="button"
        onClick={onToggleWorkspacePicker}
        disabled={disabled || isStreaming || isStopping}
        className={cn(
          'px-2 py-1 rounded-md border text-xs transition-colors',
          'text-muted-foreground hover:text-foreground hover:bg-accent',
          (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed',
        )}
        title={normalizedWorkspaceRoot || '选择工作目录'}
      >
        {`工作目录: ${workspaceRootDisplayName}`}
        <span className="ml-1">▾</span>
      </button>
      {workspacePickerOpen && (
        <div className="absolute left-0 bottom-full mb-2 z-30 w-80 bg-popover text-popover-foreground border rounded-md shadow-lg">
          <div className="px-3 py-2 border-b space-y-2">
            <div className="text-[11px] text-muted-foreground truncate" title={workspacePath || '请选择目录'}>
              当前路径: {workspacePath || '请选择目录'}
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                className="px-2 py-1 rounded border text-[11px] text-muted-foreground hover:text-foreground hover:bg-accent disabled:opacity-50"
                onClick={() => onLoadWorkspaceDirectories(workspaceParent || null)}
                disabled={workspaceLoading || !workspaceParent}
              >
                返回上级
              </button>
              <button
                type="button"
                className="px-2 py-1 rounded border text-[11px] text-muted-foreground hover:text-foreground hover:bg-accent disabled:opacity-50"
                onClick={() => onLoadWorkspaceDirectories(workspacePath || normalizedWorkspaceRoot || null)}
                disabled={workspaceLoading}
              >
                刷新
              </button>
              <button
                type="button"
                className="px-2 py-1 rounded border text-[11px] text-muted-foreground hover:text-foreground hover:bg-accent disabled:opacity-50"
                onClick={() => onSelectWorkspaceRoot(workspacePath)}
                disabled={workspaceLoading || !workspacePath}
              >
                选择当前目录
              </button>
              <button
                type="button"
                className="px-2 py-1 rounded border text-[11px] text-muted-foreground hover:text-foreground hover:bg-accent disabled:opacity-50"
                onClick={() => onSelectWorkspaceRoot(null)}
                disabled={workspaceLoading && !normalizedWorkspaceRoot}
              >
                清空
              </button>
            </div>
          </div>
          <div className="max-h-64 overflow-auto py-1">
            {workspaceLoading ? (
              <div className="px-3 py-2 text-xs text-muted-foreground">加载中...</div>
            ) : (
              (() => {
                const items = workspacePath ? workspaceEntries : workspaceRoots;
                if (!items || items.length === 0) {
                  return <div className="px-3 py-2 text-xs text-muted-foreground">没有可用目录</div>;
                }
                return items.map((entry) => (
                  <button
                    key={entry.path}
                    type="button"
                    className="w-full px-3 py-1.5 text-left text-sm hover:bg-accent flex items-center gap-2"
                    onClick={() => onLoadWorkspaceDirectories(entry.path)}
                  >
                    <svg className="w-4 h-4 text-muted-foreground shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
                      <path strokeLinecap="round" strokeLinejoin="round" d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7z" />
                    </svg>
                    <span className="truncate">{entry.name}</span>
                  </button>
                ));
              })()
            )}
          </div>
        </div>
      )}
    </div>
  );
};
