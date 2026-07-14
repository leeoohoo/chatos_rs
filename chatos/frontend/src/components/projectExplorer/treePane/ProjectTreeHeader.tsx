// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import { getUserVisiblePath } from '../../../lib/domain/filesystem';
import type { FsEntry, Project, ProjectSearchHit } from '../../../types';
import { cn } from '../../../lib/utils';
import {
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
  const { t } = useI18n();
  const visibleRootPath = getUserVisiblePath(project.displayRootPath || project.rootPath);
  const visibleSelectedPath = selectedEntry
    ? getUserVisiblePath(selectedEntry.path, project.rootPath)
    : t('projectExplorer.tree.noSelection');
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
      <div className="text-xs text-muted-foreground">{t('projectExplorer.tree.title')}</div>
      <div className="truncate text-sm font-medium text-foreground" title={visibleRootPath}>
        {project.name}
      </div>
      <div className="truncate text-[11px] text-muted-foreground" title={visibleRootPath}>
        {visibleRootPath}
      </div>
      <div className="truncate text-[11px] text-muted-foreground" title={visibleSelectedPath}>
        {t('projectExplorer.tree.currentSelection', {
          path: visibleSelectedPath,
        })}
      </div>
      <button
        type="button"
        onClick={(event) => {
          event.stopPropagation();
          onSelectProjectRoot();
        }}
        className="text-left text-[11px] text-blue-600 hover:underline"
      >
        {t('projectExplorer.tree.selectRoot')}
      </button>
      <ProjectTreeHeaderActions
        actionLoading={actionLoading}
        actionReloadPath={actionReloadPath}
        onCreateDirectoryAtRoot={onCreateDirectoryAtRoot}
        onCreateFileAtRoot={onCreateFileAtRoot}
        onRefresh={onRefresh}
      />
      <div className="text-[11px] text-muted-foreground">
        {project.sourceType?.trim().toLowerCase() === 'cloud'
          ? t('projectExplorer.tree.harnessHint')
          : t('projectExplorer.tree.gitHint')}
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
        actionMessage={actionMessage}
        actionError={actionError}
      />
    </div>
  );
};
