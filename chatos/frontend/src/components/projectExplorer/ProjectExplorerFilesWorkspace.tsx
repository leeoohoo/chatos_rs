// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import type { FsEntry } from '../../types';
import { cn } from '../../lib/utils';
import {
  EntryContextMenu,
  MoveConflictModal,
  type MoveConflictState,
} from './Overlays';
import { ProjectPreviewPane } from './PreviewPane';
import { ProjectTreePane } from './TreePane';
import type { ExplorerContextMenuState } from './useProjectExplorerState';

interface ProjectExplorerFilesWorkspaceProps {
  treePaneProps: React.ComponentProps<typeof ProjectTreePane>;
  treeWidth: number;
  isResizing: boolean;
  resizeStartX: React.MutableRefObject<number>;
  resizeStartWidth: React.MutableRefObject<number>;
  setIsResizing: React.Dispatch<React.SetStateAction<boolean>>;
  previewPaneProps: React.ComponentProps<typeof ProjectPreviewPane>;
  moveConflict: MoveConflictState | null;
  actionLoading: boolean;
  setMoveConflict: React.Dispatch<React.SetStateAction<MoveConflictState | null>>;
  onMoveConflictCancel: () => void;
  onMoveConflictOverwrite: (moveConflict: MoveConflictState | null) => Promise<void> | void;
  onMoveConflictRename: (moveConflict: MoveConflictState | null) => Promise<void> | void;
  contextMenu: ExplorerContextMenuState | null;
  contextMenuStyle?: React.CSSProperties;
  isContextRootEntry: boolean;
  setContextMenu: React.Dispatch<React.SetStateAction<ExplorerContextMenuState | null>>;
  onCreateDirectory: (path: string) => Promise<void> | void;
  onCreateFile: (path: string) => Promise<void> | void;
  onDownloadSelected: (entry: FsEntry) => Promise<void> | void;
  onCopyFilePath: (entry: FsEntry) => Promise<boolean> | boolean;
  onCopyRelativeFilePath: (entry: FsEntry) => Promise<boolean> | boolean;
  onIgnoreFile: (entry: FsEntry) => Promise<boolean> | boolean;
  onIgnoreFolder: (entry: FsEntry) => Promise<boolean> | boolean;
  onIgnoreByExtension: (entry: FsEntry) => Promise<boolean> | boolean;
  onOpenPathInDefaultProgram: (entry: FsEntry) => Promise<boolean> | boolean;
  onRevealInFinder: (entry: FsEntry) => Promise<boolean> | boolean;
  onOpenInCode: (entry: FsEntry) => Promise<boolean> | boolean;
  onDeleteSelected: (entry: FsEntry) => Promise<void> | void;
}

export const ProjectExplorerFilesWorkspace: React.FC<ProjectExplorerFilesWorkspaceProps> = ({
  treePaneProps,
  treeWidth,
  isResizing,
  resizeStartX,
  resizeStartWidth,
  setIsResizing,
  previewPaneProps,
  moveConflict,
  actionLoading,
  setMoveConflict,
  onMoveConflictCancel,
  onMoveConflictOverwrite,
  onMoveConflictRename,
  contextMenu,
  contextMenuStyle,
  isContextRootEntry,
  setContextMenu,
  onCreateDirectory,
  onCreateFile,
  onDownloadSelected,
  onCopyFilePath,
  onCopyRelativeFilePath,
  onIgnoreFile,
  onIgnoreFolder,
  onIgnoreByExtension,
  onOpenPathInDefaultProgram,
  onRevealInFinder,
  onOpenInCode,
  onDeleteSelected,
}) => {
  const runMenuAction = (action: () => Promise<unknown> | unknown) => {
    setContextMenu(null);
    void action();
  };

  return (
    <div className="flex h-full overflow-hidden">
      <ProjectTreePane {...treePaneProps} />
      <div
        className={cn('w-1 cursor-col-resize bg-border/60 hover:bg-border', isResizing && 'bg-border')}
        onMouseDown={(event) => {
          resizeStartX.current = event.clientX;
          resizeStartWidth.current = treeWidth;
          setIsResizing(true);
        }}
      />
      <div className="flex-1 flex overflow-hidden">
        <ProjectPreviewPane {...previewPaneProps} />
      </div>
      <MoveConflictModal
        moveConflict={moveConflict}
        actionLoading={actionLoading}
        onCancel={onMoveConflictCancel}
        onRenameChange={(value) => {
          setMoveConflict((prev) => (prev ? { ...prev, renameTo: value } : prev));
        }}
        onOverwrite={() => {
          void onMoveConflictOverwrite(moveConflict);
        }}
        onRename={() => {
          void onMoveConflictRename(moveConflict);
        }}
      />
      <EntryContextMenu
        contextMenu={contextMenu}
        contextMenuStyle={contextMenuStyle}
        isContextRootEntry={isContextRootEntry}
        projectRootPath={treePaneProps.project.rootPath}
        onCreateDirectory={(path) => {
          runMenuAction(() => onCreateDirectory(path));
        }}
        onCreateFile={(path) => {
          runMenuAction(() => onCreateFile(path));
        }}
        onDownload={(entry) => {
          runMenuAction(() => onDownloadSelected(entry));
        }}
        onCopyFilePath={(entry) => {
          runMenuAction(() => onCopyFilePath(entry));
        }}
        onCopyRelativeFilePath={(entry) => {
          runMenuAction(() => onCopyRelativeFilePath(entry));
        }}
        onIgnoreFile={(entry) => {
          runMenuAction(() => onIgnoreFile(entry));
        }}
        onIgnoreFolder={(entry) => {
          runMenuAction(() => onIgnoreFolder(entry));
        }}
        onIgnoreByExtension={(entry) => {
          runMenuAction(() => onIgnoreByExtension(entry));
        }}
        onOpenPathInDefaultProgram={(entry) => {
          runMenuAction(() => onOpenPathInDefaultProgram(entry));
        }}
        onRevealInFinder={(entry) => {
          runMenuAction(() => onRevealInFinder(entry));
        }}
        onOpenInCode={(entry) => {
          runMenuAction(() => onOpenInCode(entry));
        }}
        onDelete={(entry) => {
          runMenuAction(() => onDeleteSelected(entry));
        }}
      />
    </div>
  );
};

export default ProjectExplorerFilesWorkspace;
