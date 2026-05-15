import React from 'react';

import type { FsEntry, Project, ProjectChangeSummary, ProjectSearchHit } from '../../../types';
import { cn } from '../../../lib/utils';
import {
  ProjectTreeChangeCounters,
  ProjectTreeHeaderActions,
  ProjectTreeHeaderMessages,
} from './ProjectTreeHeaderActions';
import { ProjectTreeSearchControls } from './ProjectTreeSearchControls';
import { useProjectTreeRootDropHandlers } from './useProjectTreeRootDropHandlers';

interface ProjectTreeHeaderProps {
  project: Project;
  projectRootEntry: FsEntry;
  selectedEntry: FsEntry | null;
  draggingEntryPath: string | null;
  dropTargetDirPath: string | null;
  actionLoading: boolean;
  actionReloadPath: string | null;
  showOnlyChanged: boolean;
  changeSummary: ProjectChangeSummary;
  loadingSummary: boolean;
  summaryError: string | null;
  actionMessage: string | null;
  actionError: string | null;
  searchQuery: string;
  searchCaseSensitive: boolean;
  searchWholeWord: boolean;
  searchResults: ProjectSearchHit[];
  totalSearchHits: number;
  canOpenPreviousSearchHit: boolean;
  canOpenNextSearchHit: boolean;
  activeSearchHitIndex: number;
  searchLoading: boolean;
  searchError: string | null;
  searchTruncated: boolean;
  normalizePath: (value: string) => string;
  canDropToDirectory: (sourcePath: string, targetDirPath: string) => boolean;
  onSelectProjectRoot: () => void;
  onToggleShowOnlyChanged: () => void;
  onCreateDirectoryAtRoot: () => void;
  onCreateFileAtRoot: () => void;
  onRefresh: () => void;
  onSearchQueryChange: (value: string) => void;
  onToggleSearchCaseSensitive: () => void;
  onToggleSearchWholeWord: () => void;
  onClearSearch: () => void;
  onOpenPreviousSearchHit: () => void;
  onOpenNextSearchHit: () => void;
  onOpenContextMenu: (event: React.MouseEvent, entry: FsEntry) => void;
  onSetDropTargetDirPath: React.Dispatch<React.SetStateAction<string | null>>;
  onSetDraggingEntryPath: React.Dispatch<React.SetStateAction<string | null>>;
  onMoveEntryByDrop: (sourcePath: string, targetDirPath: string) => void;
  onClearDragExpandTimer: () => void;
  onClearDragAutoScroll: () => void;
}

export const ProjectTreeHeader: React.FC<ProjectTreeHeaderProps> = ({
  project,
  projectRootEntry,
  selectedEntry,
  draggingEntryPath,
  dropTargetDirPath,
  actionLoading,
  actionReloadPath,
  showOnlyChanged,
  changeSummary,
  loadingSummary,
  summaryError,
  actionMessage,
  actionError,
  searchQuery,
  searchCaseSensitive,
  searchWholeWord,
  searchResults,
  totalSearchHits,
  canOpenPreviousSearchHit,
  canOpenNextSearchHit,
  activeSearchHitIndex,
  searchLoading,
  searchError,
  searchTruncated,
  normalizePath,
  canDropToDirectory,
  onSelectProjectRoot,
  onToggleShowOnlyChanged,
  onCreateDirectoryAtRoot,
  onCreateFileAtRoot,
  onRefresh,
  onSearchQueryChange,
  onToggleSearchCaseSensitive,
  onToggleSearchWholeWord,
  onClearSearch,
  onOpenPreviousSearchHit,
  onOpenNextSearchHit,
  onOpenContextMenu,
  onSetDropTargetDirPath,
  onSetDraggingEntryPath,
  onMoveEntryByDrop,
  onClearDragExpandTimer,
  onClearDragAutoScroll,
}) => {
  const {
    handleRootDragEnter,
    handleRootDragLeave,
    handleRootDragOver,
    handleRootDrop,
  } = useProjectTreeRootDropHandlers({
    draggingEntryPath,
    projectRootPath: project.rootPath,
    normalizePath,
    canDropToDirectory,
    onSetDropTargetDirPath,
    onSetDraggingEntryPath,
    onMoveEntryByDrop,
    onClearDragExpandTimer,
    onClearDragAutoScroll,
  });

  return (
    <div
      className={cn(
        'space-y-2 border-b border-border px-3 py-2',
        dropTargetDirPath && normalizePath(dropTargetDirPath) === normalizePath(project.rootPath)
          ? 'bg-blue-500/10 ring-1 ring-blue-500'
          : '',
      )}
      onContextMenu={(event) => {
        onOpenContextMenu(event, projectRootEntry);
      }}
      onDragOver={handleRootDragOver}
      onDragEnter={handleRootDragEnter}
      onDragLeave={handleRootDragLeave}
      onDrop={handleRootDrop}
    >
      <div className="text-xs text-muted-foreground">项目目录</div>
      <div className="truncate text-sm font-medium text-foreground" title={project.rootPath}>
        {project.name}
      </div>
      <div className="truncate text-[11px] text-muted-foreground" title={project.rootPath}>
        {project.rootPath}
      </div>
      <div className="truncate text-[11px] text-muted-foreground" title={selectedEntry?.path || ''}>
        当前选择：{selectedEntry ? selectedEntry.path : '未选择'}
      </div>
      <button
        type="button"
        onClick={(event) => {
          event.stopPropagation();
          onSelectProjectRoot();
        }}
        className="text-left text-[11px] text-blue-600 hover:underline"
      >
        选中项目根目录
      </button>
      <ProjectTreeChangeCounters changeSummary={changeSummary} />
      <ProjectTreeHeaderActions
        actionLoading={actionLoading}
        actionReloadPath={actionReloadPath}
        showOnlyChanged={showOnlyChanged}
        onCreateDirectoryAtRoot={onCreateDirectoryAtRoot}
        onCreateFileAtRoot={onCreateFileAtRoot}
        onRefresh={onRefresh}
        onToggleShowOnlyChanged={onToggleShowOnlyChanged}
      />
      <div className="text-[11px] text-muted-foreground">
        这里只负责查看目录和变更；Stage、Commit、Push 请使用右上角 Git 面板
      </div>
      <ProjectTreeSearchControls
        searchQuery={searchQuery}
        searchCaseSensitive={searchCaseSensitive}
        searchWholeWord={searchWholeWord}
        searchResults={searchResults}
        totalSearchHits={totalSearchHits}
        canOpenPreviousSearchHit={canOpenPreviousSearchHit}
        canOpenNextSearchHit={canOpenNextSearchHit}
        activeSearchHitIndex={activeSearchHitIndex}
        searchLoading={searchLoading}
        searchError={searchError}
        searchTruncated={searchTruncated}
        onSearchQueryChange={onSearchQueryChange}
        onToggleSearchCaseSensitive={onToggleSearchCaseSensitive}
        onToggleSearchWholeWord={onToggleSearchWholeWord}
        onClearSearch={onClearSearch}
        onOpenPreviousSearchHit={onOpenPreviousSearchHit}
        onOpenNextSearchHit={onOpenNextSearchHit}
      />
      <ProjectTreeHeaderMessages
        loadingSummary={loadingSummary}
        summaryError={summaryError}
        actionMessage={actionMessage}
        actionError={actionError}
      />
    </div>
  );
};
