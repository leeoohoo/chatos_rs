// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useState } from 'react';

import type { FsEntry, ProjectSearchHit } from '../../../types';
import { buildProjectSearchHitId } from '../../../lib/domain/projectSearch';

interface UseProjectSearchNavigationOptions {
  searchResults: ProjectSearchHit[];
}

export const useProjectSearchNavigation = ({
  searchResults,
}: UseProjectSearchNavigationOptions) => {
  const [activeSearchHitId, setActiveSearchHitId] = useState<string | null>(null);
  const [previewTargetLine, setPreviewTargetLine] = useState<number | null>(null);
  const [previewTargetLineRevision, setPreviewTargetLineRevision] = useState(0);

  const requestPreviewTargetLine = useCallback((line: number | null) => {
    setPreviewTargetLine(line);
    if (line !== null && line > 0) {
      setPreviewTargetLineRevision((revision) => revision + 1);
    }
  }, []);

  const clearSearchNavigation = useCallback(() => {
    setActiveSearchHitId(null);
    setPreviewTargetLine(null);
  }, []);

  const totalSearchHits = searchResults.length;
  const activeSearchHitIndex = activeSearchHitId
    ? searchResults.findIndex((hit) => buildProjectSearchHitId(hit) === activeSearchHitId)
    : -1;
  const canOpenPreviousSearchHit = totalSearchHits > 0 && activeSearchHitIndex !== 0;
  const canOpenNextSearchHit = totalSearchHits > 0 && activeSearchHitIndex !== totalSearchHits - 1;

  const handleOpenSearchHit = useCallback(async (
    hit: ProjectSearchHit,
    openFile: (entry: FsEntry) => Promise<void>,
  ) => {
    await openFile({
      name: hit.relativePath.split('/').filter(Boolean).pop() || hit.path.split(/[\\/]/).pop() || hit.path,
      path: hit.path,
      isDir: false,
      size: null,
      modifiedAt: null,
    });
    setActiveSearchHitId(buildProjectSearchHitId(hit));
    requestPreviewTargetLine(hit.line);
  }, [requestPreviewTargetLine]);

  const activateSearchHit = useCallback((hit: ProjectSearchHit) => {
    setActiveSearchHitId(buildProjectSearchHitId(hit));
    requestPreviewTargetLine(hit.line);
  }, [requestPreviewTargetLine]);

  const openRelativeSearchHit = useCallback(async (
    direction: -1 | 1,
    openFile: (entry: FsEntry) => Promise<void>,
  ) => {
    if (searchResults.length === 0) {
      return;
    }

    let nextIndex = direction > 0 ? 0 : searchResults.length - 1;
    if (activeSearchHitIndex >= 0) {
      nextIndex = Math.min(
        searchResults.length - 1,
        Math.max(0, activeSearchHitIndex + direction),
      );
      if (nextIndex === activeSearchHitIndex) {
        return;
      }
    }

    const targetHit = searchResults[nextIndex];
    if (!targetHit) {
      return;
    }
    await handleOpenSearchHit(targetHit, openFile);
  }, [activeSearchHitIndex, handleOpenSearchHit, searchResults]);

  const openPreviousSearchHit = useCallback(async (
    openFile: (entry: FsEntry) => Promise<void>,
  ) => {
    await openRelativeSearchHit(-1, openFile);
  }, [openRelativeSearchHit]);

  const openNextSearchHit = useCallback(async (
    openFile: (entry: FsEntry) => Promise<void>,
  ) => {
    await openRelativeSearchHit(1, openFile);
  }, [openRelativeSearchHit]);

  return {
    activeSearchHitId,
    activeSearchHitIndex,
    totalSearchHits,
    previewTargetLine,
    previewTargetLineRevision,
    setPreviewTargetLine: requestPreviewTargetLine,
    canOpenPreviousSearchHit,
    canOpenNextSearchHit,
    clearSearchNavigation,
    activateSearchHit,
    handleOpenSearchHit,
    openPreviousSearchHit,
    openNextSearchHit,
  };
};
