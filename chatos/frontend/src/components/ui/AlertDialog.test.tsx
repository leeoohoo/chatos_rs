// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import AlertDialog from './AlertDialog';

const buildProps = (overrides: Partial<ComponentProps<typeof AlertDialog>> = {}) => ({
  isOpen: true,
  title: '请选择模型',
  message: '请先选择一个模型',
  onConfirm: vi.fn(),
  ...overrides,
});

describe('AlertDialog', () => {
  afterEach(() => {
    cleanup();
  });

  it('renders description over message when provided', () => {
    render(
      <AlertDialog
        {...buildProps({
          message: 'message text',
          description: 'description text',
        })}
      />,
    );

    expect(screen.getByText('description text')).toBeInTheDocument();
    expect(screen.queryByText('message text')).not.toBeInTheDocument();
  });

  it('resolves through confirm action', () => {
    const onConfirm = vi.fn();
    render(<AlertDialog {...buildProps({ onConfirm, confirmText: '知道了' })} />);

    fireEvent.click(screen.getByRole('button', { name: '知道了' }));

    expect(onConfirm).toHaveBeenCalledTimes(1);
  });
});
