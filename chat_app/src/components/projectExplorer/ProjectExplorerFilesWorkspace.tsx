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
  canRunFile: (entry: FsEntry) => boolean;
  onCreateDirectory: (path: string) => Promise<void> | void;
  onCreateFile: (path: string) => Promise<void> | void;
  onRunFile: (entry: FsEntry) => Promise<void> | void;
  onDownloadSelected: (entry: FsEntry) => Promise<void> | void;
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
  canRunFile,
  onCreateDirectory,
  onCreateFile,
  onRunFile,
  onDownloadSelected,
  onDeleteSelected,
}) => {
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
        canRunFile={canRunFile}
        onCreateDirectory={(path) => {
          setContextMenu(null);
          void onCreateDirectory(path);
        }}
        onCreateFile={(path) => {
          setContextMenu(null);
          void onCreateFile(path);
        }}
        onRunFile={(entry) => {
          setContextMenu(null);
          void onRunFile(entry);
        }}
        onDownload={(entry) => {
          setContextMenu(null);
          void onDownloadSelected(entry);
        }}
        onDelete={(entry) => {
          setContextMenu(null);
          void onDeleteSelected(entry);
        }}
      />
    </div>
  );
};

export default ProjectExplorerFilesWorkspace;
