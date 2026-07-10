// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { useApiClient } from '../../lib/api/ApiClientContext';
import { useAuthStoreSelector } from '../../lib/auth/authStore';
import { getUserVisiblePath } from '../../lib/domain/filesystem';
import { useTheme } from '../../hooks/useTheme';
import EmbeddedTerminalView from '../terminal/EmbeddedTerminalView';
import { RunEnvironmentDetails } from './projectRunSettingsPanel/RunEnvironmentDetails';
import { formatRunTargetOptionHint, formatRunTargetSource, getRunStatusLabel, resolveConfigKindsForTarget } from './projectRunSettingsPanel/model';
import type { ProjectRunSettingsPanelProps } from './projectRunSettingsPanel/types';

export const ProjectRunSettingsPanel: React.FC<ProjectRunSettingsPanelProps> = ({
  projectName,
  projectRootPath,
  runStatus,
  runCatalogLoading,
  runEnvironment,
  runEnvironmentLoading,
  runEnvironmentError,
  configFiles,
  validationIssues,
  runTargets,
  availableToolchainKinds,
  selectedToolchainOptions,
  missingToolchainKinds,
  customToolchainDrafts,
  envVarsDraft,
  commandPreview,
  envPreview,
  environmentHints,
  envVarsPlaceholder,
  sandboxToggleVisible,
  sandboxEnabled,
  sandboxLoading,
  sandboxSaving,
  sandboxError,
  showTerminalUi,
  selectedRunTargetId,
  starting,
  stopping,
  restarting,
  deleting,
  runnerMessage,
  runnerError,
  runnerDiagnosis,
  runnerSuggestions = [],
  projectRunState,
  projectRunInstances,
  selectedRunInstanceId,
  projectRunTerminal,
  projectRunTerminalBusy,
  onSelectRunTarget,
  onSelectRunInstance,
  onSelectToolchain,
  onApplySuggestion,
  onCustomToolchainDraftChange,
  onSaveCustomToolchain,
  onEnvVarsDraftChange,
  onSaveEnvVarsDraft,
  onSandboxEnabledChange,
  onRunnerStart,
  onRunnerStop,
  onRunnerRestart,
  onRunnerDelete,
  onRefreshRunnerState,
}) => {
  const { t } = useI18n();
  const terminalClient = useApiClient();
  const accessToken = useAuthStoreSelector((state) => state.accessToken);
  const { actualTheme } = useTheme();
  const selectedTarget = runTargets.find((target) => target.id === selectedRunTargetId) || runTargets[0] || null;
  const selectedTargetConfigKinds = resolveConfigKindsForTarget(selectedTarget);
  const selectedConfigFiles = configFiles.filter((file) => (
    selectedTargetConfigKinds.length === 0 || selectedTargetConfigKinds.includes(file.kind)
  ));
  const selectedTargetIssues = validationIssues.filter((issue) => (
    !selectedTarget?.id || !issue.targetId || issue.targetId === selectedTarget.id
  ));
  const otherTargetIssues = validationIssues.filter((issue) => (
    selectedTarget?.id && issue.targetId && issue.targetId !== selectedTarget.id
  ));
  const statusLabel = getRunStatusLabel(runStatus, t);
  const statusTone = runStatus === 'ready'
    ? 'text-emerald-700 border-emerald-500/30 bg-emerald-500/10'
    : runStatus === 'error'
      ? 'text-destructive border-destructive/30 bg-destructive/10'
      : 'text-muted-foreground border-border bg-background';
  const visibleProjectRootPath = projectRootPath
    ? getUserVisiblePath(projectRootPath)
    : t('runSettings.noProjectRoot');
  const visibleSelectedTargetCwd = selectedTarget?.cwd
    ? getUserVisiblePath(selectedTarget.cwd, projectRootPath)
    : '';
  const visibleSelectedTargetEntrypoint = selectedTarget?.entrypoint
    ? getUserVisiblePath(selectedTarget.entrypoint, projectRootPath)
    : '';
  const visibleSelectedTargetManifest = selectedTarget?.manifestPath
    ? getUserVisiblePath(selectedTarget.manifestPath, projectRootPath)
    : '';
  const sandboxToggleDisabled = sandboxLoading || sandboxSaving;
  const sandboxStatusText = sandboxLoading
    ? t('runSettings.sandboxLoading')
    : sandboxSaving
      ? t('runSettings.sandboxSaving')
      : sandboxEnabled
        ? t('runSettings.sandboxEnabled')
        : t('runSettings.sandboxDisabled');

  return (
    <div className="rounded-lg border border-border bg-card">
      <div className="border-b border-border px-4 py-3">
        <div className="min-w-0">
          <div className="truncate text-base font-semibold text-foreground">
            {projectName || t('runSettings.projectSettings')}
          </div>
          <div className="mt-1 truncate text-xs text-muted-foreground">
            {visibleProjectRootPath}
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-2 text-[11px]">
            <span className={`rounded border px-2 py-1 ${statusTone}`}>
              {t('runSettings.runStatus', { status: statusLabel })}
            </span>
            <span className="rounded border border-border px-2 py-1 text-muted-foreground">
              {t('runSettings.runTargetsCount', { count: runTargets.length })}
            </span>
            {selectedTarget?.language && (
              <span className="rounded border border-border px-2 py-1 text-muted-foreground">
                {t('runSettings.language', { language: selectedTarget.language })}
              </span>
            )}
          </div>
        </div>

        {(runnerError || runnerMessage || runEnvironmentError || runnerDiagnosis) && (
          <div className="mt-3 space-y-2 rounded border border-border/70 bg-background/60 px-3 py-2 text-xs">
            {runnerError && (
              <div className="text-destructive">{runnerError}</div>
            )}
            {runEnvironmentError && (
              <div className="text-destructive">{runEnvironmentError}</div>
            )}
            {runnerMessage && (
              <div className="text-emerald-700">{runnerMessage}</div>
            )}
            {runnerDiagnosis && !runnerError?.includes(runnerDiagnosis) && (
              <div className="text-amber-700">
                {t('runSettings.latestExitDiagnosis', { diagnosis: runnerDiagnosis })}
              </div>
            )}
          </div>
        )}
      </div>

      <div className="space-y-4 p-4">
        {sandboxToggleVisible && (
          <div className="rounded border border-border/70 bg-background/50 p-3">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="min-w-0">
                <div className="text-sm font-medium text-foreground">
                  {t('runSettings.sandboxExecution')}
                </div>
                <div className="mt-1 text-xs text-muted-foreground">
                  {t('runSettings.sandboxExecutionDescription')}
                </div>
              </div>
              <label
                className={[
                  'inline-flex shrink-0 items-center gap-2 text-xs text-muted-foreground',
                  sandboxToggleDisabled ? 'cursor-not-allowed opacity-70' : 'cursor-pointer',
                ].join(' ')}
              >
                <span>{sandboxStatusText}</span>
                <input
                  type="checkbox"
                  className="peer sr-only"
                  checked={sandboxEnabled === true}
                  disabled={sandboxToggleDisabled}
                  aria-label={t('runSettings.sandboxExecution')}
                  onChange={(event) => onSandboxEnabledChange(event.target.checked)}
                />
                <span
                  className={[
                    'relative h-6 w-11 rounded-full border transition-colors after:absolute after:left-0.5 after:top-0.5 after:h-5 after:w-5 after:rounded-full after:bg-background after:shadow after:transition-transform peer-focus-visible:outline-none peer-focus-visible:ring-2 peer-focus-visible:ring-primary/40',
                    sandboxEnabled
                      ? 'border-primary bg-primary after:translate-x-5'
                      : 'border-border bg-muted',
                  ].join(' ')}
                />
              </label>
            </div>
            {sandboxError && (
              <div className="mt-2 text-xs text-destructive">
                {sandboxError}
              </div>
            )}
          </div>
        )}

        {runnerDiagnosis && (
          <div className="rounded border border-amber-500/30 bg-amber-500/5 p-3">
            <div className="mb-2 text-[11px] text-muted-foreground">{t('runSettings.latestRunDiagnosis')}</div>
            <div className="text-sm text-amber-800">{runnerDiagnosis}</div>
            <div className="mt-2 text-[11px] text-muted-foreground">
              {t('runSettings.diagnosisDescription')}
            </div>
            {runnerSuggestions.length > 0 && (
              <div className="mt-3 space-y-2">
                <div className="text-[11px] text-muted-foreground">{t('runSettings.suggestions')}</div>
                <div className="flex flex-wrap gap-2">
                  {runnerSuggestions.map((suggestion) => (
                    <button
                      key={suggestion.id}
                      type="button"
                      onClick={() => onApplySuggestion(suggestion)}
                      className="rounded border border-amber-500/40 bg-background px-3 py-1.5 text-xs text-amber-800 hover:bg-amber-500/10"
                      title={suggestion.detail || suggestion.label}
                    >
                      {suggestion.label}
                    </button>
                  ))}
                </div>
                {runnerSuggestions.some((item) => item.detail) && (
                  <div className="space-y-1 text-[11px] text-muted-foreground">
                    {runnerSuggestions.map((suggestion) => (
                      suggestion.detail ? (
                        <div key={`${suggestion.id}:detail`} className="break-all">
                          {suggestion.label}: {suggestion.detail}
                        </div>
                      ) : null
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
        )}

        <div className="rounded border border-border/70 bg-background/50 p-3">
          <div className="mb-2 text-[11px] text-muted-foreground">{t('runSettings.preflight')}</div>
          {selectedTargetIssues.length === 0 && otherTargetIssues.length === 0 ? (
            <div className="text-sm text-emerald-700">
              {t('runSettings.preflightClean')}
            </div>
          ) : (
            <div className="space-y-3">
              {selectedTargetIssues.map((issue, index) => (
                <div key={`${issue.kind}:${issue.path || index}`} className="rounded border border-destructive/30 bg-destructive/5 p-3">
                  <div className="text-sm text-destructive">{issue.message}</div>
                  {issue.path && (
                    <div className="mt-2 break-all font-mono text-[11px] text-muted-foreground">
                      {getUserVisiblePath(issue.path, projectRootPath)}
                    </div>
                  )}
                  {issue.hint && (
                    <div className="mt-2 text-[11px] text-muted-foreground">
                      {t('runSettings.issueHint', { hint: issue.hint })}
                    </div>
                  )}
                </div>
              ))}
              {otherTargetIssues.length > 0 && (
                <details className="rounded border border-border/60 bg-card">
                  <summary className="cursor-pointer list-none px-3 py-2 text-xs text-muted-foreground">
                    {t('runSettings.otherTargetIssues', { count: otherTargetIssues.length })}
                  </summary>
                  <div className="border-t border-border/60 space-y-3 px-3 py-3">
                    {otherTargetIssues.map((issue, index) => (
                      <div key={`${issue.kind}:${issue.targetId || issue.path || index}`} className="rounded border border-border/60 bg-background p-3">
                        <div className="text-sm text-foreground">
                          {issue.targetLabel ? `[${issue.targetLabel}] ` : ''}
                          {issue.message}
                        </div>
                        {issue.path && (
                          <div className="mt-2 break-all font-mono text-[11px] text-muted-foreground">
                            {getUserVisiblePath(issue.path, projectRootPath)}
                          </div>
                        )}
                        {issue.hint && (
                          <div className="mt-2 text-[11px] text-muted-foreground">
                            {t('runSettings.issueHint', { hint: issue.hint })}
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                </details>
              )}
            </div>
          )}
        </div>

        <div className="rounded border border-border/70 bg-background/50 p-3">
          <div className="mb-2 text-[11px] text-muted-foreground">{t('runSettings.runTargets')}</div>
          {runTargets.length === 0 ? (
            <div className="text-sm text-muted-foreground">{t('runSettings.noRunTargets')}</div>
          ) : (
            <div className="space-y-3">
              <div className="flex flex-wrap items-center gap-2">
                <select
                  value={selectedRunTargetId || runTargets[0]?.id || ''}
                  onChange={(event) => onSelectRunTarget(event.target.value)}
                  disabled={starting || stopping || restarting || deleting || runCatalogLoading}
                  className="h-9 min-w-[280px] max-w-[520px] rounded border border-border bg-background px-2 text-sm text-foreground disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {runTargets.map((target) => (
                    <option key={target.id} value={target.id}>
                      {[target.label, formatRunTargetOptionHint(target, t)].filter(Boolean).join(' · ')}
                    </option>
                  ))}
                </select>
                <div className="truncate text-xs text-muted-foreground" title={visibleSelectedTargetCwd}>
                  {visibleSelectedTargetCwd}
                </div>
              </div>

              {selectedTarget && (
                <div className="flex flex-wrap gap-2 text-[11px] text-muted-foreground">
                  <span className="rounded border border-border px-2 py-1">
                    {formatRunTargetSource(selectedTarget, t)}
                  </span>
                  {visibleSelectedTargetEntrypoint && (
                    <span className="rounded border border-border px-2 py-1" title={visibleSelectedTargetEntrypoint}>
                      {t('runSettings.entrypoint', { entrypoint: visibleSelectedTargetEntrypoint })}
                    </span>
                  )}
                  {visibleSelectedTargetManifest && (
                    <span className="rounded border border-border px-2 py-1" title={visibleSelectedTargetManifest}>
                      {t('runSettings.manifest', { manifest: visibleSelectedTargetManifest })}
                    </span>
                  )}
                  <span className="rounded border border-border px-2 py-1">
                    {t('runSettings.command', { command: selectedTarget.command })}
                  </span>
                </div>
              )}

              <div className="flex flex-wrap items-center gap-2 border-t border-border/60 pt-3">
                <button
                  type="button"
                  onClick={onRunnerStart}
                  disabled={runStatus !== 'ready' || starting || stopping || restarting || deleting || runCatalogLoading}
                  className="h-8 rounded border border-emerald-500/40 px-3 text-xs text-emerald-700 hover:bg-emerald-500/10 disabled:cursor-not-allowed disabled:opacity-50"
                  title={commandPreview}
                >
                  {starting ? t('runSettings.starting') : t('runSettings.startNew')}
                </button>
                <button
                  type="button"
                  onClick={onRunnerStop}
                  disabled={runStatus !== 'ready' || starting || stopping || restarting || deleting || !selectedRunInstanceId}
                  className="h-8 rounded border border-rose-500/40 px-3 text-xs text-rose-700 hover:bg-rose-500/10 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {stopping ? t('runSettings.stopping') : t('runSettings.stopCurrent')}
                </button>
                <button
                  type="button"
                  onClick={onRunnerRestart}
                  disabled={runStatus !== 'ready' || starting || stopping || restarting || deleting || runCatalogLoading || !selectedRunInstanceId}
                  className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                  title={commandPreview}
                >
                  {restarting ? t('runSettings.restarting') : t('runSettings.restartCurrent')}
                </button>
                <button
                  type="button"
                  onClick={onRunnerDelete}
                  disabled={starting || stopping || restarting || deleting || !selectedRunInstanceId}
                  className="h-8 rounded border border-destructive/40 px-3 text-xs text-destructive hover:bg-destructive/10 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {deleting ? t('runSettings.deleting') : t('runSettings.deleteCurrent')}
                </button>
                <button
                  type="button"
                  onClick={onRefreshRunnerState}
                  disabled={runCatalogLoading || runEnvironmentLoading || deleting}
                  className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {runCatalogLoading || runEnvironmentLoading ? t('runSettings.refreshing') : t('runSettings.refreshStatus')}
                </button>
              </div>
            </div>
          )}
        </div>

        {showTerminalUi ? (
          <div className="rounded border border-border/70 bg-background/50 p-3">
            <div className="mb-2 flex items-center justify-between gap-3">
              <div className="text-[11px] text-muted-foreground">{t('runSettings.instances')}</div>
              <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                <span className="rounded border border-border px-2 py-1">
                  {t('runSettings.instanceCount', { count: projectRunInstances.length })}
                </span>
                <span className="rounded border border-border px-2 py-1">
                  {t('runSettings.projectStatus', { status: projectRunState?.status || 'idle' })}
                </span>
              </div>
            </div>

            {projectRunInstances.length === 0 ? (
              <div className="text-sm text-muted-foreground">
                {t('runSettings.noInstances')}
              </div>
            ) : (
              <div className="space-y-3">
                <div className="flex flex-wrap gap-2">
                  {projectRunInstances.map((instance, index) => {
                    const selected = instance.terminalId === selectedRunInstanceId;
                    return (
                      <button
                        key={instance.terminalId}
                        type="button"
                        onClick={() => onSelectRunInstance(instance.terminalId)}
                        className={[
                          'rounded border px-3 py-2 text-left text-xs transition-colors',
                          selected
                            ? 'border-primary bg-primary/10 text-foreground'
                            : 'border-border bg-card text-muted-foreground hover:bg-accent',
                        ].join(' ')}
                      >
                        <div className="font-medium text-foreground">
                          {t('runSettings.instance', { index: index + 1 })}
                        </div>
                        <div className="mt-1">
                          {instance.running ? (instance.busy ? t('runSettings.running') : t('runSettings.idle')) : instance.status}
                        </div>
                      </button>
                    );
                  })}
                </div>

                <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                  <span className="rounded border border-border px-2 py-1">
                    {t('runSettings.terminalStatus', { status: projectRunTerminal?.status || 'idle' })}
                  </span>
                  <span className="rounded border border-border px-2 py-1">
                    {t('runSettings.process', {
                      status: projectRunTerminal
                        ? (projectRunTerminalBusy ? t('runSettings.running') : (projectRunTerminal.status === 'running' ? t('runSettings.idle') : t('runSettings.notRunning')))
                        : t('runSettings.notRunning'),
                    })}
                  </span>
                  {projectRunTerminal?.name && (
                    <span className="rounded border border-border px-2 py-1">
                      {projectRunTerminal.name}
                    </span>
                  )}
                </div>
              </div>
            )}
          </div>
        ) : (
          <div className="rounded border border-border/70 bg-background/50 p-3 text-sm text-muted-foreground">
            {t('runSettings.terminalUiDisabledInSettings')}
          </div>
        )}

        <RunEnvironmentDetails
          projectRootPath={projectRootPath}
          availableToolchainKinds={availableToolchainKinds}
          commandPreview={commandPreview}
          customToolchainDrafts={customToolchainDrafts}
          deleting={deleting}
          environmentHints={environmentHints}
          envPreview={envPreview}
          envVarsDraft={envVarsDraft}
          envVarsPlaceholder={envVarsPlaceholder}
          missingToolchainKinds={missingToolchainKinds}
          onCustomToolchainDraftChange={onCustomToolchainDraftChange}
          onEnvVarsDraftChange={onEnvVarsDraftChange}
          onSaveCustomToolchain={onSaveCustomToolchain}
          onSaveEnvVarsDraft={onSaveEnvVarsDraft}
          onSelectToolchain={onSelectToolchain}
          restarting={restarting}
          runEnvironment={runEnvironment}
          runEnvironmentLoading={runEnvironmentLoading}
          selectedConfigFiles={selectedConfigFiles}
          selectedTarget={selectedTarget}
          selectedToolchainOptions={selectedToolchainOptions}
          starting={starting}
          stopping={stopping}
          t={t}
        />

        {showTerminalUi && runEnvironment && (
          <div className="rounded border border-border/70 bg-background/50 p-3">
            <div className="mb-2 text-[11px] text-muted-foreground">{t('runSettings.terminal')}</div>
            <div className="h-[420px] overflow-hidden rounded border border-border/60 bg-card">
              <EmbeddedTerminalView
                terminal={projectRunTerminal}
                emptyText={t('runSettings.terminalEmpty')}
                client={terminalClient}
                accessToken={accessToken}
                actualTheme={actualTheme}
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default ProjectRunSettingsPanel;
