// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { Cloud, Clock3, HardDrive, Loader2, PlayCircle, RefreshCw, ScrollText } from 'lucide-react';

import { useI18n } from '../../i18n/I18nProvider';
import { useApiClient } from '../../lib/api/ApiClientContext';
import type {
  ProjectRuntimeEnvironmentProgressResponse,
  ProjectRuntimeEnvironmentResponse,
} from '../../lib/api/client/types';
import { resolveProjectSourceKind } from '../../lib/domain/projectSource';
import { cn } from '../../lib/utils';
import {
  actionNoticeForRuntimeStatus,
  type RuntimeActionNotice,
} from './cloudRuntimeActionNotice';
import {
  asRecord,
  buildDetectedStackView,
  displayValue,
  environmentRecord,
  formatDateTime,
  formatElapsed,
  readBoolean,
  readNumber,
  readString,
  serviceConfigEntries,
  statusTone,
} from './cloudRuntimeEnvironmentView';
import CloudRuntimeImagePlans from './CloudRuntimeImagePlans';
import RuntimeAnalysisRequirementDialog, {
  MAX_ANALYSIS_REQUIREMENT_LENGTH,
} from './RuntimeAnalysisRequirementDialog';

interface CloudProjectRuntimeEnvironmentPanelProps {
  projectId: string;
  projectName: string;
  projectSourceType?: string | null;
  projectRootPath?: string | null;
}


