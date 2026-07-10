// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import { cn } from '../../../lib/utils';
import { CommitDialog, ComparePanel, DiffDialog } from './GitBranchDialogs';
import {
  BranchSection,
  GitActionRows,
  GitClientInfoBlock,
  GitSummaryBlock,
  NewBranchRow,
  StatusSection,
} from './GitBranchPanels';
import type { GitBranchButtonModel } from './useGitBranchButtonModel';

export const GitBranchTrigger: React.FC<{
  model: GitBranchButtonModel;
}> = ({ model }) => {
  const { branchLabel, changeCount, git, open, toggleOpen } = model;

  return (
    <button
      type="button"
      onClick={toggleOpen}
      className={cn(
        'inline-flex h-8 max-w-56 items-center gap-2 rounded-md border border-border bg-background px-3 text-xs text-foreground shadow-sm transition-colors hover:bg-accent',
        git.summary?.dirty && 'border-amber-400/70',
      )}
      title={branchLabel}
    >
      <span className="text-muted-foreground">Git</span>
      <span className="truncate font-medium">{branchLabel}</span>
      {git.summary?.isRepo && git.summary.ahead > 0 && (
        <span className="text-[11px] text-emerald-600">↑{git.summary.ahead}</span>
      )}
      {git.summary?.isRepo && git.summary.behind > 0 && (
        <span className="text-[11px] text-sky-600">↓{git.summary.behind}</span>
      )}
      {changeCount > 0 && (
        <span className="rounded bg-amber-500/15 px-1.5 py-0.5 text-[11px] text-amber-700">
          +{changeCount}
        </span>
      )}
      <span className="text-muted-foreground">{open ? '⌃' : '⌄'}</span>
    </button>
  );
};

