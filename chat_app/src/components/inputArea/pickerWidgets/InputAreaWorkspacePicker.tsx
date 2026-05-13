import React from 'react';

import {
  DirectoryPickerActionButton,
  DirectoryPickerEntryList,
  DirectoryPickerPathDisplay,
} from '../../ui/DirectoryPickerShared';
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
            <DirectoryPickerPathDisplay
              currentPath={workspacePath}
              emptyText="请选择目录"
              label="当前路径: "
              className="truncate text-[11px]"
            />
            <div className="flex items-center gap-2">
              <DirectoryPickerActionButton
                onClick={() => onLoadWorkspaceDirectories(workspaceParent || null)}
                disabled={workspaceLoading || !workspaceParent}
                className="border text-[11px] hover:text-foreground"
              >
                返回上级
              </DirectoryPickerActionButton>
              <DirectoryPickerActionButton
                onClick={() => onLoadWorkspaceDirectories(workspacePath || normalizedWorkspaceRoot || null)}
                disabled={workspaceLoading}
                className="border text-[11px] hover:text-foreground"
              >
                刷新
              </DirectoryPickerActionButton>
              <DirectoryPickerActionButton
                onClick={() => onSelectWorkspaceRoot(workspacePath)}
                disabled={workspaceLoading || !workspacePath}
                className="border text-[11px] hover:text-foreground"
              >
                选择当前目录
              </DirectoryPickerActionButton>
              <DirectoryPickerActionButton
                onClick={() => onSelectWorkspaceRoot(null)}
                disabled={workspaceLoading && !normalizedWorkspaceRoot}
                className="border text-[11px] hover:text-foreground"
              >
                清空
              </DirectoryPickerActionButton>
            </div>
          </div>
          <DirectoryPickerEntryList
            loading={workspaceLoading}
            items={workspacePath ? workspaceEntries : workspaceRoots}
            emptyText="没有可用目录"
            onOpenEntry={(path) => onLoadWorkspaceDirectories(path)}
            showFolderIcon
            className="max-h-64 overflow-auto py-1"
            loadingClassName="px-3 py-2 text-xs"
            emptyClassName="px-3 py-2 text-xs"
            itemClassName="px-3 py-1.5 text-sm"
          />
        </div>
      )}
    </div>
  );
};
