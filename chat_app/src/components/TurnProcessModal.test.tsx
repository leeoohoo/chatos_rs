// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type ApiClient from '../lib/api/client';
import type { Message } from '../types';
import TurnProcessModal from './TurnProcessModal';

vi.mock('./LazyMarkdownRenderer', () => ({
  LazyMarkdownRenderer: ({ content }: { content: string }) => (
    <div data-testid="lazy-markdown">{content}</div>
  ),
}));

vi.mock('./ToolCallRenderer', () => ({
  ToolCallRenderer: ({ toolCall }: { toolCall: { name: string } }) => (
    <div data-testid="tool-call-renderer">{toolCall.name}</div>
  ),
}));

vi.mock('./turnProcessViewer/useTurnProcessViewerModel', () => ({
  useTurnProcessViewerModel: vi.fn(),
}));

import { useTurnProcessViewerModel } from './turnProcessViewer/useTurnProcessViewerModel';

const mockedUseTurnProcessViewerModel = vi.mocked(useTurnProcessViewerModel);

const buildUserMessage = (overrides: Partial<Message> = {}): Message => ({
  id: 'user-1',
  sessionId: 'session-1',
  role: 'user',
  content: '帮我看一下发布流程',
  status: 'completed',
  createdAt: new Date('2026-05-18T10:00:00.000Z'),
  metadata: {},
  ...overrides,
});

const apiClient = {} as ApiClient;