export const GitBranchDropdown: React.FC<{
  model: GitBranchButtonModel;
}> = ({ model }) => {
  const { t } = useI18n();
  const {
    activeRepoRoot,
    allStatusFiles,
    branchLabel,
    changeCount,
    closePanel,
    createBranch,
    filteredBranches,
    gitAvailableRepositories,
    git,
    newBranchName,
    openCommitDialog,
    openDiffDialog,
    projectRoot,
    query,
    readOnly,
    setNewBranchName,
    setQuery,
    selectRepository,
  } = model;

  return (
    <div className="absolute right-0 top-10 z-50 flex max-h-[78vh] w-[min(720px,calc(100vw-2rem))] flex-col overflow-hidden rounded-lg border border-border bg-popover text-popover-foreground shadow-xl">
      <div className="border-b border-border p-3">
        <input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={t('git.searchPlaceholder')}
          className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm outline-none focus:border-primary"
          autoFocus
        />
      </div>

      <div className="flex-1 overflow-y-auto p-2">
        {git.error && (
          <div className="mb-2 rounded border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
            {git.error}
          </div>
        )}
        {git.actionMessage && (
          <div className="mb-2 rounded border border-emerald-500/30 bg-emerald-500/5 px-3 py-2 text-xs text-emerald-700">
            {git.actionMessage}
          </div>
        )}
        {!readOnly && (
          <GitClientInfoBlock
            clientInfo={git.clientInfo}
            loading={git.loadingClientInfo}
            onRefresh={git.refreshClientInfo}
          />
        )}

        {gitAvailableRepositories.length > 0 && (
          <div className="mb-2 rounded-md border border-border bg-background p-3 text-xs">
            <div className="mb-2 text-[11px] text-muted-foreground">{t('git.repositorySelect')}</div>
            <div className="space-y-2">
              {gitAvailableRepositories.map((repo) => {
                const selected = (activeRepoRoot || '') === repo.root;
                return (
                  <button
                    key={repo.root}
                    type="button"
                    onClick={() => { void selectRepository(repo.root); }}
                    className={cn(
                      'flex w-full items-start justify-between rounded border px-3 py-2 text-left transition-colors',
                      selected
                        ? 'border-primary bg-primary/5 text-foreground'
                        : 'border-border hover:bg-accent text-muted-foreground'
                    )}
                  >
                    <span className="min-w-0">
                      <span className="block truncate text-sm font-medium text-foreground">{repo.label}</span>
                      <span className="block truncate text-[11px] text-muted-foreground">
                        {repo.relativePath || '.'}
                      </span>
                    </span>
                    {selected && (
                      <span className="ml-3 shrink-0 text-[11px] text-primary">{t('git.current')}</span>
                    )}
                  </button>
                );
              })}
            </div>
          </div>
        )}

        {!git.summary?.isRepo ? (
          <div className="space-y-3 p-3 text-sm text-muted-foreground">
            <div>
              {gitAvailableRepositories.length > 0
                ? t('git.notRepoWithCandidates')
                : t('git.notRepo')}
            </div>
            <button
              type="button"
              onClick={() => { void git.refreshSummary({ force: true }); }}
              className="h-8 rounded border border-border px-3 text-xs hover:bg-accent"
            >
              {t('git.refresh')}
            </button>
          </div>
        ) : (
          <>
            <GitSummaryBlock
              branchLabel={branchLabel}
              changeCount={changeCount}
              summary={git.summary}
            />
            {!readOnly && (
              <>
                <GitActionRows
                  actionLoading={git.actionLoading}
                  onFetch={git.fetchRemote}
                  onPull={git.pullCurrent}
                  onPush={git.pushCurrent}
                  onOpenCommit={openCommitDialog}
                />
                <StatusSection
                  files={allStatusFiles}
                  loading={git.loadingStatus}
                  loadingDiff={git.loadingDiff}
                  actionLoading={git.actionLoading}
                  onLoadDiff={openDiffDialog}
                  onStageFiles={git.stageFiles}
                  onUnstageFiles={git.unstageFiles}
                  onDiscardFiles={git.discardFiles}
                />
                <ComparePanel
                  compareResult={git.compareResult}
                  loadingCompare={git.loadingCompare}
                  loadingDiff={git.loadingDiff}
                  onLoadFileDiff={openDiffDialog}
                  onClear={git.clearCompare}
                />
                <NewBranchRow
                  value={newBranchName}
                  disabled={git.actionLoading}
                  onChange={setNewBranchName}
                  onCreate={createBranch}
                />
              </>
            )}
            <BranchSection
              title={readOnly ? 'Harness' : 'Local'}
              branches={filteredBranches.locals}
              loading={git.loadingBranches}
              actionLoading={git.actionLoading}
              loadingCompare={git.loadingCompare}
              operationState={git.summary?.operationState}
              readOnly={readOnly}
              onCheckout={git.checkoutBranch}
              onMerge={git.mergeBranch}
              onCompare={git.compareBranch}
            />
            {!readOnly && (
              <BranchSection
                title="Remote"
                branches={filteredBranches.remotes}
                loading={git.loadingBranches}
                actionLoading={git.actionLoading}
                loadingCompare={git.loadingCompare}
                operationState={git.summary?.operationState}
                onCheckout={git.checkoutBranch}
                onMerge={git.mergeBranch}
                onCompare={git.compareBranch}
              />
            )}
          </>
        )}
      </div>

      <div className="flex items-center justify-between border-t border-border px-3 py-2">
        <span className="text-[11px] text-muted-foreground">
          {git.loadingSummary || git.loadingBranches || git.loadingStatus
            ? t('git.loading')
            : readOnly ? 'Harness Git' : activeRepoRoot || projectRoot}
        </span>
        <button
          type="button"
          onClick={closePanel}
          className="h-7 rounded border border-border px-2 text-xs hover:bg-accent"
        >
          {t('git.close')}
        </button>
      </div>
    </div>
  );
};

export const GitBranchDialogMounts: React.FC<{
  model: GitBranchButtonModel;
}> = ({ model }) => {
  const {
    closeCommitDialog,
    closeDiffDialog,
    commitMessage,
    commitOpen,
    diffDialogOpen,
    git,
    selectableCommitFiles,
    selectedCommitPaths,
    setCommitMessage,
    setCommitPathsSelected,
    submitCommit,
    submitStagedCommit,
    toggleCommitPath,
  } = model;

  return (
    <>
      {commitOpen && (
        <CommitDialog
          files={selectableCommitFiles}
          message={commitMessage}
          selectedPaths={selectedCommitPaths}
          actionLoading={git.actionLoading}
          onMessageChange={setCommitMessage}
          onTogglePath={toggleCommitPath}
          onSetPathsSelected={setCommitPathsSelected}
          onCancel={closeCommitDialog}
          onSubmit={() => { void submitCommit(); }}
          onSubmitStagedOnly={() => { void submitStagedCommit(); }}
        />
      )}

      <DiffDialog
        open={diffDialogOpen}
        fileDiff={git.fileDiff}
        loading={git.loadingDiff}
        error={git.error}
        onClose={closeDiffDialog}
      />
    </>
  );
};
