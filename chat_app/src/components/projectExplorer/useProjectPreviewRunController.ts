import { useMemo, useState } from 'react';

import { useProjectPreviewExitInspection } from './previewRunController/useProjectPreviewExitInspection';
import { useProjectPreviewRunnerCommands } from './previewRunController/useProjectPreviewRunnerCommands';
import { useProjectPreviewTerminalPolling } from './previewRunController/useProjectPreviewTerminalPolling';
import type {
  ProjectPreviewActiveRunState,
  UseProjectPreviewRunControllerParams,
} from './previewRunController/previewRunControllerTypes';

export const useProjectPreviewRunController = ({
  projectId,
  projectRootPath,
  runCwd,
  runTargets,
  selectedRunTargetId,
  onRunCommand,
  onInterruptTerminal,
  onListTerminalLogs,
  onListTerminals,
}: UseProjectPreviewRunControllerParams) => {
  const [runCommand, setRunCommand] = useState('');
  const [runMessage, setRunMessage] = useState<string | null>(null);
  const [runError, setRunError] = useState<string | null>(null);
  const [activeRun, setActiveRun] = useState<ProjectPreviewActiveRunState | null>(null);
  const [activeTerminalBusy, setActiveTerminalBusy] = useState(false);
  const [manualControlAt, setManualControlAt] = useState(0);
  const [lastExitCheckedRunKey, setLastExitCheckedRunKey] = useState('');

  const selectedRunTarget = useMemo(
    () => runTargets.find((item) => item.id === selectedRunTargetId) || null,
    [runTargets, selectedRunTargetId]
  );

  const runTargetCwd = useMemo(
    () => (selectedRunTarget?.cwd || runCwd || projectRootPath || '').trim(),
    [projectRootPath, runCwd, selectedRunTarget?.cwd]
  );

  const currentCommand = useMemo(
    () => (runCommand.trim() || selectedRunTarget?.command || '').trim(),
    [runCommand, selectedRunTarget?.command]
  );

  const runnerCommands = useProjectPreviewRunnerCommands({
    activeRun,
    activeTerminalBusy,
    currentCommand,
    runTargetCwd,
    onRunCommand,
    onInterruptTerminal,
    setActiveRun,
    setActiveTerminalBusy,
    setLastExitCheckedRunKey,
    setManualControlAt,
    setRunError,
    setRunMessage,
  });

  useProjectPreviewTerminalPolling({
    activeRunTerminalId: activeRun?.terminalId || null,
    currentCommand,
    projectId,
    projectRootPath,
    runTargetCwd,
    selectedRunTargetCommand: selectedRunTarget?.command,
    onListTerminals,
    setActiveRun,
    setActiveTerminalBusy,
  });

  useProjectPreviewExitInspection({
    activeRun,
    activeTerminalBusy,
    lastExitCheckedRunKey,
    manualControlAt,
    onListTerminalLogs,
    setLastExitCheckedRunKey,
    setRunError,
    setRunMessage,
  });

  return {
    activeRun,
    activeTerminalBusy,
    currentCommand,
    restarting: runnerCommands.restarting,
    runCommand,
    runError,
    runMessage,
    runTargetCwd,
    running: runnerCommands.running,
    selectedRunTarget,
    setRunCommand,
    stopping: runnerCommands.stopping,
    handleRestart: runnerCommands.handleRestart,
    handleRun: runnerCommands.handleRun,
    handleStop: runnerCommands.handleStop,
  };
};
