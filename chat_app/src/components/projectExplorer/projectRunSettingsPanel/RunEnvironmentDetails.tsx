import React from 'react';

import type { TranslateFn } from '../../../i18n/I18nProvider';
import type {
  ProjectRunEnvironment,
  ProjectRunTarget,
  ProjectRunToolchainOption,
} from '../../../types';
import {
  buildInjectedEnvHint,
  formatToolchainKind,
  formatToolchainSource,
  resolveConfigFilesEmptyText,
  resolveConfigSectionTitle,
  resolveManualHint,
} from './model';

interface ProjectRunConfigFile {
  kind: string;
  label: string;
  path: string;
  preview?: string | null;
  source: string;
}

interface RunEnvironmentDetailsProps {
  availableToolchainKinds: string[];
  commandPreview: string;
  customToolchainDrafts: Record<string, string>;
  deleting: boolean;
  environmentHints: string[];
  envPreview: string;
  envVarsDraft: string;
  envVarsPlaceholder: string;
  missingToolchainKinds: string[];
  onCustomToolchainDraftChange: (kind: string, value: string) => void;
  onEnvVarsDraftChange: (value: string) => void;
  onSaveCustomToolchain: (kind: string) => void;
  onSaveEnvVarsDraft: () => void;
  onSelectToolchain: (kind: string, optionId: string) => void;
  restarting: boolean;
  runEnvironment: ProjectRunEnvironment | null;
  runEnvironmentLoading: boolean;
  selectedConfigFiles: ProjectRunConfigFile[];
  selectedTarget: ProjectRunTarget | null;
  selectedToolchainOptions: Record<string, ProjectRunToolchainOption | null>;
  starting: boolean;
  stopping: boolean;
  t: TranslateFn;
}

