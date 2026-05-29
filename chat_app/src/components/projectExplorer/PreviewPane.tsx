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
  saveError,
  savingFile,
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
  onRequestDocumentSymbols,
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
  onSaveFile,
}) => {
  const { t } = useI18n();
  const [documentSymbolsExpanded, setDocumentSymbolsExpanded] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [draftContent, setDraftContent] = useState('');

  useEffect(() => {
    setDocumentSymbolsExpanded(false);
  }, [selectedFile?.path]);

  useEffect(() => {
    setIsEditing(false);
    setDraftContent(selectedFile?.isBinary ? '' : (selectedFile?.content || ''));
  }, [selectedFile?.content, selectedFile?.isBinary, selectedFile?.path]);

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
  const canEdit = Boolean(selectedFile && !selectedFile.isBinary && selectedFile.writable !== false);
  const hasUnsavedChanges = canEdit && selectedFile ? draftContent !== selectedFile.content : false;

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <ProjectPreviewHeader
        selectedFile={selectedFile}
        selectedPath={selectedPath}
        isEditing={isEditing}
        canEdit={canEdit}
        hasUnsavedChanges={hasUnsavedChanges}
        savingFile={savingFile}
        onStartEditing={() => {
          if (!canEdit || !selectedFile) {
            return;
          }
          setDraftContent(selectedFile.content);
          setIsEditing(true);
        }}
        onCancelEditing={() => {
          setDraftContent(selectedFile?.content || '');
          setIsEditing(false);
        }}
        onSaveEditing={() => {
          if (!selectedFile || !canEdit) {
            return;
          }
          void (async () => {
            const ok = await onSaveFile(selectedFile.path, draftContent);
            if (ok) {
              setIsEditing(false);
            }
          })();
        }}
      />

      <div className="flex flex-1 flex-col overflow-hidden">
        {selectedFile && !selectedFile.isBinary && !isEditing && (
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
              setDocumentSymbolsExpanded((value) => {
                const next = !value;
                if (next) {
                  onRequestDocumentSymbols();
                }
                return next;
              });
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
              saveError={saveError}
              savingFile={savingFile}
              isEditing={isEditing}
              draftContent={draftContent}
              targetLine={targetLine}
              targetLineRevision={targetLineRevision}
              searchQuery={searchQuery}
              searchCaseSensitive={searchCaseSensitive}
              searchWholeWord={searchWholeWord}
              searchResults={searchResults}
              activeSearchHitId={activeSearchHitId}
              onActivateSearchHit={onActivateSearchHit}
              onTokenSelection={onTokenSelection}
              onDraftContentChange={setDraftContent}
              onSaveDraft={async () => {
                if (!selectedFile || !canEdit) {
                  return false;
                }
                const ok = await onSaveFile(selectedFile.path, draftContent);
                if (ok) {
                  setIsEditing(false);
                }
                return ok;
              }}
            />
          )}
        </div>
      </div>
    </div>
  );
};
