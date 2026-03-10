// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import ConfirmDialog from './ConfirmDialog';

const buildProps = (overrides: Partial<ComponentProps<typeof ConfirmDialog>> = {}) => ({
  isOpen: true,
  title: '删除会话',
  message: '确认删除当前会话？',
  onConfirm: vi.fn(),
  onCancel: vi.fn(),
  ...overrides,
});

describe('ConfirmDialog', () => {
  afterEach(() => {
    cleanup();
  });

  it('prefers detailsLines over details', () => {
    render(
      <ConfirmDialog
        {...buildProps({
          details: '单段详情文本',
          detailsLines: [' 第一行 ', '', '第二行'],
        })}
      />,
    );

    expect(screen.getByText('第一行')).toBeInTheDocument();
    expect(screen.getByText('第二行')).toBeInTheDocument();
    expect(screen.queryByText('单段详情文本')).not.toBeInTheDocument();
  });

  it('uses default detailsTitle when details exist', () => {
    render(<ConfirmDialog {...buildProps({ details: '请先结束传输任务' })} />);

    expect(screen.getByText('详情/建议操作')).toBeInTheDocument();
    expect(screen.getByText('请先结束传输任务')).toBeInTheDocument();
  });

  it('renders custom detailsTitle when provided', () => {
    render(
      <ConfirmDialog
        {...buildProps({
          details: '请检查密钥文件权限',
          detailsTitle: '建议操作',
        })}
      />,
    );

    expect(screen.getByText('建议操作')).toBeInTheDocument();
  });

  it('falls back to message when description is missing', () => {
    const { rerender } = render(
      <ConfirmDialog {...buildProps({ message: '只显示 message 文案' })} />,
    );
    expect(screen.getByText('只显示 message 文案')).toBeInTheDocument();

    rerender(
      <ConfirmDialog
        {...buildProps({
          message: 'message 会被 description 覆盖',
          description: '优先显示 description 文案',
        })}
      />,
    );
    expect(screen.getByText('优先显示 description 文案')).toBeInTheDocument();
    expect(
      screen.queryByText('message 会被 description 覆盖'),
    ).not.toBeInTheDocument();
  });

  it('does not render details block when details and detailsLines are empty', () => {
    render(
      <ConfirmDialog
        {...buildProps({
          details: '   ',
          detailsLines: ['', '   '],
        })}
      />,
    );

    expect(screen.queryByText('详情/建议操作')).not.toBeInTheDocument();
  });
});
