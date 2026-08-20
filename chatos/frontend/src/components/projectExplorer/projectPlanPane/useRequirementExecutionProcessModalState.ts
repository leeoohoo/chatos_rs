// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useRef, useState } from 'react';

import type { Message } from '../../../types';
import {
  createFallbackMessage,
  isStoppedExecutionStatus,
  withProcessStatus,
} from './requirementExecutionPhase';
import type { RequirementExecutionProcess } from './requirementExecutionProcessModel';

export function useRequirementExecutionProcessModalState(
  process: RequirementExecutionProcess,
) {
  const [liveProcess, setLiveProcess] = useState(process);
  const [message, setMessage] = useState<Message>(
    withProcessStatus(process.initialMessage || createFallbackMessage(process), process),
  );
  const [feedback, setFeedback] = useState('');
  const [syncError, setSyncError] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [pausing, setPausing] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [revising, setRevising] = useState(false);
  const [rerunning, setRerunning] = useState(false);
  const [rerunCancellationSettling, setRerunCancellationSettling] = useState(false);
  const [rerunConfirmOpen, setRerunConfirmOpen] = useState(false);
  const [failedTaskRetryOpen, setFailedTaskRetryOpen] = useState(false);
  const [discardConfirmOpen, setDiscardConfirmOpen] = useState(false);
  const [cancelConfirmOpen, setCancelConfirmOpen] = useState(false);
  const [planStopped, setPlanStopped] = useState(
    isStoppedExecutionStatus(process.serverStatus),
  );
  const [planDiscarded, setPlanDiscarded] = useState(Boolean(process.tasksDiscarded));
  const [executionConfirmed, setExecutionConfirmed] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionMessage, setActionMessage] = useState<string | null>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const [panelWidth, setPanelWidth] = useState(900);
  const graphContainerRef = useRef<HTMLDivElement>(null);
  const pollingRef = useRef(false);
  const rerunningRef = useRef(false);
  const activeExecutionGroupIdRef = useRef(process.executionGroupId);

  useEffect(() => {
    const stopped = isStoppedExecutionStatus(process.serverStatus);
    activeExecutionGroupIdRef.current = process.executionGroupId;
    setLiveProcess(process);
    setMessage(withProcessStatus(
      process.initialMessage || createFallbackMessage(process),
      process,
    ));
    setFeedback('');
    setPlanStopped(stopped);
    setPlanDiscarded(Boolean(process.tasksDiscarded));
    setExecutionConfirmed(Boolean(process.hasStartedRuns));
    setActionError(null);
    setActionMessage(null);
    setSyncError(null);
    setRerunCancellationSettling(false);
    setRerunConfirmOpen(false);
    setFailedTaskRetryOpen(false);
    setDiscardConfirmOpen(false);
    setCancelConfirmOpen(false);
  }, [process.executionGroupId]);

  return {
    actionError,
    actionMessage,
    activeExecutionGroupIdRef,
    cancelConfirmOpen,
    confirming,
    discardConfirmOpen,
    executionConfirmed,
    failedTaskRetryOpen,
    feedback,
    fullscreen,
    graphContainerRef,
    liveProcess,
    message,
    panelWidth,
    pausing,
    planDiscarded,
    planStopped,
    pollingRef,
    rerunCancellationSettling,
    rerunConfirmOpen,
    rerunning,
    rerunningRef,
    revising,
    setActionError,
    setActionMessage,
    setCancelConfirmOpen,
    setConfirming,
    setDiscardConfirmOpen,
    setExecutionConfirmed,
    setFailedTaskRetryOpen,
    setFeedback,
    setFullscreen,
    setLiveProcess,
    setMessage,
    setPanelWidth,
    setPausing,
    setPlanDiscarded,
    setPlanStopped,
    setRerunCancellationSettling,
    setRerunConfirmOpen,
    setRerunning,
    setRevising,
    setStopping,
    setSyncError,
    setSyncing,
    stopping,
    syncError,
    syncing,
  };
}
