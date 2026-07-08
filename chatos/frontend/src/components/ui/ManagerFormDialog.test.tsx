// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import ManagerFormDialog from './ManagerFormDialog';

describe('ManagerFormDialog', () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    document.body.style.overflow = '';
  });

  it('closes on escape and restores body scrolling after close', () => {
    const onClose = vi.fn();
    const { rerender } = render(
      <ManagerFormDialog open title="编辑表单" onClose={onClose}>
        <div>表单内容</div>
      </ManagerFormDialog>,
    );

    expect(screen.getByRole('dialog', { name: '编辑表单' })).toBeInTheDocument();
    expect(document.body.style.overflow).toBe('hidden');

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);

    rerender(
      <ManagerFormDialog open={false} title="编辑表单" onClose={onClose}>
        <div>表单内容</div>
      </ManagerFormDialog>,
    );

    expect(document.body.style.overflow).toBe('');
  });

  it('closes when clicking the backdrop', () => {
    const onClose = vi.fn();
    render(
      <ManagerFormDialog open title="新增表单" onClose={onClose}>
        <button type="button">内部按钮</button>
      </ManagerFormDialog>,
    );

    fireEvent.click(screen.getByRole('dialog').parentElement as HTMLElement);
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
