// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useState } from 'react';
import type { Dispatch, SetStateAction } from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import type ApiClient from '../../../lib/api/client';
import { readProjectRunnerDispatchTarget } from '../../../lib/domain/projectRunner';
import type { ProjectRunnerActiveTerminal } from '../../../lib/domain/projectRunner';
import type { Project, ProjectRunTarget } from '../../../types';
import {
  buildProjectRunnerSelectedTerminalId,
  buildProjectRunnerDispatchState,
  resolveProjectRunnerDeleteTarget,
} from './projectRunnerCommandState';
import { extractProjectRunnerValidationMessage } from './projectRunnerCommandErrors';

interface UseProjectRunnerCommandsOptions {
  client: ApiClient;
  project: Project | null;
  runTargets: ProjectRunTarget[];
  selectedRunTargetId: string | null;
  commandPreview: string;
  activeRun: ProjectRunnerActiveTerminal | null;
  projectRunTerminalIds: string[];
  selectedTerminalId: string | null;
  selectRunInstance: (terminalId: string | null) => void;
  setActiveRun: (value: ProjectRunnerActiveTerminal | null) => void;
  setLastExitedRun: Dispatch<SetStateAction<ProjectRunnerActiveTerminal | null>>;
  setActiveTerminalBusy: (value: boolean) => void;
  removeRunInstanceLocally: (terminalId: string, nextSelectedTerminalId?: string | null) => void;
  refreshProjectActiveRun: () => Promise<void>;
}

