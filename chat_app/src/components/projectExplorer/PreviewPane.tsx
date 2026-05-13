import React, { useEffect, useMemo, useState } from 'react';

import { DiffPanel } from './ChangeLogPanels';
import { ProjectPreviewFileContent } from './previewPane/ProjectPreviewFileContent';
import { ProjectPreviewHeader } from './previewPane/ProjectPreviewHeader';
import { ProjectPreviewNavigation } from './previewPane/ProjectPreviewNavigation';
import { ProjectRunnerMemberPickerDialog } from './previewPane/ProjectRunnerMemberPickerDialog';
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
  selectedLog,
  runStatus,
  runCatalogLoading,
  projectMembers,
  projectMembersLoading,
  runnerScriptExists,
  runnerScriptChecking,
  runnerScriptPath,
  runnerStartCommand,
  runnerStopCommand,
  runnerRestartCommand,
  starting,
  stopping,
  restarting,
  runnerMessage,
  runnerError,
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
  onRunnerStart,
  onRunnerStop,
  onRunnerRestart,
  onRefreshRunnerState,
  onGenerateRunnerScriptForContact,
}) => {
  const [memberPickerOpen, setMemberPickerOpen] = useState(false);
  const [memberPickerSelectedId, setMemberPickerSelectedId] = useState<string | null>(null);
  const [generating, setGenerating] = useState(false);
  const [generationError, setGenerationError] = useState<string | null>(null);
  const [generationMessage, setGenerationMessage] = useState<string | null>(null);
  const [documentSymbolsExpanded, setDocumentSymbolsExpanded] = useState(false);

  useEffect(() => {
    setDocumentSymbolsExpanded(false);
  }, [selectedFile?.path]);

  const selectedMember = useMemo(
    () => projectMembers.find((member) => member.contactId === memberPickerSelectedId) || null,
    [memberPickerSelectedId, projectMembers],
  );

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
    if (navRequestKind === 'definition') return '定义结果';
    if (navRequestKind === 'references') return '引用结果';
    return '导航结果';
  }, [navRequestKind, navResult]);
  const documentSymbolCount = documentSymbols?.symbols?.length || 0;
  const mergedError = runnerError || generationError;
  const mergedMessage = !mergedError ? (generationMessage || runnerMessage) : null;

  const handleGenerateForMember = async (
    member: typeof projectMembers[number],
  ): Promise<boolean> => {
    setGenerating(true);
    setGenerationError(null);
    setGenerationMessage(null);
    try {
      await onGenerateRunnerScriptForContact(member);
      setGenerationMessage(`已向 ${member.name || member.contactId} 发送脚本生成任务`);
      onRefreshRunnerState();
      return true;
    } catch (generation) {
      setGenerationError(generation instanceof Error ? generation.message : '发送脚本生成任务失败');
      return false;
    } finally {
      setGenerating(false);
    }
  };

  const handleGenerateClick = () => {
    setGenerationError(null);
    setGenerationMessage(null);
    if (projectMembersLoading) {
      return;
    }
    if (projectMembers.length === 0) {
      setGenerationError('当前项目还没有团队成员，请先添加联系人');
      return;
    }
    if (projectMembers.length === 1) {
      void handleGenerateForMember(projectMembers[0]);
      return;
    }
    setMemberPickerSelectedId(projectMembers[0]?.contactId || null);
    setMemberPickerOpen(true);
  };

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <ProjectPreviewHeader
        selectedFile={selectedFile}
        selectedPath={selectedPath}
        runnerScriptExists={runnerScriptExists}
        generating={generating}
        projectMembersLoading={projectMembersLoading}
        runnerScriptChecking={runnerScriptChecking}
        runStatus={runStatus}
        runCatalogLoading={runCatalogLoading}
        starting={starting}
        stopping={stopping}
        restarting={restarting}
        runnerStartCommand={runnerStartCommand}
        runnerStopCommand={runnerStopCommand}
        runnerRestartCommand={runnerRestartCommand}
        onGenerateClick={handleGenerateClick}
        onRunnerStart={onRunnerStart}
        onRunnerStop={onRunnerStop}
        onRunnerRestart={onRunnerRestart}
        onRefreshRunnerState={onRefreshRunnerState}
      />

      {(mergedMessage || mergedError) && (
        <div className="border-b border-border/70 bg-card px-4 py-1.5">
          <div className={mergedError ? 'text-[11px] text-destructive' : 'text-[11px] text-emerald-600'}>
            {mergedError || mergedMessage}
          </div>
        </div>
      )}

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

        <DiffPanel selectedLog={selectedLog} />

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

      {memberPickerOpen && (
        <ProjectRunnerMemberPickerDialog
          projectMembers={projectMembers}
          selectedMemberId={memberPickerSelectedId}
          generationError={generationError}
          generating={generating}
          runnerScriptPath={runnerScriptPath}
          onSelectMember={setMemberPickerSelectedId}
          onClose={() => {
            if (generating) return;
            setMemberPickerOpen(false);
          }}
          onConfirm={() => {
            if (!selectedMember) return;
            void handleGenerateForMember(selectedMember).then((success) => {
              if (success) {
                setMemberPickerOpen(false);
              }
            });
          }}
        />
      )}
    </div>
  );
};
