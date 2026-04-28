// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import PromptDialog from './PromptDialog';

const buildProps = (overrides: Partial<ComponentProps<typeof PromptDialog>> = {}) => ({
  isOpen: true,
  title: '新建目录',
  message: '请输入新目录名称',
  value: '',
  onValueChange: vi.fn(),
  onConfirm: vi.fn(),
  onCancel: vi.fn(),
  ...overrides,
});

describe('PromptDialog', () => {
  afterEach(() => {
    cleanup();
  });

  it('renders description when provided', () => {
    render(
      <PromptDialog
        {...buildProps({
          message: 'message text',
          description: 'description text',
        })}
      />,
    );

    expect(screen.getByText('description text')).toBeInTheDocument();
    expect(screen.queryByText('message text')).not.toBeInTheDocument();
  });

  it('calls onValueChange when input changes', () => {
    const onValueChange = vi.fn();
    render(<PromptDialog {...buildProps({ onValueChange })} />);

    fireEvent.change(screen.getByRole('textbox'), {
      target: { value: 'notes' },
    });

    expect(onValueChange).toHaveBeenCalledWith('notes');
  });

  it('renders validation error text', () => {
    render(<PromptDialog {...buildProps({ error: '名称不能为空' })} />);

    expect(screen.getByText('名称不能为空')).toBeInTheDocument();
  });

  it('submits on Enter key', () => {
    const onConfirm = vi.fn();
    render(<PromptDialog {...buildProps({ onConfirm })} />);

    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Enter' });

    expect(onConfirm).toHaveBeenCalledTimes(1);
  });
});
