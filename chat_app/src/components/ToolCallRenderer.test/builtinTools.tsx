import { fireEvent, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import {
  buildToolCall,
  renderWithEnglishI18n,
  ToolCallRenderer,
} from './helpers';

describe('ToolCallRenderer builtin tool cards', () => {
  it('normalizes builtin terminal tool names and renders command cards', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'builtin_terminal_controller_execute_command',
          arguments: {
            path: 'chat_app',
            command: 'npm run test',
            background: false,
          },
          result: {
            path: '/workspace/chat_app',
            background: false,
            busy: false,
            terminal_reused: true,
            finished_by: 'idle',
            output: 'Tests passed',
            terminal_id: 'terminal-1',
            process_id: 'process-1',
          },
        })}
      />,
    );

    expect(screen.getByText('@execute_command')).toBeInTheDocument();
    expect(screen.queryByText('@builtin_terminal_controller_execute_command')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    expect(screen.getByText('Command status')).toBeInTheDocument();
    expect(screen.getByText('Output')).toBeInTheDocument();
    expect(screen.getByText('Tests passed')).toBeInTheDocument();
    expect(screen.queryByText('terminal-1')).not.toBeInTheDocument();
    expect(screen.queryByText('process-1')).not.toBeInTheDocument();
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
  });

  it('routes remote read_file to remote cards instead of code maintainer cards', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'builtin_remote_connection_controller_read_file',
          arguments: {
            path: '/srv/app/config.json',
            max_bytes: 4096,
          },
          result: {
            connection_id: 'conn_1',
            path: '/srv/app/config.json',
            truncated: false,
            source_size_bytes: 128,
            content: '{\"env\":\"prod\"}',
          },
        })}
      />,
    );

    expect(screen.getByText('@read_file')).toBeInTheDocument();
    expect(screen.getByText('Remote')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    expect(screen.getByText('Remote file')).toBeInTheDocument();
    expect(screen.getByText('Remote file content')).toBeInTheDocument();
    expect(screen.getByText('{"env":"prod"}')).toBeInTheDocument();
    expect(screen.queryByText('File content')).not.toBeInTheDocument();
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
  });

  it('renders task manager cards for task lists', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'builtin_task_manager_list_tasks',
          arguments: {
            include_done: false,
            limit: 20,
          },
          result: {
            count: 2,
            tasks: [
              {
                id: 't1',
                title: 'Refine tool cards',
                details: 'Remove noisy metadata',
                priority: 'high',
                status: 'doing',
                tags: ['frontend', 'ux'],
              },
              {
                id: 't2',
                title: 'Add remote cards',
                details: '',
                priority: 'medium',
                status: 'todo',
                tags: ['frontend'],
              },
            ],
          },
        })}
      />,
    );

    expect(screen.getByText('Tasks')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    expect(screen.getByText('Task scope')).toBeInTheDocument();
    expect(screen.getAllByText('Tasks').length).toBeGreaterThan(0);
    expect(screen.getByText('Refine tool cards')).toBeInTheDocument();
    expect(screen.getByText('Remove noisy metadata · #frontend #ux')).toBeInTheDocument();
  });

  it('renders agent builder recommendation cards', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'builtin_agent_builder_recommend_agent_profile',
          arguments: {
            requirement: '帮我做代码评审和 bug 修复',
          },
          result: {
            name: '研发协作助手',
            category: 'engineering',
            description: '建议用于研发协作。',
            role_definition: '你是研发协作助手，负责拆解修复任务。',
            suggested_skill_ids: ['code_review', 'bug_fix'],
          },
        })}
      />,
    );

    expect(screen.getByText('Agent builder')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    expect(screen.getByText('Recommended profile')).toBeInTheDocument();
    expect(screen.getByText('Role definition')).toBeInTheDocument();
    expect(screen.getByText('你是研发协作助手，负责拆解修复任务。')).toBeInTheDocument();
    expect(screen.getByText('Suggested skills')).toBeInTheDocument();
    expect(screen.getByText('code_review')).toBeInTheDocument();
  });

  it('renders memory reader cards for skill details', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'builtin_memory_skill_reader_get_skill_detail',
          arguments: {
            skill_ref: 'SK1',
          },
          result: {
            skill_ref: 'SK1',
            name: 'Tool Card Review',
            source_type: 'skill_center',
            plugin_source: 'local/plugin',
            source_path: 'skills/tool-card.md',
            content: '# Tool card\nReview all tool cards.',
          },
        })}
      />,
    );

    expect(screen.getByText('Memory')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    expect(screen.getByText('Skill')).toBeInTheDocument();
    expect(screen.getByText('Skill content')).toBeInTheDocument();
    const skillContentCard = screen
      .getByText('Skill content')
      .closest('.tool-detail-card') as HTMLElement;
    const skillContent = skillContentCard.querySelector('pre');
    expect(skillContent).not.toBeNull();
    expect(skillContent).toHaveTextContent(/# Tool card\s+Review all tool cards\./);
  });

  it('renders notepad init card without raw storage paths', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'builtin_notepad_init',
          arguments: {},
          result: {
            ok: true,
            data_dir: '/tmp/chatos/notepad',
            notes_root: '/tmp/chatos/notepad/notes',
            index_path: '/tmp/chatos/notepad/notes-index.json',
            version: 1,
            notes: 12,
          },
        })}
      />,
    );

    expect(screen.getByText('Notepad')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    const readyCard = screen.getByText('Notepad ready').closest('.tool-detail-card') as HTMLElement;
    expect(readyCard).toBeInTheDocument();
    expect(within(readyCard).getByText('yes')).toBeInTheDocument();
    expect(within(readyCard).getByText('12')).toBeInTheDocument();
    expect(within(readyCard).getByText('1')).toBeInTheDocument();
    expect(screen.queryByText('/tmp/chatos/notepad')).not.toBeInTheDocument();
    expect(screen.queryByText('/tmp/chatos/notepad/notes')).not.toBeInTheDocument();
    expect(screen.queryByText('/tmp/chatos/notepad/notes-index.json')).not.toBeInTheDocument();
  });

  it('renders remote connectivity summary without exposing connection ids', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'builtin_remote_connection_controller_test_connection',
          arguments: {},
          result: {
            connection_id: 'conn_prod_1',
            name: 'production',
            host: '10.0.0.2',
            port: 22,
            username: 'deploy',
            result: {
              success: true,
              remote_host: 'prod-app-01',
              connected_at: '2026-04-15T10:00:00Z',
            },
          },
        })}
      />,
    );

    expect(screen.getByText('Remote')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    const connectionResultCard = screen
      .getByText('Connection result')
      .closest('.tool-detail-card') as HTMLElement;
    expect(connectionResultCard).toBeInTheDocument();
    expect(within(connectionResultCard).getByText('yes')).toBeInTheDocument();
    expect(within(connectionResultCard).getByText('prod-app-01')).toBeInTheDocument();
    expect(within(connectionResultCard).getByText('2026-04-15T10:00:00Z')).toBeInTheDocument();
    expect(screen.queryByText('conn_prod_1')).not.toBeInTheDocument();
  });

  it('renders process summary card with extended state fields', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'process',
          result: {
            wait_status: 'completed',
            terminal_id: 'terminal-123',
            process_id: 'process-123',
            busy: false,
            completed: true,
            timed_out: false,
            processes: [{ id: 'p1' }, { id: 'p2' }],
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    const processCard = screen
      .getAllByText('Process summary')[0]
      .closest('.tool-summary-card') as HTMLElement;
    expect(processCard).toBeInTheDocument();
    const statusRow = within(processCard)
      .getByText('status')
      .closest('.tool-summary-row') as HTMLElement;
    expect(statusRow).toBeInTheDocument();
    expect(within(statusRow).getByText('completed')).toBeInTheDocument();
    expect(within(processCard).queryByText('terminal-123')).not.toBeInTheDocument();
    expect(within(processCard).queryByText('process-123')).not.toBeInTheDocument();
    expect(within(processCard).getByText('no')).toBeInTheDocument();
    expect(within(processCard).getByText('2')).toBeInTheDocument();
  });
});
