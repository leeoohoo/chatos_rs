// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useMemo } from 'react';

import type { FsEntry, Project, ProjectSearchHit } from '../../types';
import { ProjectTreeEntries } from './treePane/ProjectTreeEntries';
import { ProjectTreeHeader } from './treePane/ProjectTreeHeader';

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
  normalizePath: (value: string) => string;
  toExpandedKey: (path: string) => string;
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
  onOpenSearchHit: (hit: ProjectSearchHit) => void;
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
  normalizePath,
  toExpandedKey,
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
  onOpenSearchHit,
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
        searchQuery={searchQuery}
        searchCaseSensitive={searchCaseSensitive}
        searchWholeWord={searchWholeWord}
        searchResults={searchResults}
        activeSearchHitId={activeSearchHitId}
        normalizePath={normalizePath}
        toExpandedKey={toExpandedKey}
        canDropToDirectory={canDropToDirectory}
        onOpenSearchHit={onOpenSearchHit}
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