export const useProjectRunnerCommands = ({
  client,
  project,
  runTargets,
  selectedRunTargetId,
  commandPreview,
  activeRun,
  projectRunTerminalIds,
  selectedTerminalId,
  selectRunInstance,
  setActiveRun,
  setLastExitedRun,
  setActiveTerminalBusy,
  removeRunInstanceLocally,
  refreshProjectActiveRun,
}: UseProjectRunnerCommandsOptions) => {
  const { t } = useI18n();
  const [starting, setStarting] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [restarting, setRestarting] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [runnerMessage, setRunnerMessage] = useState<string | null>(null);
  const [runnerError, setRunnerError] = useState<string | null>(null);
  const [runnerDiagnosis, setRunnerDiagnosis] = useState<string | null>(null);
  const [manualControlAt, setManualControlAt] = useState(0);
  const [lastExitCheckedRunKey, setLastExitCheckedRunKey] = useState('');

  const selectedRunTarget = runTargets.find((item) => item.id === selectedRunTargetId) || runTargets[0] || null;

  const dispatchProjectRunTarget = useCallback(async (
    target: ProjectRunTarget,
    label: string,
    preferredTerminalId?: string | null,
  ) => {
    if (!project?.id) {
      throw new Error(t('runSettings.error.projectMissing'));
    }
    const result = await client.executeProjectRun(project.id, {
      target_id: target.id,
      create_if_missing: true,
      terminal_id: preferredTerminalId || undefined,
    });
    const { terminalId, terminalName } = readProjectRunnerDispatchTarget(result);
    const nextState = buildProjectRunnerDispatchState({ target, terminalId, terminalName, commandPreview });
    setLastExitedRun(null);
    if (nextState) {
      selectRunInstance(nextState.terminalId);
      setActiveRun(nextState);
      setActiveTerminalBusy(true);
      setLastExitCheckedRunKey('');
    }
    setRunnerMessage(
      terminalName
        ? t('runSettings.commandDispatchedInTerminal', { label, terminal: terminalName })
        : t('runSettings.commandDispatched', { label }),
    );
    setRunnerDiagnosis(null);
  }, [
    commandPreview,
    client,
    project?.id,
    selectRunInstance,
    setActiveRun,
    setLastExitedRun,
    setActiveTerminalBusy,
    t,
  ]);

  const handleRunnerStart = useCallback(async () => {
    setStarting(true);
    setRunnerError(null);
    try {
      if (!selectedRunTarget) {
        throw new Error(t('runSettings.error.noRunTarget'));
      }
      await dispatchProjectRunTarget(selectedRunTarget, t('runSettings.startSuccess'), null);
    } catch (error) {
      setRunnerError(extractProjectRunnerValidationMessage(error, error instanceof Error ? error.message : t('runSettings.startFailed'), t));
      setRunnerMessage(null);
    } finally {
      setStarting(false);
    }
  }, [dispatchProjectRunTarget, selectedRunTarget, t]);

  const handleRunnerStop = useCallback(async () => {
    setStopping(true);
    setRunnerError(null);
    try {
      const terminalId = buildProjectRunnerSelectedTerminalId(selectedTerminalId || activeRun?.terminalId);
      if (!terminalId) {
        throw new Error(t('runSettings.error.noDedicatedTerminal'));
      }
      setManualControlAt(Date.now());
      await client.interruptTerminal(terminalId, { reason: 'project_run_stop' });
      setActiveTerminalBusy(false);
      setRunnerMessage(t('runSettings.stopSuccess'));
      setRunnerDiagnosis(null);
    } catch (error) {
      setRunnerError(error instanceof Error ? error.message : t('runSettings.stopFailed'));
      setRunnerMessage(null);
    } finally {
      setStopping(false);
    }
  }, [
    activeRun?.terminalId,
    client,
    selectedTerminalId,
    setActiveTerminalBusy,
    t,
  ]);

  const handleRunnerRestart = useCallback(async () => {
    setRestarting(true);
    setRunnerError(null);
    try {
      if (!project?.id) {
        throw new Error(t('runSettings.error.projectMissing'));
      }
      if (!selectedRunTarget) {
        throw new Error(t('runSettings.error.noRunTarget'));
      }
      const terminalId = buildProjectRunnerSelectedTerminalId(selectedTerminalId || activeRun?.terminalId);
      if (terminalId) {
        setManualControlAt(Date.now());
        await client.interruptTerminal(terminalId, { reason: 'project_run_restart' });
        setActiveTerminalBusy(false);
      }
      await dispatchProjectRunTarget(selectedRunTarget, t('runSettings.restartSuccess'), terminalId || null);
    } catch (error) {
      setRunnerError(extractProjectRunnerValidationMessage(error, error instanceof Error ? error.message : t('runSettings.restartFailed'), t));
      setRunnerMessage(null);
    } finally {
      setRestarting(false);
    }
  }, [
    activeRun?.terminalId,
    client,
    dispatchProjectRunTarget,
    project?.id,
    selectedRunTarget,
    selectedTerminalId,
    setActiveTerminalBusy,
    setManualControlAt,
    t,
  ]);

  const handleRunnerDelete = useCallback(async () => {
    setDeleting(true);
    setRunnerError(null);
    try {
      const terminalId = buildProjectRunnerSelectedTerminalId(selectedTerminalId || activeRun?.terminalId);
      if (!terminalId) {
        throw new Error(t('runSettings.error.noDedicatedTerminal'));
      }
      const nextTerminalId = resolveProjectRunnerDeleteTarget(projectRunTerminalIds, terminalId);
      await client.deleteTerminal(terminalId);
      removeRunInstanceLocally(terminalId, nextTerminalId);
      selectRunInstance(nextTerminalId);
      if (activeRun?.terminalId === terminalId) {
        setActiveRun(null);
      }
      setLastExitedRun((value) => (value?.terminalId === terminalId ? null : value));
      setActiveTerminalBusy(false);
      setRunnerMessage(nextTerminalId ? t('runSettings.deleteSwitched') : t('runSettings.deleteRemoved'));
      setRunnerDiagnosis(null);
      await refreshProjectActiveRun();
    } catch (error) {
      setRunnerError(error instanceof Error ? error.message : t('runSettings.deleteFailed'));
      setRunnerMessage(null);
    } finally {
      setDeleting(false);
    }
  }, [
    activeRun,
    client,
    projectRunTerminalIds,
    removeRunInstanceLocally,
    refreshProjectActiveRun,
    selectRunInstance,
    selectedTerminalId,
    setActiveRun,
    setLastExitedRun,
    setActiveTerminalBusy,
    t,
  ]);

  const resetRunnerCommandState = useCallback(() => {
    setRunnerMessage(null);
    setRunnerError(null);
    setRunnerDiagnosis(null);
    setManualControlAt(0);
    setLastExitCheckedRunKey('');
  }, []);

  return {
    starting,
    stopping,
    restarting,
    deleting,
    runnerMessage,
    runnerError,
    runnerDiagnosis,
    manualControlAt,
    lastExitCheckedRunKey,
    setLastExitCheckedRunKey,
    setRunnerDiagnosis,
    setRunnerError,
    setRunnerMessage,
    resetRunnerCommandState,
    handleRunnerStart,
    handleRunnerStop,
    handleRunnerRestart,
    handleRunnerDelete,
  };
};
