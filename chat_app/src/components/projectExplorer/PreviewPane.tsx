import React, { useEffect, useMemo, useState } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { ProjectPreviewFileContent } from './previewPane/ProjectPreviewFileContent';
import { ProjectPreviewHeader } from './previewPane/ProjectPreviewHeader';
import { ProjectPreviewNavigation } from './previewPane/ProjectPreviewNavigation';
import type { ProjectPreviewPaneProps } from './previewPane/previewPaneTypes';

export const ProjectPreviewPane: React.FC<ProjectPreviewPaneProps> = ({
  selectedFile,
  selectedPath,
  selectedEntry,
  loadingFile,
  error,
  searchQuery,
  searchCaseSensitive,
  searchWholeWord,
  searchResults,
  activeSearchHitId,
  activeSearchHitIndex,
  totalSearchHits,
  canOpenPreviousSearchHit,
  canOpenNextSearchHit,
  targetLine,
  targetLineRevision,
  navCapabilities,
  navCapabilitiesError,
  selectedToken,
  navResult,
  navRequestKind,
  navLoading,
  navError,
  activeNavLocationId,
  canGoBackFromNav,
  documentSymbols,
  documentSymbolsLoading,
  documentSymbolsError,
  onTokenSelection,
  onClearTokenSelection,
  onRequestDefinition,
  onRequestReferences,
  onGoBackFromNav,
  onSearchInProject,
  onOpenPreviousSearchHit,
  onOpenNextSearchHit,
  onActivateSearchHit,
  onOpenNavLocation,
  onOpenDocumentSymbol,
}) => {
  const { t } = useI18n();
  const [documentSymbolsExpanded, setDocumentSymbolsExpanded] = useState(false);

  useEffect(() => {
    setDocumentSymbolsExpanded(false);
  }, [selectedFile?.path]);

  const displayedToken = selectedToken || navResult?.token || null;
  const activeSearchQuery = searchQuery.trim();
  const activeSearchPositionLabel = totalSearchHits > 0
    ? `${activeSearchHitIndex >= 0 ? activeSearchHitIndex + 1 : 0} / ${totalSearchHits}`
    : null;
  const canNavigateToDefinition = Boolean(
    navCapabilities?.supportsDefinition || navCapabilities?.fallbackAvailable,
  );
  const canNavigateToReferences = Boolean(
    navCapabilities?.supportsReferences || navCapabilities?.fallbackAvailable,
  );
  const navResultLabel = useMemo(() => {
    if (!navResult || !navRequestKind) return null;
    if (navRequestKind === 'definition') return t('projectExplorer.preview.nav.definition');
    if (navRequestKind === 'references') return t('projectExplorer.preview.nav.references');
    return t('projectExplorer.preview.nav.default');
  }, [navRequestKind, navResult, t]);
  const documentSymbolCount = documentSymbols?.symbols?.length || 0;

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <ProjectPreviewHeader
        selectedFile={selectedFile}
        selectedPath={selectedPath}
      />

      <div className="flex flex-1 flex-col overflow-hidden">
        {selectedFile && !selectedFile.isBinary && (
          <ProjectPreviewNavigation
            displayedToken={displayedToken}
            activeSearchQuery={activeSearchQuery}
            activeSearchPositionLabel={activeSearchPositionLabel}
            totalSearchHits={totalSearchHits}
            canOpenPreviousSearchHit={canOpenPreviousSearchHit}
            canOpenNextSearchHit={canOpenNextSearchHit}
            canNavigateToDefinition={canNavigateToDefinition}
            canNavigateToReferences={canNavigateToReferences}
            selectedToken={selectedToken}
            navLoading={navLoading}
            navRequestKind={navRequestKind}
            navResult={navResult}
            navResultLabel={navResultLabel}
            navCapabilitiesError={navCapabilitiesError}
            navError={navError}
            activeNavLocationId={activeNavLocationId}
            canGoBackFromNav={canGoBackFromNav}
            documentSymbolsExpanded={documentSymbolsExpanded}
            documentSymbolsLoading={documentSymbolsLoading}
            documentSymbolsError={documentSymbolsError}
            documentSymbolCount={documentSymbolCount}
            documentSymbols={documentSymbols}
            targetLine={targetLine}
            onToggleDocumentSymbols={() => {
              setDocumentSymbolsExpanded((value) => !value);
            }}
            onOpenPreviousSearchHit={onOpenPreviousSearchHit}
            onOpenNextSearchHit={onOpenNextSearchHit}
            onRequestDefinition={onRequestDefinition}
            onRequestReferences={onRequestReferences}
            onGoBackFromNav={onGoBackFromNav}
            onSearchInProject={onSearchInProject}
            onClearTokenSelection={onClearTokenSelection}
            onOpenNavLocation={onOpenNavLocation}
            onOpenDocumentSymbol={onOpenDocumentSymbol}
          />
        )}

        <div className="min-h-0 flex-1 overflow-hidden">
          {error ? (
            <div className="p-4 text-sm text-destructive">{error}</div>
          ) : (
            <ProjectPreviewFileContent
              selectedFile={selectedFile}
              selectedPath={selectedPath}
              selectedEntry={selectedEntry}
              loadingFile={loadingFile}
              targetLine={targetLine}
              targetLineRevision={targetLineRevision}
              searchQuery={searchQuery}
              searchCaseSensitive={searchCaseSensitive}
              searchWholeWord={searchWholeWord}
              searchResults={searchResults}
              activeSearchHitId={activeSearchHitId}
              onActivateSearchHit={onActivateSearchHit}
              onTokenSelection={onTokenSelection}
            />
          )}
        </div>
      </div>
    </div>
  );
};