describe('TurnProcessModal', () => {
  const scrollToMock = vi.fn();

  beforeEach(() => {
    mockedUseTurnProcessViewerModel.mockReturnValue({
      userMessage: buildUserMessage(),
      resolvedTurnId: 'turn-1',
      finalAssistantMessage: null,
      processMessages: [],
      timelineItems: [
        {
          id: 'thinking-1',
          kind: 'thinking',
          createdAt: new Date('2026-05-18T10:00:01.000Z'),
          text: '先整理执行步骤',
          isStreaming: true,
        },
        {
          id: 'tool-1',
          kind: 'tool_call',
          createdAt: new Date('2026-05-18T10:00:02.000Z'),
          toolCall: {
            id: 'tool-1',
            messageId: 'assistant-1',
            name: 'run_command',
            arguments: { cmd: 'pwd' },
            createdAt: new Date('2026-05-18T10:00:02.000Z'),
            result: undefined,
          },
          streamLog: '/workspace',
          completed: false,
        },
      ],
      stats: {
        toolCount: 1,
        thinkingCount: 1,
        unavailableCount: 0,
        processMessageCount: 1,
      },
      loading: false,
      error: null,
      isStreaming: true,
    });

    scrollToMock.mockReset();
    Object.defineProperty(HTMLElement.prototype, 'scrollTo', {
      configurable: true,
      value: scrollToMock,
    });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    document.body.style.overflow = '';
  });

  it('renders as a dialog with streaming timeline content and focuses close button', () => {
    render(
      <TurnProcessModal
        open
        sessionId="session-1"
        userMessageId="user-1"
        turnId="turn-1"
        messages={[buildUserMessage()]}
        apiClient={apiClient}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByRole('dialog', { name: '过程详情' })).toBeInTheDocument();
    expect(screen.getAllByText('执行中').length).toBeGreaterThan(0);
    expect(screen.getByText('工具：1')).toBeInTheDocument();
    expect(screen.getByText('思考：1')).toBeInTheDocument();
    expect(screen.getByText('思考')).toBeInTheDocument();
    expect(screen.getByText('工具 · run_command')).toBeInTheDocument();
    expect(screen.getAllByText('进行中').length).toBeGreaterThan(0);
    expect(screen.getByText('实时跟随中')).toBeInTheDocument();
    expect(screen.getByText('新的步骤会自动滚动到视野内。')).toBeInTheDocument();
    expect(screen.getByText('总步骤')).toBeInTheDocument();
    expect(screen.getByText('最新活跃')).toBeInTheDocument();
    expect(screen.getByText('仍在进行中，但不是最新更新的步骤')).toBeInTheDocument();
    expect(screen.getByText('当前活跃步骤')).toBeInTheDocument();
    expect(screen.getByText('实时输出')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '关闭' })).toHaveFocus();
    expect(document.body.style.overflow).toBe('hidden');
    expect(scrollToMock).toHaveBeenCalled();
  });

  it('closes on Escape and overlay click', () => {
    const onClose = vi.fn();

    render(
      <TurnProcessModal
        open
        sessionId="session-1"
        userMessageId="user-1"
        turnId="turn-1"
        messages={[buildUserMessage()]}
        apiClient={apiClient}
        onClose={onClose}
      />,
    );

    fireEvent.keyDown(window, { key: 'Escape' });
    fireEvent.click(screen.getByTestId('turn-process-modal-overlay'));

    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it('does not render when closed', () => {
    render(
      <TurnProcessModal
        open={false}
        sessionId="session-1"
        userMessageId="user-1"
        turnId="turn-1"
        messages={[buildUserMessage()]}
        apiClient={apiClient}
        onClose={vi.fn()}
      />,
    );

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('hides stream log once a tool call is effectively completed', () => {
    mockedUseTurnProcessViewerModel.mockReturnValue({
      userMessage: buildUserMessage(),
      resolvedTurnId: 'turn-1',
      finalAssistantMessage: null,
      processMessages: [],
      timelineItems: [
        {
          id: 'tool-completed-1',
          kind: 'tool_call',
          createdAt: new Date('2026-05-18T10:00:03.000Z'),
          toolCall: {
            id: 'tool-completed-1',
            messageId: 'assistant-1',
            name: 'run_command',
            arguments: { cmd: 'pwd' },
            createdAt: new Date('2026-05-18T10:00:03.000Z'),
            result: '/workspace/result',
          },
          streamLog: '/workspace',
          completed: false,
        },
      ],
      stats: {
        toolCount: 1,
        thinkingCount: 0,
        unavailableCount: 0,
        processMessageCount: 1,
      },
      loading: false,
      error: null,
      isStreaming: true,
    });

    render(
      <TurnProcessModal
        open
        sessionId="session-1"
        userMessageId="user-1"
        turnId="turn-1"
        messages={[buildUserMessage()]}
        apiClient={apiClient}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getAllByText('已完成').length).toBeGreaterThan(0);
    expect(screen.queryByText('实时输出')).not.toBeInTheDocument();
  });

  it('stops auto-follow when user scrolls away from bottom and can jump back to latest', () => {
    render(
      <TurnProcessModal
        open
        sessionId="session-1"
        userMessageId="user-1"
        turnId="turn-1"
        messages={[buildUserMessage()]}
        apiClient={apiClient}
        onClose={vi.fn()}
      />,
    );

    const scrollContainer = screen.getByTestId('turn-process-modal-scroll');
    Object.defineProperty(scrollContainer, 'scrollHeight', {
      configurable: true,
      value: 1200,
    });
    Object.defineProperty(scrollContainer, 'clientHeight', {
      configurable: true,
      value: 400,
    });
    Object.defineProperty(scrollContainer, 'scrollTop', {
      configurable: true,
      writable: true,
      value: 200,
    });

    fireEvent.scroll(scrollContainer);

    expect(screen.getByText('已暂停跟随')).toBeInTheDocument();
    expect(screen.getByText('有新的过程步骤')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '回到最新' })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '回到最新' }));

    expect(scrollToMock).toHaveBeenCalledWith({
      top: 1200,
      behavior: 'smooth',
    });
  });

  it('keeps following when streaming content grows without adding a new timeline item', () => {
    const { rerender } = render(
      <TurnProcessModal
        open
        sessionId="session-1"
        userMessageId="user-1"
        turnId="turn-1"
        messages={[buildUserMessage()]}
        apiClient={apiClient}
        onClose={vi.fn()}
      />,
    );

    scrollToMock.mockClear();

    mockedUseTurnProcessViewerModel.mockReturnValue({
      userMessage: buildUserMessage(),
      resolvedTurnId: 'turn-1',
      finalAssistantMessage: null,
      processMessages: [],
      timelineItems: [
        {
          id: 'thinking-1',
          kind: 'thinking',
          createdAt: new Date('2026-05-18T10:00:01.000Z'),
          text: '先整理执行步骤',
          isStreaming: true,
        },
        {
          id: 'tool-1',
          kind: 'tool_call',
          createdAt: new Date('2026-05-18T10:00:02.000Z'),
          toolCall: {
            id: 'tool-1',
            messageId: 'assistant-1',
            name: 'run_command',
            arguments: { cmd: 'pwd' },
            createdAt: new Date('2026-05-18T10:00:02.000Z'),
            result: undefined,
          },
          streamLog: '/workspace\nnext-line',
          completed: false,
        },
      ],
      stats: {
        toolCount: 1,
        thinkingCount: 1,
        unavailableCount: 0,
        processMessageCount: 1,
      },
      loading: false,
      error: null,
      isStreaming: true,
    });

    rerender(
      <TurnProcessModal
        open
        sessionId="session-1"
        userMessageId="user-1"
        turnId="turn-1"
        messages={[buildUserMessage()]}
        apiClient={apiClient}
        onClose={vi.fn()}
      />,
    );

    expect(scrollToMock).toHaveBeenCalled();
  });

  it('counts pending updates while follow mode is paused', () => {
    const { rerender } = render(
      <TurnProcessModal
        open
        sessionId="session-1"
        userMessageId="user-1"
        turnId="turn-1"
        messages={[buildUserMessage()]}
        apiClient={apiClient}
        onClose={vi.fn()}
      />,
    );

    const scrollContainer = screen.getByTestId('turn-process-modal-scroll');
    Object.defineProperty(scrollContainer, 'scrollHeight', {
      configurable: true,
      value: 1200,
    });
    Object.defineProperty(scrollContainer, 'clientHeight', {
      configurable: true,
      value: 400,
    });
    Object.defineProperty(scrollContainer, 'scrollTop', {
      configurable: true,
      writable: true,
      value: 200,
    });

    fireEvent.scroll(scrollContainer);

    mockedUseTurnProcessViewerModel.mockReturnValue({
      userMessage: buildUserMessage(),
      resolvedTurnId: 'turn-1',
      finalAssistantMessage: null,
      processMessages: [],
      timelineItems: [
        {
          id: 'thinking-1',
          kind: 'thinking',
          createdAt: new Date('2026-05-18T10:00:01.000Z'),
          text: '先整理执行步骤',
          isStreaming: true,
        },
        {
          id: 'tool-1',
          kind: 'tool_call',
          createdAt: new Date('2026-05-18T10:00:02.000Z'),
          toolCall: {
            id: 'tool-1',
            messageId: 'assistant-1',
            name: 'run_command',
            arguments: { cmd: 'pwd' },
            createdAt: new Date('2026-05-18T10:00:02.000Z'),
            result: undefined,
          },
          streamLog: '/workspace\nanother-line',
          completed: false,
        },
      ],
      stats: {
        toolCount: 1,
        thinkingCount: 1,
        unavailableCount: 0,
        processMessageCount: 1,
      },
      loading: false,
      error: null,
      isStreaming: true,
    });

    rerender(
      <TurnProcessModal
        open
        sessionId="session-1"
        userMessageId="user-1"
        turnId="turn-1"
        messages={[buildUserMessage()]}
        apiClient={apiClient}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('有 1 次新更新')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '回到最新' }));

    expect(screen.queryByText('有 1 次新更新')).not.toBeInTheDocument();
  });
});
