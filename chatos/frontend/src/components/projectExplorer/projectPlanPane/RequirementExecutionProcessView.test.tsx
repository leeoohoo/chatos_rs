// @vitest-environment jsdom
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { RequirementExecutionProcessActions } from './RequirementExecutionProcessView';

afterEach(() => cleanup());

const noop = vi.fn();

describe('requirement execution process actions', () => {
  it('keeps rerun disabled and visibly loading while old batch cancellation is settling', () => {
    render(
      <RequirementExecutionProcessActions
        actuallyStarted
        canRegenerate={false}
        canRerun
        cancellationSettling
        confirming={false}
        executionPaused={false}
        graphReady={false}
        hasActiveRuns
        onCancelRequirementExecution={noop}
        onClose={noop}
        onConfirmExecution={noop}
        onOpenCancelConfirm={noop}
        onOpenDiscardConfirm={noop}
        onOpenFailedTaskRetry={noop}
        onOpenRerunConfirm={noop}
        onRegenerate={noop}
        onTogglePause={noop}
        pausing={false}
        phase="stopped"
        queuedTaskCount={1}
        rerunSettling
        rerunning={false}
        retryableFailedTaskCount={0}
        retryingTaskId={null}
        revising={false}
        runningTaskCount={1}
        stopping={false}
      />,
    );

    const rerunButton = screen.getByRole('button', { name: '等待取消完成' });
    expect((rerunButton as HTMLButtonElement).disabled).toBe(true);
    expect(rerunButton.getAttribute('aria-busy')).toBe('true');
  });
});