export const RunEnvironmentDetails: React.FC<RunEnvironmentDetailsProps> = ({
  availableToolchainKinds,
  commandPreview,
  customToolchainDrafts,
  deleting,
  environmentHints,
  envPreview,
  envVarsDraft,
  envVarsPlaceholder,
  missingToolchainKinds,
  onCustomToolchainDraftChange,
  onEnvVarsDraftChange,
  onSaveCustomToolchain,
  onSaveEnvVarsDraft,
  onSelectToolchain,
  restarting,
  runEnvironment,
  runEnvironmentLoading,
  selectedConfigFiles,
  selectedTarget,
  selectedToolchainOptions,
  starting,
  stopping,
  t,
}) => (
  <details className="rounded border border-border/70 bg-background/50" open={false}>
    <summary className="cursor-pointer list-none px-3 py-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="text-[11px] text-muted-foreground">{t('runSettings.runEnvironment')}</div>
        <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
          <span className="rounded border border-border px-2 py-1">
            {t('runSettings.toolchainItems', { count: availableToolchainKinds.length })}
          </span>
          {missingToolchainKinds.length > 0 && (
            <span className="rounded border border-amber-500/30 bg-amber-500/10 px-2 py-1 text-amber-700">
              {t('runSettings.missing', { count: missingToolchainKinds.length })}
            </span>
          )}
          <span className="rounded border border-border px-2 py-1">
            {t('runSettings.collapsedDefault')}
          </span>
        </div>
      </div>
    </summary>
    <div className="border-t border-border/60 px-3 py-3">
      {availableToolchainKinds.length === 0 ? (
        <div className="text-sm text-muted-foreground">{t('runSettings.noToolchainNeeded')}</div>
      ) : (
        <div className="grid gap-3 md:grid-cols-2">
          {availableToolchainKinds.map((kind) => {
            const options = runEnvironment?.optionsByKind[kind] || [];
            const selectedOption = selectedToolchainOptions[kind];
            const isMissing = missingToolchainKinds.includes(kind);
            const manualDraft = customToolchainDrafts[kind] || '';
            const showManualInput = isMissing || selectedOption?.source === 'manual';
            return (
              <div key={kind} className="rounded border border-border/60 bg-card p-3">
                <div className="mb-1 flex items-center justify-between gap-3">
                  <div className="text-xs font-medium text-foreground">{formatToolchainKind(kind)}</div>
                  {options.length > 0 && (
                    <div className="text-[11px] text-muted-foreground">
                      {t('runSettings.foundOptions', { count: options.length })}
                    </div>
                  )}
                </div>
                <select
                  value={selectedOption?.id || options[0]?.id || ''}
                  onChange={(event) => onSelectToolchain(kind, event.target.value)}
                  disabled={options.length === 0 || starting || stopping || restarting || deleting || runEnvironmentLoading}
                  className="h-9 w-full rounded border border-border bg-background px-2 text-sm text-foreground disabled:cursor-not-allowed disabled:opacity-50"
                  title={selectedOption?.path || kind}
                >
                  {options.length === 0 ? (
                    <option value="">{t('runSettings.notFoundToolchain', { name: formatToolchainKind(kind) })}</option>
                  ) : (
                    options.map((option) => (
                      <option key={option.id} value={option.id}>
                        {option.label} · {formatToolchainSource(option.source, t)}
                      </option>
                    ))
                  )}
                </select>
                <div className="mt-2 space-y-2">
                  <div className="truncate text-[11px] text-muted-foreground" title={selectedOption?.path || ''}>
                    {selectedOption?.path || (isMissing ? t('runSettings.missingToolchainPath', { name: formatToolchainKind(kind) }) : '')}
                  </div>
                  {selectedOption && (
                    <div className="flex flex-wrap gap-2 text-[11px] text-muted-foreground">
                      <span className="rounded border border-border px-2 py-1">
                        {t('runSettings.source', { source: formatToolchainSource(selectedOption.source, t) })}
                      </span>
                      {selectedOption.version && (
                        <span className="rounded border border-border px-2 py-1">
                          {t('runSettings.versionHint', { version: selectedOption.version })}
                        </span>
                      )}
                    </div>
                  )}
                </div>

                <details className="mt-3 rounded border border-dashed border-border/70 bg-background/40">
                  <summary className="cursor-pointer list-none px-3 py-2 text-xs text-muted-foreground">
                    {showManualInput ? t('runSettings.manualPath') : t('runSettings.manualPathAlt')}
                  </summary>
                  <div className="border-t border-border/60 px-3 py-3">
                    <div className="mb-2 text-[11px] text-muted-foreground">
                      {resolveManualHint(kind, t)}
                    </div>
                    <div className="flex items-center gap-2">
                      <input
                        value={manualDraft}
                        onChange={(event) => onCustomToolchainDraftChange(kind, event.target.value)}
                        placeholder={t('runSettings.manualPathPlaceholder', { name: formatToolchainKind(kind) })}
                        className="h-9 flex-1 rounded border border-border bg-background px-3 text-sm text-foreground"
                      />
                      <button
                        type="button"
                        onClick={() => onSaveCustomToolchain(kind)}
                        disabled={!manualDraft.trim() || runEnvironmentLoading}
                        className="h-9 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        {t('runSettings.saveAndSelect')}
                      </button>
                    </div>
                  </div>
                </details>
              </div>
            );
          })}
        </div>
      )}

      <div className="mt-4 rounded border border-border/70 bg-background/50 p-3">
        <div className="mb-2 text-[11px] text-muted-foreground">{resolveConfigSectionTitle(selectedTarget, t)}</div>
        {selectedConfigFiles.length === 0 ? (
          <div className="text-sm text-muted-foreground">
            {resolveConfigFilesEmptyText(selectedTarget, availableToolchainKinds, t)}
          </div>
        ) : (
          <div className="space-y-3">
            {selectedConfigFiles.map((file) => (
              <div key={`${file.kind}:${file.path}`} className="rounded border border-border/60 bg-card p-3">
                <div className="flex flex-wrap items-center gap-2 text-xs">
                  <span className="font-medium text-foreground">{file.label}</span>
                  <span className="rounded border border-border px-2 py-1 text-[11px] text-muted-foreground">
                    {t('runSettings.source', { source: formatToolchainSource(file.source, t) })}
                  </span>
                </div>
                <div className="mt-2 break-all font-mono text-[11px] text-muted-foreground">
                  {file.path}
                </div>
                {file.preview && (
                  <div className="mt-2 rounded border border-border/60 bg-background px-2 py-2 font-mono text-[11px] text-foreground">
                    {file.preview}
                  </div>
                )}
              </div>
            ))}
            <div className="text-[11px] text-muted-foreground">
              {t('runSettings.configReadonlyHint')}
            </div>
          </div>
        )}
      </div>

      <div className="mt-4 rounded border border-border/70 bg-background/50 p-3">
        <div className="mb-2 text-[11px] text-muted-foreground">{t('runSettings.envVars')}</div>
        {environmentHints.length > 0 && (
          <div className="mb-3 flex flex-wrap gap-2 text-[11px] text-muted-foreground">
            {environmentHints.map((hint) => (
              <span key={hint} className="rounded border border-border px-2 py-1">
                {hint}
              </span>
            ))}
          </div>
        )}
        <textarea
          value={envVarsDraft}
          onChange={(event) => onEnvVarsDraftChange(event.target.value)}
          placeholder={envVarsPlaceholder}
          className="min-h-[120px] w-full rounded border border-border bg-background px-3 py-2 font-mono text-xs text-foreground"
        />
        <div className="mt-2 flex items-center justify-between gap-3">
          <div className="text-[11px] text-muted-foreground">
            {t('runSettings.envVarsHelp', { hint: buildInjectedEnvHint(availableToolchainKinds, t) })}
          </div>
          <button
            type="button"
            onClick={onSaveEnvVarsDraft}
            disabled={runEnvironmentLoading}
            className="h-9 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
          >
            {t('runSettings.saveEnvVars')}
          </button>
        </div>
        <div className="mt-3 rounded border border-border/60 bg-card p-3">
          <div className="mb-2 text-[11px] text-muted-foreground">{t('runSettings.envPreview')}</div>
          <pre className="overflow-x-auto whitespace-pre-wrap break-all font-mono text-xs text-foreground">
            {envPreview || t('runSettings.noEnvPreview')}
          </pre>
        </div>
      </div>

      <div className="mt-4 rounded border border-border/70 bg-background/50 p-3">
        <div className="mb-2 text-[11px] text-muted-foreground">{t('runSettings.commandPreview')}</div>
        <pre className="overflow-x-auto whitespace-pre-wrap break-all font-mono text-xs text-foreground">
          {commandPreview || t('runSettings.noCommand')}
        </pre>
      </div>
    </div>
  </details>
);
