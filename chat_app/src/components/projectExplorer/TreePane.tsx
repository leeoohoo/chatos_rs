import React, { useMemo } from 'react';

import type { FsEntry, Project, ProjectChangeSummary, ProjectSearchHit } from '../../types';
import { ProjectTreeEntries } from './treePane/ProjectTreeEntries';
import { ProjectTreeHeader } from './treePane/ProjectTreeHeader';
import type { ChangeKind } from './utils';

interface ProjectTreePaneProps {
  project: Project;
  treeWidth: number;
  treeScrollRef: React.MutableRefObject<HTMLDivElement | null>;
  entriesMap: Record<string, FsEntry[]>;
  expandedPaths: Set<string>;
  loadingPaths: Set<string>;
  selectedPath: string | null;
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
  searchLoading: boolean;
  searchError: string | null;
  searchTruncated: boolean;
  activeSearchHitId: string | null;
  activeSearchHitIndex: number;
  totalSearchHits: number;
  canOpenPreviousSearchHit: boolean;
  canOpenNextSearchHit: boolean;
  aggregatedChangeKindByPath: Map<string, ChangeKind>;
  normalizePath: (value: string) => string;
  toExpandedKey: (path: string) => string;
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
  onOpenSearchHit: (hit: ProjectSearchHit) => void;
  onSelectDeletedPath: (path: string) => void;
  onSelectMarkedPath: (path: string) => void;
  onToggleDir: (entry: FsEntry) => void;
  onOpenFile: (entry: FsEntry) => void;
  onDragStart: (event: React.DragEvent, entry: FsEntry) => void;
  onDragEnd: () => void;
  onSetDropTargetDirPath: React.Dispatch<React.SetStateAction<string | null>>;
  onSetDraggingEntryPath: React.Dispatch<React.SetStateAction<string | null>>;
  onMoveEntryByDrop: (sourcePath: string, targetDirPath: string) => void;
  onScheduleDragExpand: (path: string) => void;
  onCancelDragExpandIfMatches: (path: string) => void;
  onClearDragExpandTimer: () => void;
  onStartDragAutoScroll: (velocity: number) => void;
  onClearDragAutoScroll: () => void;
}

export const ProjectTreePane: React.FC<ProjectTreePaneProps> = ({
  project,
  treeWidth,
  treeScrollRef,
  entriesMap,
  expandedPaths,
  loadingPaths,
  selectedPath,
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
  searchLoading,
  searchError,
  searchTruncated,
  activeSearchHitId,
  activeSearchHitIndex,
  totalSearchHits,
  canOpenPreviousSearchHit,
  canOpenNextSearchHit,
  aggregatedChangeKindByPath,
  normalizePath,
  toExpandedKey,
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
  onOpenSearchHit,
  onSelectDeletedPath,
  onSelectMarkedPath,
  onToggleDir,
  onOpenFile,
  onDragStart,
  onDragEnd,
  onSetDropTargetDirPath,
  onSetDraggingEntryPath,
  onMoveEntryByDrop,
  onScheduleDragExpand,
  onCancelDragExpandIfMatches,
  onClearDragExpandTimer,
  onStartDragAutoScroll,
  onClearDragAutoScroll,
}) => {
  const projectRootEntry = useMemo<FsEntry>(() => ({
    name: project.name || project.rootPath,
    path: project.rootPath,
    isDir: true,
    size: null,
    modifiedAt: null,
  }), [project.name, project.rootPath]);

  return (
    <div className="border-r border-border bg-card flex flex-col shrink-0" style={{ width: treeWidth }}>
      <ProjectTreeHeader
        project={project}
        projectRootEntry={projectRootEntry}
        selectedEntry={selectedEntry}
        draggingEntryPath={draggingEntryPath}
        dropTargetDirPath={dropTargetDirPath}
        actionLoading={actionLoading}
        actionReloadPath={actionReloadPath}
        showOnlyChanged={showOnlyChanged}
        changeSummary={changeSummary}
        loadingSummary={loadingSummary}
        summaryError={summaryError}
        actionMessage={actionMessage}
        actionError={actionError}
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
        normalizePath={normalizePath}
        canDropToDirectory={canDropToDirectory}
        onSelectProjectRoot={onSelectProjectRoot}
        onToggleShowOnlyChanged={onToggleShowOnlyChanged}
        onCreateDirectoryAtRoot={onCreateDirectoryAtRoot}
        onCreateFileAtRoot={onCreateFileAtRoot}
        onRefresh={onRefresh}
        onSearchQueryChange={onSearchQueryChange}
        onToggleSearchCaseSensitive={onToggleSearchCaseSensitive}
        onToggleSearchWholeWord={onToggleSearchWholeWord}
        onClearSearch={onClearSearch}
        onOpenPreviousSearchHit={onOpenPreviousSearchHit}
        onOpenNextSearchHit={onOpenNextSearchHit}
        onOpenContextMenu={onOpenContextMenu}
        onSetDropTargetDirPath={onSetDropTargetDirPath}
        onSetDraggingEntryPath={onSetDraggingEntryPath}
        onMoveEntryByDrop={onMoveEntryByDrop}
        onClearDragExpandTimer={onClearDragExpandTimer}
        onClearDragAutoScroll={onClearDragAutoScroll}
      />
      <ProjectTreeEntries
        project={project}
        treeScrollRef={treeScrollRef}
        entriesMap={entriesMap}
        expandedPaths={expandedPaths}
        loadingPaths={loadingPaths}
        selectedPath={selectedPath}
        draggingEntryPath={draggingEntryPath}
        dropTargetDirPath={dropTargetDirPath}
        showOnlyChanged={showOnlyChanged}
        changeSummary={changeSummary}
        searchQuery={searchQuery}
        searchCaseSensitive={searchCaseSensitive}
        searchWholeWord={searchWholeWord}
        searchResults={searchResults}
        activeSearchHitId={activeSearchHitId}
        aggregatedChangeKindByPath={aggregatedChangeKindByPath}
        normalizePath={normalizePath}
        toExpandedKey={toExpandedKey}
        canDropToDirectory={canDropToDirectory}
        onOpenSearchHit={onOpenSearchHit}
        onSelectDeletedPath={onSelectDeletedPath}
        onSelectMarkedPath={onSelectMarkedPath}
        onToggleDir={onToggleDir}
        onOpenFile={onOpenFile}
        onOpenContextMenu={onOpenContextMenu}
        onDragStart={onDragStart}
        onDragEnd={onDragEnd}
        onSetDropTargetDirPath={onSetDropTargetDirPath}
        onSetDraggingEntryPath={onSetDraggingEntryPath}
        onMoveEntryByDrop={onMoveEntryByDrop}
        onScheduleDragExpand={onScheduleDragExpand}
        onCancelDragExpandIfMatches={onCancelDragExpandIfMatches}
        onStartDragAutoScroll={onStartDragAutoScroll}
        onClearDragAutoScroll={onClearDragAutoScroll}
      />
    </div>
  );
};