export const CloudProjectRuntimeEnvironmentPanel: React.FC<CloudProjectRuntimeEnvironmentPanelProps> = ({
  projectId,
  projectName,
  projectSourceType,
  projectRootPath,
}) => {
  const { t } = useI18n();
  const client = useApiClient();
  const projectSourceKind = resolveProjectSourceKind({
    sourceType: projectSourceType,
    rootPath: projectRootPath || '',
  });
  const cloudProjectSource = projectSourceKind === 'cloud';
  const ProjectSourceIcon = cloudProjectSource ? Cloud : HardDrive;
  const [response, setResponse] = useState<ProjectRuntimeEnvironmentResponse | null>(null);
  const [progress, setProgress] = useState<ProjectRuntimeEnvironmentProgressResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [analyzing, setAnalyzing] = useState(false);
  const [buildingImageId, setBuildingImageId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [actionNotice, setActionNotice] = useState<RuntimeActionNotice | null>(null);
  const [analysisDialogOpen, setAnalysisDialogOpen] = useState(false);
  const [analysisRequirement, setAnalysisRequirement] = useState('');
  const [selectedDependencies, setSelectedDependencies] = useState<string[]>([]);
  const [preferChinaMirrors, setPreferChinaMirrors] = useState(false);
  const [analysisRequirementError, setAnalysisRequirementError] = useState<string | null>(null);

  const loadEnvironment = useCallback(async () => {
    setLoading(true);
    setError(null);
    setActionNotice(null);
    try {
      setResponse(await client.getProjectRuntimeEnvironment(projectId));
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : t('cloudRuntime.loadFailed'));
    } finally {
      setLoading(false);
    }
  }, [client, projectId, t]);

  useEffect(() => {
    void loadEnvironment();
  }, [loadEnvironment]);

  const analyzeEnvironment = useCallback(async (
    requirement: string,
    dependencies: string[],
    preferChinaMirrors: boolean,
  ) => {
    setAnalyzing(true);
    setError(null);
    setActionNotice(null);
    setProgress(null);
    try {
      const nextResponse = await client.analyzeProjectRuntimeEnvironment(projectId, {
        analysis_requirement: requirement || undefined,
        selected_dependencies: dependencies,
        prefer_china_mirrors: preferChinaMirrors,
      });
      const nextEnvironment = asRecord(environmentRecord(nextResponse));
      const nextStatus = readString(nextEnvironment, ['status'], 'pending');
      const nextSummary = readString(nextEnvironment, ['analysis_summary', 'analysisSummary']);
      setResponse(nextResponse);
      setActionNotice(actionNoticeForRuntimeStatus(nextStatus, t, nextSummary));
    } catch (analyzeError) {
      setError(analyzeError instanceof Error ? analyzeError.message : t('cloudRuntime.analyzeFailed'));
    } finally {
      setAnalyzing(false);
    }
  }, [client, projectId, t]);

  const openAnalysisDialog = useCallback(() => {
    setAnalysisRequirementError(null);
    setPreferChinaMirrors(false);
    setAnalysisDialogOpen(true);
  }, []);

  const closeAnalysisDialog = useCallback(() => {
    setAnalysisRequirementError(null);
    setAnalysisDialogOpen(false);
  }, []);

  const submitAnalysisRequirement = useCallback(() => {
    const requirement = analysisRequirement.trim();
    if (requirement.length > MAX_ANALYSIS_REQUIREMENT_LENGTH) {
      setAnalysisRequirementError(t('cloudRuntime.requirementTooLong'));
      return;
    }
    if (!requirement && selectedDependencies.length === 0 && !preferChinaMirrors) {
      setAnalysisRequirementError(t('cloudRuntime.requirementRequired'));
      return;
    }
    setAnalysisRequirementError(null);
    setAnalysisDialogOpen(false);
    void analyzeEnvironment(requirement, selectedDependencies, preferChinaMirrors);
  }, [analysisRequirement, analyzeEnvironment, preferChinaMirrors, selectedDependencies, t]);

  const generateRuntimeImage = useCallback(async (imageId: string) => {
    setBuildingImageId(imageId);
    setError(null);
    setProgress(null);
    setActionNotice({
      tone: 'info',
      message: cloudProjectSource
        ? t('cloudRuntime.imageBuildSubmitted')
        : t('cloudRuntime.localBuildSubmitted'),
    });
    setResponse((current) => current ? {
      ...current,
      environment: {
        ...current.environment,
        status: 'pending_image_build',
      },
      images: (current.images || []).map((image) => (
        image.dockerfile
          ? { ...image, status: 'building', error: null }
          : { ...image, status: 'preparing', error: null }
      )),
    } : current);
    try {
      const nextResponse = await client.generateProjectRuntimeEnvironmentImage(projectId, imageId);
      const nextEnvironment = asRecord(environmentRecord(nextResponse));
      const nextStatus = readString(nextEnvironment, ['status'], 'pending');
      const nextSummary = readString(nextEnvironment, ['analysis_summary', 'analysisSummary']);
      setResponse(nextResponse);
      setActionNotice(actionNoticeForRuntimeStatus(nextStatus, t, nextSummary));
    } catch (buildError) {
      setError(buildError instanceof Error ? buildError.message : t('cloudRuntime.imageBuildFailed'));
      const [nextEnvironment, nextProgress] = await Promise.allSettled([
        client.getProjectRuntimeEnvironment(projectId),
        client.getProjectRuntimeEnvironmentProgress(projectId),
      ]);
      if (nextEnvironment.status === 'fulfilled') {
        setResponse(nextEnvironment.value);
      }
      if (nextProgress.status === 'fulfilled') {
        setProgress(nextProgress.value);
      }
    } finally {
      setBuildingImageId(null);
    }
  }, [client, cloudProjectSource, projectId, t]);

  const environment = environmentRecord(response);
  const environmentData = asRecord(environment);
  const status = readString(environmentData, ['status'], 'pending');
  const sandboxEnabled = readBoolean(environmentData, ['sandbox_enabled', 'sandboxEnabled'], true);
  const sandboxProvider = readString(environmentData, ['sandbox_provider', 'sandboxProvider'], 'cloud_sandbox_manager');
  const fileProvider = readString(environmentData, ['file_provider', 'fileProvider'], 'harness');
  const updatedAt = readString(environmentData, ['updated_at', 'updatedAt']);
  const agentRunId = readString(environmentData, ['last_agent_run_id', 'lastAgentRunId']);
  const analysisSummary = readString(environmentData, ['analysis_summary', 'analysisSummary']);
  const lastError = readString(environmentData, ['last_error', 'lastError']);
  const notRunnableReason = readString(environmentData, ['not_runnable_reason', 'notRunnableReason']);
  const detectedStack = environmentData.detected_stack ?? environmentData.detectedStack;
  const requiredServices = Array.isArray(environmentData.required_services)
    ? environmentData.required_services
    : Array.isArray(environmentData.requiredServices)
      ? environmentData.requiredServices
      : [];
  const envVars = asRecord(environmentData.env_vars ?? environmentData.envVars);
  const images = response?.images || [];
  const envEntries = useMemo(() => Object.entries(envVars), [envVars]);
  const detectedStackView = useMemo(() => buildDetectedStackView(detectedStack, t), [detectedStack, t]);
  const imagePreparationActive = images.some((image) => {
    const imageStatus = readString(asRecord(image), ['status']).toLowerCase();
    return ['building', 'preparing', 'running'].includes(imageStatus);
  });
  const backendAnalyzing = status === 'analyzing';
  const backendBuilding = status === 'pending_image_build' && imagePreparationActive;
  const backendBusy = backendAnalyzing || backendBuilding || buildingImageId !== null;
  const progressData = asRecord(progress);
  const progressStatus = readString(progressData, ['status']);
  const progressPhase = readString(
    progressData,
    ['phase'],
    backendAnalyzing ? 'analyzing_project' : backendBuilding ? 'building_image' : '',
  );
  const progressPercent = readNumber(progressData, ['progress_percent', 'progressPercent']);
  const progressJobId = readString(progressData, ['job_id', 'jobId', 'run_id', 'runId']);
  const progressImageId = readString(progressData, ['image_id', 'imageId']);
  const progressImageRef = readString(progressData, ['image_ref', 'imageRef']);
  const progressStartedAt = readString(progressData, ['started_at', 'startedAt']);
  const progressUpdatedAt = readString(progressData, ['updated_at', 'updatedAt']);
  const progressLogs = readString(progressData, ['logs']);
  const progressError = readString(progressData, ['error']);
  const progressActive = ['pending', 'queued', 'running', 'building'].includes(
    progressStatus.toLowerCase(),
  );
  const settledEnvironment = ['ready', 'pending_configuration', 'not_runnable', 'disabled'].includes(
    status.toLowerCase(),
  );
  const showProgress = backendBusy || progressActive || Boolean(progressError);
  const visibleNotice = status === 'pending_configuration'
    ? actionNotice || {
      tone: 'warning' as const,
      message: analysisSummary || t('cloudRuntime.configurationRequired'),
    }
    : actionNotice;

  useEffect(() => {
    if (!progress) {
      return;
    }
    if (!backendBusy && settledEnvironment && progressStatus.toLowerCase() !== 'failed') {
      setProgress(null);
    }
  }, [backendBusy, progress, progressStatus, settledEnvironment]);

  useEffect(() => {
    if (!backendBusy) {
      return undefined;
    }
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    const poll = async () => {
      const [environmentResult, progressResult] = await Promise.allSettled([
        client.getProjectRuntimeEnvironment(projectId),
        client.getProjectRuntimeEnvironmentProgress(projectId),
      ]);
      if (disposed) {
        return;
      }
      if (environmentResult.status === 'fulfilled') {
        setResponse(environmentResult.value);
        const nextEnvironment = asRecord(environmentRecord(environmentResult.value));
        const nextStatus = readString(
          nextEnvironment,
          ['status'],
          'pending',
        );
        if (nextStatus !== 'analyzing') {
          setActionNotice(actionNoticeForRuntimeStatus(
            nextStatus,
            t,
            readString(nextEnvironment, ['analysis_summary', 'analysisSummary']),
          ));
        }
      }
      if (progressResult.status === 'fulfilled') {
        setProgress(progressResult.value);
        const nextStatus = readString(asRecord(progressResult.value), ['status']);
        if (nextStatus === 'failed' || nextStatus === 'succeeded') {
          try {
            const finalResponse = await client.getProjectRuntimeEnvironment(projectId);
            const finalEnvironment = asRecord(environmentRecord(finalResponse));
            const finalStatus = readString(
              finalEnvironment,
              ['status'],
              'pending',
            );
            setResponse(finalResponse);
            setActionNotice(actionNoticeForRuntimeStatus(
              finalStatus,
              t,
              readString(finalEnvironment, ['analysis_summary', 'analysisSummary']),
            ));
          } catch {
            // The next scheduled poll will retry the project status refresh.
          }
        }
      }
      if (environmentResult.status === 'rejected' && progressResult.status === 'rejected') {
        const reason = progressResult.reason;
        setError(reason instanceof Error ? reason.message : t('cloudRuntime.progressFailed'));
      }
      if (!disposed) {
        timer = setTimeout(() => void poll(), 1500);
      }
    };

    void poll();
    return () => {
      disposed = true;
      if (timer) {
        clearTimeout(timer);
      }
    };
  }, [backendBusy, client, projectId, t]);

  return (
    <div className="overflow-hidden border border-border bg-card">
      <div className="flex flex-wrap items-start justify-between gap-3 px-4 py-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <ProjectSourceIcon
              className={cn('h-4 w-4', cloudProjectSource ? 'text-sky-600' : 'text-emerald-600')}
              aria-hidden="true"
            />
            <h2 className="truncate text-base font-semibold text-foreground">
              {projectName || t('cloudRuntime.title')}
            </h2>
            <span className="rounded-full border border-border bg-muted/40 px-2 py-0.5 text-[11px] text-muted-foreground">
              {t(cloudProjectSource ? 'cloudRuntime.source.cloud' : 'cloudRuntime.source.localConnector')}
            </span>
            <span className={cn('border px-2 py-0.5 text-[11px]', statusTone(status))}>
              {t(`cloudRuntime.status.${status}`)}
            </span>
          </div>
          <div className="mt-1 text-xs text-muted-foreground">
            {t(cloudProjectSource ? 'cloudRuntime.cloudManagedSubtitle' : 'cloudRuntime.localConnectorManagedSubtitle')}
          </div>
          <div className="mt-1 max-w-3xl text-[11px] leading-5 text-muted-foreground">
            {t(cloudProjectSource ? 'cloudRuntime.cloudGatewayHint' : 'cloudRuntime.localConnectorGatewayHint')}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <label className="inline-flex items-center gap-2 text-xs text-muted-foreground">
            <span>
              {cloudProjectSource
                ? t('cloudRuntime.sandboxFixed')
                : t('cloudRuntime.sandboxEnabled')}
            </span>
            <input
              type="checkbox"
              checked={sandboxEnabled}
              disabled
              readOnly
              className="peer sr-only"
              aria-label={cloudProjectSource
                ? t('cloudRuntime.sandboxFixed')
                : t('cloudRuntime.sandboxEnabled')}
            />
            <span className="relative h-6 w-11 rounded-full border border-primary bg-primary opacity-80 after:absolute after:left-0.5 after:top-0.5 after:h-5 after:w-5 after:translate-x-5 after:rounded-full after:bg-background after:shadow" />
          </label>
          <button
            type="button"
            onClick={() => void loadEnvironment()}
            disabled={loading}
            className="inline-flex h-8 items-center gap-1.5 border border-border bg-background px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
          >
            <RefreshCw className={cn('h-3.5 w-3.5', loading && 'animate-spin')} aria-hidden="true" />
            {t('common.refresh')}
          </button>
          <button
            type="button"
            onClick={openAnalysisDialog}
            disabled={loading || analyzing || backendBusy}
            className="inline-flex h-8 items-center gap-1.5 bg-primary px-3 text-xs text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {analyzing || backendBusy ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
            ) : (
              <PlayCircle className="h-3.5 w-3.5" aria-hidden="true" />
            )}
            {analyzing || backendBusy
              ? backendBuilding
                ? cloudProjectSource
                  ? t('cloudRuntime.preparingAllImages')
                  : t('cloudRuntime.localBuilding')
                : t('cloudRuntime.analyzing')
              : status === 'pending_configuration'
                ? t('cloudRuntime.checkConfiguration')
                : t('cloudRuntime.analyze')}
          </button>
        </div>
      </div>

      {analysisDialogOpen && (
        <RuntimeAnalysisRequirementDialog
          requirement={analysisRequirement}
          selectedDependencies={selectedDependencies}
          preferChinaMirrors={preferChinaMirrors}
          error={analysisRequirementError}
          onRequirementChange={(value) => {
            setAnalysisRequirement(value);
            setAnalysisRequirementError(null);
          }}
          onSelectedDependenciesChange={(dependencies) => {
            setSelectedDependencies(dependencies);
            setAnalysisRequirementError(null);
          }}
          onPreferChinaMirrorsChange={(value) => {
            setPreferChinaMirrors(value);
            setAnalysisRequirementError(null);
          }}
          onCancel={closeAnalysisDialog}
          onSubmit={submitAnalysisRequirement}
        />
      )}

      {visibleNotice && !(error || lastError || notRunnableReason) && (
        <div
          role={visibleNotice.tone === 'warning' ? 'alert' : 'status'}
          className={cn(
            'border-t px-4 py-3 text-xs',
            visibleNotice.tone === 'warning'
              ? 'border-amber-500/30 bg-amber-500/10 text-amber-800'
              : visibleNotice.tone === 'success'
                ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-800'
                : 'border-sky-500/30 bg-sky-500/10 text-sky-800',
          )}
        >
          {visibleNotice.message}
        </div>
      )}

      {(error || lastError || notRunnableReason) && (
        <div className="border-t border-border bg-destructive/5 px-4 py-3 text-xs text-destructive">
          {error || lastError || notRunnableReason}
        </div>
      )}

      {showProgress && (
        <section className="border-t border-border px-4 py-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex items-center gap-2">
              <Loader2
                className={cn('h-4 w-4 text-primary', (backendBusy || progressStatus === 'running') && 'animate-spin')}
                aria-hidden="true"
              />
              <h3 className="text-sm font-semibold text-foreground">{t('cloudRuntime.buildProgress')}</h3>
              {progressPhase && (
                <span className={cn('border px-2 py-0.5 text-[11px]', statusTone(progressStatus === 'failed' ? 'failed' : status))}>
                  {t(`cloudRuntime.phase.${progressPhase}`)}
                </span>
              )}
            </div>
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <Clock3 className="h-3.5 w-3.5" aria-hidden="true" />
              <span>{t('cloudRuntime.elapsed')}: {formatElapsed(progressStartedAt)}</span>
            </div>
          </div>

          <div
            className="mt-3 h-2 overflow-hidden bg-muted"
            role="progressbar"
            aria-label={t('cloudRuntime.buildProgress')}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={progressPercent ?? undefined}
          >
            {progressPercent == null ? (
              <div className="h-full w-1/3 animate-pulse bg-primary" />
            ) : (
              <div
                className={cn('h-full bg-primary transition-[width] duration-300', progressStatus === 'failed' && 'bg-destructive')}
                style={{ width: `${Math.min(100, Math.max(0, progressPercent))}%` }}
              />
            )}
          </div>

          <div className="mt-3 grid gap-x-6 gap-y-2 text-xs sm:grid-cols-2 xl:grid-cols-4">
            <div><span className="text-muted-foreground">{t('cloudRuntime.jobId')}: </span><span className="font-mono">{progressJobId || '-'}</span></div>
            <div><span className="text-muted-foreground">{t('cloudRuntime.image')}: </span><span className="font-mono">{progressImageRef || progressImageId || '-'}</span></div>
            <div><span className="text-muted-foreground">{t('cloudRuntime.startedAt')}: </span>{formatDateTime(progressStartedAt)}</div>
            <div><span className="text-muted-foreground">{t('cloudRuntime.updatedAt')}: </span>{formatDateTime(progressUpdatedAt)}</div>
          </div>

          {(progressLogs || progressError) && (
            <div className="mt-4 border border-border bg-background">
              <div className="flex items-center gap-2 border-b border-border bg-muted/40 px-3 py-2 text-xs font-medium text-foreground">
                <ScrollText className="h-3.5 w-3.5" aria-hidden="true" />
                {t('cloudRuntime.buildLogs')}
              </div>
              <pre className="max-h-80 min-h-32 overflow-auto whitespace-pre-wrap break-words p-3 font-mono text-[11px] leading-5 text-foreground">
                {progressLogs || progressError}
              </pre>
            </div>
          )}
        </section>
      )}

      <section className="border-t border-border px-4 py-4">
        <h3 className="text-sm font-semibold text-foreground">{t('cloudRuntime.initialization')}</h3>
        <div className="mt-3 overflow-x-auto border border-border">
          <table className="w-full min-w-[720px] text-left text-xs">
            <tbody className="divide-y divide-border">
              <tr className="divide-x divide-border">
                <th className="w-36 bg-muted/40 px-3 py-2 font-medium">{t('cloudRuntime.sandbox')}</th>
                <td className="px-3 py-2">{sandboxEnabled ? t('runSettings.sandboxEnabled') : t('runSettings.sandboxDisabled')}</td>
                <th className="w-36 bg-muted/40 px-3 py-2 font-medium">{t('cloudRuntime.status')}</th>
                <td className="px-3 py-2">{t(`cloudRuntime.status.${status}`)}</td>
                <th className="w-36 bg-muted/40 px-3 py-2 font-medium">{t('cloudRuntime.fileProvider')}</th>
                <td className="px-3 py-2 font-mono">{fileProvider}</td>
              </tr>
              <tr className="divide-x divide-border">
                <th className="bg-muted/40 px-3 py-2 font-medium">{t('cloudRuntime.sandboxProvider')}</th>
                <td className="px-3 py-2 font-mono">{sandboxProvider}</td>
                <th className="bg-muted/40 px-3 py-2 font-medium">{t('cloudRuntime.updatedAt')}</th>
                <td className="px-3 py-2">{formatDateTime(updatedAt)}</td>
                <th className="bg-muted/40 px-3 py-2 font-medium">{t('cloudRuntime.agentRun')}</th>
                <td className="px-3 py-2 font-mono">{agentRunId || '-'}</td>
              </tr>
              <tr className="divide-x divide-border">
                <th className="bg-muted/40 px-3 py-2 align-top font-medium">{t('cloudRuntime.summary')}</th>
                <td className="px-3 py-2 leading-5" colSpan={5}>{analysisSummary || t('cloudRuntime.noSummary')}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>

      <section className="border-t border-border px-4 py-4">
        <h3 className="text-sm font-semibold text-foreground">{t('cloudRuntime.detectedStack')}</h3>
        {detectedStackView.hasContent ? (
          <div className="mt-3 overflow-hidden border border-border">
            <table className="w-full table-fixed text-left text-xs">
              <tbody className="divide-y divide-border">
                {detectedStackView.summaryItems.length > 0 ? (
                  <tr className="align-top">
                    <th className="w-36 bg-muted/40 px-3 py-2 font-medium text-muted-foreground">
                      {t('cloudRuntime.stack.summary')}
                    </th>
                    <td className="px-3 py-2">
                      <ul className="list-disc space-y-1 pl-4 leading-5 text-foreground">
                        {detectedStackView.summaryItems.map((item) => (
                          <li key={item} className="break-words">{item}</li>
                        ))}
                      </ul>
                    </td>
                  </tr>
                ) : null}
                {detectedStackView.metrics.map((metric) => (
                  <tr key={metric.label} className="align-top">
                    <th className="w-36 bg-muted/40 px-3 py-2 font-medium text-muted-foreground">
                      {metric.label}
                    </th>
                    <td className="px-3 py-2 font-mono">{metric.value}</td>
                  </tr>
                ))}
                {detectedStackView.groups.map((group) => (
                  <tr key={group.label} className="align-top">
                    <th className="w-36 bg-muted/40 px-3 py-2 font-medium text-muted-foreground">
                      {group.label}
                    </th>
                    <td className="px-3 py-2">
                      <div className="flex flex-wrap gap-1.5">
                        {group.items.slice(0, 12).map((item) => (
                          <span key={item} className="max-w-full break-words border border-border bg-muted/40 px-2 py-0.5 font-mono text-[11px]">
                            {item}
                          </span>
                        ))}
                        {group.items.length > 12 ? (
                          <span className="px-2 py-0.5 text-muted-foreground">
                            {t('cloudRuntime.stack.andMore')} {group.items.length - 12}
                          </span>
                        ) : null}
                      </div>
                    </td>
                  </tr>
                ))}
                {detectedStackView.files.length > 0 ? (
                  <tr className="align-top">
                    <th className="w-36 bg-muted/40 px-3 py-2 font-medium text-muted-foreground">
                      {t('cloudRuntime.stack.evidenceFiles')} ({detectedStackView.files.length})
                    </th>
                    <td className="px-3 py-2">
                      <div className="flex flex-wrap gap-1.5">
                        {detectedStackView.files.slice(0, 8).map((file) => (
                          <span key={file} className="max-w-full break-words border border-border bg-muted/40 px-2 py-0.5 font-mono text-[11px]" title={file}>
                            {file}
                          </span>
                        ))}
                        {detectedStackView.files.length > 8 ? (
                          <span className="px-2 py-0.5 text-muted-foreground">
                            {t('cloudRuntime.stack.andMore')} {detectedStackView.files.length - 8}
                          </span>
                        ) : null}
                      </div>
                    </td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </div>
        ) : (
          <div className="mt-3 text-xs text-muted-foreground">{t('cloudRuntime.noDetectedStack')}</div>
        )}
      </section>

      <section className="border-t border-border px-4 py-4">
        <h3 className="text-sm font-semibold text-foreground">{t('cloudRuntime.requiredServices')}</h3>
        <div className="mt-3 overflow-x-auto border border-border">
          <table className="w-full min-w-[640px] text-left text-xs">
            <thead className="bg-muted/40 text-muted-foreground">
              <tr>
                <th className="px-3 py-2 font-medium">{t('cloudRuntime.serviceName')}</th>
                <th className="px-3 py-2 font-medium">{t('cloudRuntime.serviceType')}</th>
                <th className="px-3 py-2 font-medium">{t('cloudRuntime.description')}</th>
                <th className="px-3 py-2 font-medium">{t('cloudRuntime.configuration')}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {requiredServices.length === 0 ? (
                <tr><td className="px-3 py-6 text-center text-muted-foreground" colSpan={4}>{t('cloudRuntime.noServices')}</td></tr>
              ) : requiredServices.map((service, index) => {
                const record = asRecord(service);
                const config = asRecord(record.config ?? record.configuration);
                const configEntries = serviceConfigEntries(record, t);
                const serviceType = readString(
                  record,
                  ['type', 'kind'],
                  readString(config, ['type', 'kind', 'environment_key', 'environmentKey'], '-'),
                );
                const serviceName = readString(
                  record,
                  ['name', 'display_name', 'displayName', 'environment_key', 'environmentKey'],
                  readString(config, ['name', 'display_name', 'displayName', 'environment_key', 'environmentKey'], serviceType),
                );
                return (
                  <tr key={`${readString(record, ['id', 'name', 'type', 'kind'], serviceName || 'service')}-${index}`}>
                    <td className="px-3 py-2">{serviceName}</td>
                    <td className="px-3 py-2">{serviceType}</td>
                    <td className="px-3 py-2">{readString(record, ['description', 'detail'], '-')}</td>
                    <td className="max-w-lg px-3 py-2">
                      {configEntries.length > 0 ? (
                        <div className="flex flex-wrap gap-1.5">
                          {configEntries.map((entry) => (
                            <span key={`${entry.label}:${entry.value}`} className="border border-border bg-muted/40 px-2 py-0.5">
                              <span className="text-muted-foreground">{entry.label}: </span>
                              <span className="font-mono">{entry.value}</span>
                            </span>
                          ))}
                        </div>
                      ) : '-'}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </section>

      <section className="border-t border-border px-4 py-4">
        <h3 className="text-sm font-semibold text-foreground">{t('cloudRuntime.envVars')}</h3>
        <div className="mt-3 overflow-x-auto border border-border">
          <table className="w-full min-w-[520px] text-left text-xs">
            <thead className="bg-muted/40 text-muted-foreground">
              <tr>
                <th className="px-3 py-2 font-medium">{t('cloudRuntime.varName')}</th>
                <th className="px-3 py-2 font-medium">{t('cloudRuntime.varValue')}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {envEntries.length === 0 ? (
                <tr><td className="px-3 py-6 text-center text-muted-foreground" colSpan={2}>{t('cloudRuntime.noEnvVars')}</td></tr>
              ) : envEntries.map(([name, value]) => (
                <tr key={name}>
                  <td className="px-3 py-2 font-mono">{name}</td>
                  <td className="whitespace-pre-wrap px-3 py-2 font-mono">{displayValue(value)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <CloudRuntimeImagePlans
        images={images}
        isCloudProject={cloudProjectSource}
        buildingImageId={buildingImageId || (backendBuilding ? '__local_environment__' : null)}
        onGenerateImage={(imageId) => void generateRuntimeImage(imageId)}
      />
    </div>
  );
};

export default CloudProjectRuntimeEnvironmentPanel;
