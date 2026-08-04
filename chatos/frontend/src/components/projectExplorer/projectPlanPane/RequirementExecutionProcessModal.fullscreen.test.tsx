// @vitest-environment jsdom
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  RequirementExecutionStartingModal,
  requirementExecutionModalShellClassName,
} from './RequirementExecutionProcessModal';

afterEach(() => cleanup());

describe('requirement execution modal fullscreen mode', () => {
  it('uses a viewport-filling shell only in fullscreen mode', () => {
    expect(requirementExecutionModalShellClassName(false)).toContain('max-w-[1500px]');
    expect(requirementExecutionModalShellClassName(false)).toContain('h-[94dvh]');
    expect(requirementExecutionModalShellClassName(true)).toContain('inset-0');
    expect(requirementExecutionModalShellClassName(true)).toContain('h-[100dvh]');
    expect(requirementExecutionModalShellClassName(true)).toContain('max-w-none');
  });

  it('toggles between normal and fullscreen layouts from the header', async () => {
    const user = userEvent.setup();
    render(
      <RequirementExecutionStartingModal
        requirement={{ id: 'requirement-1', title: 'Requirement 1' }}
        executionPlane="cloud"
        starting={false}
        onClose={vi.fn()}
        onStart={vi.fn()}
      />,
    );

    const dialog = screen.getByRole('dialog', { name: '执行计划工作台' });
    const shell = dialog.querySelector('section');
    expect(shell?.getAttribute('data-fullscreen')).toBe('false');

    await user.click(screen.getByRole('button', { name: '全屏' }));
    expect(shell?.getAttribute('data-fullscreen')).toBe('true');
    expect(
      screen.getByRole('button', { name: '退出全屏' }).getAttribute('aria-pressed'),
    ).toBe('true');

    await user.click(screen.getByRole('button', { name: '退出全屏' }));
    expect(shell?.getAttribute('data-fullscreen')).toBe('false');
  });

  it('waits for an explicit click before starting the planning agent', async () => {
    const user = userEvent.setup();
    const onStart = vi.fn();
    render(
      <RequirementExecutionStartingModal
        requirement={{ id: 'requirement-1', title: 'Requirement 1' }}
        executionPlane="cloud"
        starting={false}
        onClose={vi.fn()}
        onStart={onStart}
      />,
    );

    expect(screen.getByText('等待开始生成执行计划')).toBeTruthy();
    expect(onStart).not.toHaveBeenCalled();

    await user.type(
      screen.getByPlaceholderText('输入希望执行计划遵循的要求，例如：先补测试，再拆分接口；把部署放到最后……'),
      '先补测试，再拆分接口',
    );
    await user.click(screen.getByRole('button', { name: '开始生成执行计划' }));

    expect(onStart).toHaveBeenCalledWith('先补测试，再拆分接口');
  });
});
