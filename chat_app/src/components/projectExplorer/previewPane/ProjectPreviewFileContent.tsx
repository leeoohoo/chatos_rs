// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import type { FsEntry, FsReadResult, ProjectSearchHit } from '../../../types';
import {
  ProjectPreviewBinaryDownload,
  ProjectPreviewDeletedPathState,
  ProjectPreviewEmptyState,
  ProjectPreviewImageContent,
  ProjectPreviewLoadingState,
} from './ProjectPreviewFileStates';
import { ProjectPreviewTextContent } from './ProjectPreviewTextContent';
import type { PreviewTokenSelection } from './previewPaneTypes';

interface ProjectPreviewFileContentProps {
  selectedFile: FsReadResult | null;
  selectedPath: string | null;
  selectedEntry: FsEntry | null;
  loadingFile: boolean;
  saveError: string | null;
  savingFile: boolean;
  isEditing: boolean;
  draftContent: string;
  targetLine: number | null;
  targetLineRevision: number;
  searchQuery: string;
  searchCaseSensitive: boolean;
  searchWholeWord: boolean;
  searchResults: ProjectSearchHit[];
  activeSearchHitId: string | null;
  onActivateSearchHit: (hit: ProjectSearchHit) => void;
  onTokenSelection: (selection: PreviewTokenSelection | null) => void;
  onDraftContentChange: (value: string) => void;
  onSaveDraft: () => Promise<boolean>;
}

export const ProjectPreviewFileContent: React.FC<ProjectPreviewFileContentProps> = ({
  selectedFile,
  selectedPath,
  selectedEntry,
  loadingFile,
  saveError,
  savingFile,
  isEditing,
  draftContent,
  targetLine,
  targetLineRevision,
  searchQuery,
  searchCaseSensitive,
  searchWholeWord,
  searchResults,
  activeSearchHitId,
  onActivateSearchHit,
  onTokenSelection,
  onDraftContentChange,
  onSaveDraft,
}) => {
  if (loadingFile) {
    return <ProjectPreviewLoadingState />;
  }
  if (!selectedFile) {
    return selectedPath && !selectedEntry
      ? <ProjectPreviewDeletedPathState />
      : <ProjectPreviewEmptyState />;
  }

  const isImage = selectedFile.contentType.startsWith('image/');
  if (isImage && selectedFile.isBinary) {
    return <ProjectPreviewImageContent selectedFile={selectedFile} />;
  }
  if (selectedFile.isBinary) {
    return <ProjectPreviewBinaryDownload selectedFile={selectedFile} />;
  }

  return (
    <ProjectPreviewTextContent
      selectedFile={selectedFile}
      selectedPath={selectedPath}
      isEditing={isEditing}
      draftContent={draftContent}
      saveError={saveError}
      savingFile={savingFile}
      targetLine={targetLine}
      targetLineRevision={targetLineRevision}
      searchQuery={searchQuery}
      searchCaseSensitive={searchCaseSensitive}
      searchWholeWord={searchWholeWord}
      searchResults={searchResults}
      activeSearchHitId={activeSearchHitId}
      onActivateSearchHit={onActivateSearchHit}
      onTokenSelection={onTokenSelection}
      onDraftContentChange={onDraftContentChange}
      onSaveDraft={onSaveDraft}
    />
  );
};
