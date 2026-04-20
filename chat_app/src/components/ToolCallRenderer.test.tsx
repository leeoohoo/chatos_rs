// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Message, ToolCall } from '../types';
import ToolCallRenderer from './ToolCallRenderer';

vi.mock('./LazyMarkdownRenderer', () => ({
  LazyMarkdownRenderer: ({ content }: { content: string }) => (
    <div data-testid="lazy-markdown">{content}</div>
  ),
}));

const buildToolCall = (overrides: Partial<ToolCall> = {}): ToolCall => ({
  id: 'tool_1',
  messageId: 'msg_1',
  name: 'web_extract',
  arguments: { url: 'https://example.com' },
  result: {},
  createdAt: new Date('2026-04-15T10:00:00Z'),
  ...overrides,
});

const buildToolResultMessage = (overrides: Partial<Message> = {}): Message => ({
  id: 'tool_msg_1',
  sessionId: 'session_1',
  role: 'tool',
  content: 'summary only',
  status: 'completed',
  createdAt: new Date('2026-04-15T10:00:01Z'),
  metadata: {},
  ...overrides,
});

describe('ToolCallRenderer summaries', () => {
  afterEach(() => {
    cleanup();
  });

  it('renders extract summary while hiding backend execution metadata', () => {
    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          result: {
            backend: 'jina',
            fallback_used: true,
            provider_attempts: [{ provider: 'jina' }, { provider: 'scrape' }],
            extract_summary: {
              page_count: 3,
              truncated_page_count: 1,
              total_omitted_chars: 5000,
            },
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    expect(screen.queryByText('Web backend')).not.toBeInTheDocument();

    const extractCard = screen
      .getByText('Extract summary')
      .closest('.tool-summary-card') as HTMLElement;
    expect(extractCard).toBeInTheDocument();
    expect(within(extractCard).getByText('3')).toBeInTheDocument();
    expect(within(extractCard).getByText('1')).toBeInTheDocument();
    expect(within(extractCard).queryByText('5000')).not.toBeInTheDocument();
  });

  it('shortens code maintainer tool names and renders file details without tree tables', () => {
    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'code_maintainer_read_read_file_range',
          arguments: {
            path: 'src/main.rs',
            start_line: 10,
            end_line: 20,
          },
          result: {
            path: 'src/main.rs',
            size_bytes: 128,
            sha256: 'abc123',
            start_line: 10,
            end_line: 20,
            total_lines: 80,
            content: '10: fn main() {\n11:   println!("hello");\n12: }',
          },
        })}
      />,
    );

    expect(screen.getByText('@read_file_range')).toBeInTheDocument();
    expect(screen.getByTitle('code_maintainer_read_read_file_range')).toBeInTheDocument();
    expect(screen.queryByText('@code_maintainer_read_read_file_range')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    expect(screen.getByText('Input summary')).toBeInTheDocument();
    expect(screen.getAllByText('src/main.rs').length).toBeGreaterThan(0);
    expect(screen.queryByText('File summary')).not.toBeInTheDocument();
    expect(screen.getByText('File content')).toBeInTheDocument();
    const fileContentCard = screen
      .getByText('File content')
      .closest('.tool-detail-card') as HTMLElement;
    expect(fileContentCard).toBeInTheDocument();
    expect(within(fileContentCard).getByText('10-20')).toBeInTheDocument();
    const codeBlock = fileContentCard.querySelector('.tool-detail-code');
    expect(codeBlock).toHaveTextContent('println!("hello");');
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
    expect(screen.queryByText('值')).not.toBeInTheDocument();
  });

  it('hides redundant search summary and keeps only the match list for search tools', () => {
    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'code_maintainer_read_search_text',
          arguments: {
            path: 'src',
            pattern: 'TODO',
            max_results: 200,
          },
          result: {
            count: 2,
            results: [
              { path: 'src/a.ts', line: 3, text: 'TODO first' },
              { path: 'src/b.ts', line: 8, text: 'TODO second' },
            ],
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    expect(screen.getByText('@search_text')).toBeInTheDocument();
    expect(screen.getByText('Input summary')).toBeInTheDocument();
    expect(screen.queryByText('max results')).not.toBeInTheDocument();
    expect(screen.queryByText('Search summary')).not.toBeInTheDocument();
    expect(screen.getByText('Matches')).toBeInTheDocument();
    expect(screen.getByText('src/a.ts')).toBeInTheDocument();
    expect(screen.getByText('TODO second')).toBeInTheDocument();
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
  });

  it('normalizes builtin list_dir names and renders directory entries cards', () => {
    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'code_maintainer_read_builtin__list_dir',
          arguments: {
            path: 'src',
            max_entries: 20,
          },
          result: {
            entries: [
              { name: 'components', path: 'src/components', type: 'dir' },
              { name: 'main.tsx', path: 'src/main.tsx', type: 'file', size: 1024 },
            ],
          },
        })}
      />,
    );

    expect(screen.getByText('@list_dir')).toBeInTheDocument();
    expect(screen.queryByText('@builtin__list_dir')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    expect(screen.getByText('Directory entries')).toBeInTheDocument();
    expect(screen.getByText('components')).toBeInTheDocument();
    expect(screen.getByText('main.tsx')).toBeInTheDocument();
    expect(screen.queryByText('max entries')).not.toBeInTheDocument();
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
  });

  it('normalizes builtin apply_patch names and avoids tree-table fallback', () => {
    const patchText = '*** Begin Patch\n*** Update File: src/a.ts\n@@\n-old\n+new\n*** End Patch';

    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'code_maintainer_write_builtin__apply_patch',
          arguments: {
            patch: patchText,
          },
          result: {
            result: {
              updated: ['src/a.ts'],
              added: [],
              deleted: [],
            },
            files: [
              { path: 'src/a.ts', sha256: 'abc123' },
            ],
            message: 'Applied patch successfully.',
          },
        })}
      />,
    );

    expect(screen.getByText('@apply_patch')).toBeInTheDocument();
    expect(screen.queryByText('@builtin__apply_patch')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    expect(screen.queryByText('Patch')).not.toBeInTheDocument();
    expect(screen.getByText('Patch payload')).toBeInTheDocument();
    const patchCard = screen.getByText('Patch payload').closest('.tool-detail-card') as HTMLElement;
    const patchContent = patchCard.querySelector('pre');
    expect(patchContent).not.toBeNull();
    expect(patchContent).toHaveTextContent(/\*\*\* Begin Patch[\s\S]*\*\*\* End Patch/);
    expect(screen.queryByText('Patch summary')).not.toBeInTheDocument();
    expect(screen.queryByText('Change summary')).not.toBeInTheDocument();
    expect(screen.queryByText('Touched files')).not.toBeInTheDocument();
    expect(screen.queryByText('abc123')).not.toBeInTheDocument();
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
  });

  it('prefers structured finalResult JSON strings for read_file_raw cards', () => {
    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'code_maintainer_read_read_file_raw',
          arguments: {
            path: 'src/app.ts',
          },
          result: 'streaming placeholder',
          finalResult: JSON.stringify({
            path: 'src/app.ts',
            size_bytes: 320,
            sha256: 'hash',
            line_count: 3,
            content: 'export const a = 1;\nexport const b = 2;',
          }),
        } as Partial<ToolCall> & { finalResult?: string })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    expect(screen.getByText('@read_file_raw')).toBeInTheDocument();
    expect(screen.getByText('内置面板')).toBeInTheDocument();
    expect(screen.queryByText('通用面板')).not.toBeInTheDocument();
    const fileContentCard = screen
      .getByText('File content')
      .closest('.tool-detail-card') as HTMLElement;
    expect(fileContentCard).toBeInTheDocument();
    expect(fileContentCard).toHaveTextContent('export const a = 1;');
    expect(fileContentCard).toHaveTextContent('export const b = 2;');
    expect(screen.queryByText('streaming placeholder')).not.toBeInTheDocument();
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
  });

  it('recovers truncated read_file_raw JSON-ish text into builtin file cards', () => {
    const truncatedResult = [
      'Recovered tool result snapshot:',
      '{',
      '  "path": "src/app.ts",',
      '  "size_bytes": 320,',
      '  "sha256": "hash",',
      '  "line_count": 3,',
      '  "content": "export const a = 1;\\nexport const b = 2;"',
    ].join('\n');

    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'code_maintainer_read_read_file_raw',
          arguments: {
            path: 'src/app.ts',
          },
          result: 'streaming placeholder',
          finalResult: truncatedResult,
        } as Partial<ToolCall> & { finalResult?: string })}
      />,
    );

    expect(screen.getByText('内置面板')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    const fileContentCard = screen
      .getByText('File content')
      .closest('.tool-detail-card') as HTMLElement;
    expect(fileContentCard).toBeInTheDocument();
    expect(fileContentCard).toHaveTextContent('export const a = 1;');
    expect(fileContentCard).toHaveTextContent('export const b = 2;');
    expect(screen.queryByText('Recovered tool result snapshot:')).not.toBeInTheDocument();
    expect(screen.queryByText('streaming placeholder')).not.toBeInTheDocument();
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
  });

  it('normalizes builtin terminal tool names and renders command cards', () => {
    render(
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

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    expect(screen.getByText('Command status')).toBeInTheDocument();
    expect(screen.getByText('Output')).toBeInTheDocument();
    expect(screen.getByText('Tests passed')).toBeInTheDocument();
    expect(screen.queryByText('terminal-1')).not.toBeInTheDocument();
    expect(screen.queryByText('process-1')).not.toBeInTheDocument();
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
  });

  it('routes remote read_file to remote cards instead of code maintainer cards', () => {
    render(
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
    expect(screen.getByText('远程连接')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    expect(screen.getByText('Remote file')).toBeInTheDocument();
    expect(screen.getByText('Remote file content')).toBeInTheDocument();
    expect(screen.getByText('{"env":"prod"}')).toBeInTheDocument();
    expect(screen.queryByText('File content')).not.toBeInTheDocument();
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
  });

  it('renders task manager cards for task lists', () => {
    render(
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

    expect(screen.getByText('任务管理')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    expect(screen.getByText('Task scope')).toBeInTheDocument();
    expect(screen.getByText('Tasks')).toBeInTheDocument();
    expect(screen.getByText('Refine tool cards')).toBeInTheDocument();
    expect(screen.getByText('Remove noisy metadata · #frontend #ux')).toBeInTheDocument();
  });

  it('renders agent builder recommendation cards', () => {
    render(
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

    expect(screen.getByText('智能体构建')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    expect(screen.getByText('Recommended profile')).toBeInTheDocument();
    expect(screen.getByText('Role definition')).toBeInTheDocument();
    expect(screen.getByText('你是研发协作助手，负责拆解修复任务。')).toBeInTheDocument();
    expect(screen.getByText('Suggested skills')).toBeInTheDocument();
    expect(screen.getByText('code_review')).toBeInTheDocument();
  });

  it('renders memory reader cards for skill details', () => {
    render(
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

    expect(screen.getByText('记忆读取')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

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
    render(
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

    expect(screen.getByText('笔记工具')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    const readyCard = screen.getByText('Notepad ready').closest('.tool-detail-card') as HTMLElement;
    expect(readyCard).toBeInTheDocument();
    expect(within(readyCard).getByText('yes')).toBeInTheDocument();
    expect(within(readyCard).getByText('12')).toBeInTheDocument();
    expect(within(readyCard).getByText('1')).toBeInTheDocument();
    expect(screen.queryByText('/tmp/chatos/notepad')).not.toBeInTheDocument();
    expect(screen.queryByText('/tmp/chatos/notepad/notes')).not.toBeInTheDocument();
    expect(screen.queryByText('/tmp/chatos/notepad/notes-index.json')).not.toBeInTheDocument();
  });

  it('renders ui prompt cards with separated form values and chosen options', () => {
    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'builtin_ui_prompter_prompt_mixed_form',
          arguments: {
            title: 'Confirm deploy',
          },
          result: {
            status: 'submitted',
            values: {
              reason: 'Need one more review',
              urgent: true,
            },
            selection: ['deploy', 'notify'],
          },
        })}
      />,
    );

    expect(screen.getByText('交互确认')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    expect(screen.getByText('Mixed form result')).toBeInTheDocument();
    const formValuesCard = screen.getByText('Form values').closest('.tool-detail-card') as HTMLElement;
    expect(formValuesCard).toBeInTheDocument();
    expect(within(formValuesCard).getByText('reason')).toBeInTheDocument();
    expect(within(formValuesCard).getByText('Need one more review')).toBeInTheDocument();
    expect(within(formValuesCard).getByText('urgent')).toBeInTheDocument();
    expect(within(formValuesCard).getByText('yes')).toBeInTheDocument();
    expect(screen.getByText('Selection')).toBeInTheDocument();
    expect(screen.getByText('deploy')).toBeInTheDocument();
    expect(screen.getByText('notify')).toBeInTheDocument();
    expect(screen.queryByText('Process summary')).not.toBeInTheDocument();
  });

  it('renders remote connectivity summary without exposing connection ids', () => {
    render(
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

    expect(screen.getByText('远程连接')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

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
    render(
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

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

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

  it('renders console summary card with message counters', () => {
    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_console',
          result: {
            total_messages: 4,
            total_errors: 1,
            clear_applied: true,
            message_count_by_type: {
              log: 2,
              warn: 1,
              error: 1,
            },
            messages_brief: [
              { type: 'warn', text_preview: 'Deprecated API usage' },
            ],
            errors_brief: [
              { message_preview: 'Uncaught TypeError' },
            ],
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    const consoleCard = screen
      .getByText('Console summary')
      .closest('.tool-summary-card') as HTMLElement;
    expect(consoleCard).toBeInTheDocument();
    expect(within(consoleCard).getByText('4')).toBeInTheDocument();
    expect(within(consoleCard).getByText('1')).toBeInTheDocument();
    expect(within(consoleCard).queryByText('clear applied')).not.toBeInTheDocument();
    expect(within(consoleCard).queryByText('warn')).not.toBeInTheDocument();
    expect(screen.getByText('Console messages')).toBeInTheDocument();
    expect(screen.getByText('JavaScript errors')).toBeInTheDocument();
  });

  it('renders vision analysis without exposing transport metadata', () => {
    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_vision',
          result: {
            analysis: 'The page shows a pricing table.',
            vision: {
              enabled: true,
              mode: 'user_model',
              prompt_source: 'contact_agent',
              provider: 'gpt',
              model: 'gpt-4o',
              transport: 'chat_completions',
              fallback_used: true,
              transport_fallback_used: true,
            },
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    expect(screen.queryByText('Vision summary')).not.toBeInTheDocument();
    expect(screen.getByText('Vision analysis')).toBeInTheDocument();
    expect(screen.getByText('The page shows a pricing table.')).toBeInTheDocument();
    expect(screen.queryByText('chat_completions')).not.toBeInTheDocument();
  });

  it('renders research summary card from nested research payload', () => {
    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'web_research',
          result: {
            _summary_text: 'Research bundle completed.',
            research_findings: {
              answer_frame: 'Web research found enough signal to compare the target topic across sources.',
              web_findings: [
                'Search returned 5 result(s).',
                'Extraction reviewed 3 selected URL(s) and returned 3 page(s).',
              ],
              source_highlights: [
                {
                  kind: 'extract',
                  title: 'Competitor pricing breakdown',
                  url: 'https://example.com/competitor-pricing',
                  status: 'ok',
                  note: 'Includes concrete plan names and monthly pricing.',
                },
              ],
              recommended_next_steps: [
                'Open the strongest source and compare its claims against the current product positioning.',
              ],
            },
            research_summary: {
              search_backend: 'chatos_native_search',
              extract_backend: 'chatos_native_extract',
              search_result_count: 5,
              extracted_page_count: 3,
              selected_url_count: 3,
              total_omitted_chars: 1200,
              warning: 'extract fallback used',
            },
            search: {
              backend: 'chatos_native_search',
              result_count: 5,
            },
            extract: {
              backend: 'chatos_native_extract',
              extract_summary: {
                page_count: 3,
                total_omitted_chars: 1200,
              },
            },
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    const findingsCard = screen
      .getByText('Research findings')
      .closest('.tool-summary-card') as HTMLElement;
    expect(findingsCard).toBeInTheDocument();
    expect(within(findingsCard).getByText('Web research found enough signal to compare the target topic across sources.')).toBeInTheDocument();
    expect(within(findingsCard).getByText('Competitor pricing breakdown')).toBeInTheDocument();
    expect(within(findingsCard).getByText('Open the strongest source and compare its claims against the current product positioning.')).toBeInTheDocument();

    const researchCard = screen
      .getByText('Research overview')
      .closest('.tool-summary-card') as HTMLElement;
    expect(researchCard).toBeInTheDocument();
    expect(within(researchCard).getByText('5')).toBeInTheDocument();
    expect(within(researchCard).getAllByText('3').length).toBeGreaterThan(0);
    expect(within(researchCard).getByText('extract fallback used')).toBeInTheDocument();
    expect(within(researchCard).queryByText('firecrawl')).not.toBeInTheDocument();
    expect(within(researchCard).queryByText('direct_http')).not.toBeInTheDocument();
  });

  it('renders inspect summary card for browser_inspect results', () => {
    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_inspect',
          result: {
            _summary_text: 'Observed the current browser page.',
            inspection_mode: 'read_only_observe',
            title: 'Pricing',
            url: 'https://example.com/pricing',
            element_count: 18,
            inspection_steps: {
              snapshot: 'ok',
              console: 'ok',
              vision: 'skipped',
            },
            total_messages: 4,
            total_errors: 1,
            page_state_available: true,
            inspection_warning: 'console: one warning was captured',
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    const inspectCard = screen
      .getByText('Current page')
      .closest('.tool-summary-card') as HTMLElement;
    expect(inspectCard).toBeInTheDocument();
    expect(within(inspectCard).getByText('Pricing [https://example.com/pricing]')).toBeInTheDocument();
    expect(within(inspectCard).getByText('4')).toBeInTheDocument();
    expect(within(inspectCard).getByText('1')).toBeInTheDocument();
    expect(within(inspectCard).getByText('console: one warning was captured')).toBeInTheDocument();
    expect(within(inspectCard).queryByText('read_only_observe')).not.toBeInTheDocument();
  });

  it('renders browser_inspect blank page state without surfacing about blank as an active page', () => {
    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_inspect',
          result: {
            _summary_text: 'No active browser page was available.',
            success: false,
            inspection_mode: 'read_only_observe',
            url: 'about:blank',
            inspection_steps: {
              snapshot: 'ok',
              console: 'ok',
              vision: 'skipped',
            },
            total_messages: 0,
            total_errors: 0,
            page_state_available: false,
            inspection_warning: 'page: no active browser page was available; open a page before running browser_inspect',
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    const inspectCard = screen
      .getByText('Current page')
      .closest('.tool-summary-card') as HTMLElement;
    expect(inspectCard).toBeInTheDocument();
    expect(within(inspectCard).getByText('未打开页面')).toBeInTheDocument();
    expect(within(inspectCard).getByText('page: no active browser page was available; open a page before running browser_inspect')).toBeInTheDocument();
    expect(within(inspectCard).queryByText('about:blank')).not.toBeInTheDocument();
    expect(screen.getByText('Inspection warning')).toBeInTheDocument();
    expect(screen.queryByText('Vision analysis')).not.toBeInTheDocument();
  });

  it('renders inspect and research summaries for browser_research results', () => {
    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_research',
          result: {
            _summary_text: 'Researched the current browser page.',
            page: {
              inspection_mode: 'read_only_observe',
              title: 'Docs',
              url: 'https://example.com/docs',
              element_count: 12,
              snapshot: 'VERY_LONG_SNAPSHOT_BLOCK',
              inspection_steps: {
                snapshot: 'ok',
                console: 'ok',
                vision: 'ok',
              },
              total_messages: 2,
              total_errors: 0,
              page_state_available: true,
              console_messages: [
                { text: 'RAW_CONSOLE_ENTRY' },
              ],
              js_errors: [
                { message: 'RAW_JS_ERROR_ENTRY' },
              ],
            },
            selected_urls: [
              'https://example.com/release-notes',
              'https://example.com/extra-source',
            ],
            research_findings: {
              answer_frame: 'Combined page and web research completed for "What changed?".',
              page_findings: [
                'Current page: Docs [https://example.com/docs].',
                'Inspection steps finished with snapshot=ok, console=ok, vision=ok.',
              ],
              web_findings: [
                'External search for "docs changelog" returned 4 result(s).',
              ],
              source_highlights: [
                {
                  kind: 'extract',
                  title: 'Release notes',
                  url: 'https://example.com/release-notes',
                  status: 'ok',
                  note: 'Highlights API and UI changes from the last release.',
                },
              ],
              recommended_next_steps: [
                'Open the release notes source and compare it against the current page.',
              ],
            },
            research_summary: {
              search_backend: 'chatos_native_search',
              extract_backend: 'chatos_native_extract',
              search_result_count: 4,
              extracted_page_count: 2,
              selected_url_count: 2,
              total_omitted_chars: 900,
              warning: 'web_extract: fallback used',
            },
            search: {
              backend: 'chatos_native_search',
              result_count: 4,
              provider_attempts: [],
              data: {
                web: [
                  {
                    title: 'RAW_SEARCH_HIT',
                  },
                ],
              },
              results_brief: [
                {
                  title: 'Release notes brief',
                  url: 'https://example.com/release-notes',
                  description_preview: 'Visible brief summary',
                },
              ],
            },
            extract: {
              backend: 'chatos_native_extract',
              provider_attempts: [],
              extract_summary: {
                page_count: 2,
                total_omitted_chars: 900,
              },
              results_brief: [
                {
                  title: 'Release notes extract',
                  url: 'https://example.com/release-notes',
                  status: 'ok',
                  content_preview: 'Visible extract summary',
                },
              ],
              results: [
                {
                  content: 'VERY_LONG_RAW_SOURCE_TEXT',
                },
              ],
            },
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    const findingsCard = screen
      .getByText('Research findings')
      .closest('.tool-summary-card') as HTMLElement;
    expect(findingsCard).toBeInTheDocument();
    expect(within(findingsCard).getByText('Current page: Docs [https://example.com/docs].')).toBeInTheDocument();
    expect(within(findingsCard).getByText('Release notes')).toBeInTheDocument();

    const inspectCard = screen
      .getByText('Current page')
      .closest('.tool-summary-card') as HTMLElement;
    expect(inspectCard).toBeInTheDocument();
    expect(within(inspectCard).getByText('Docs [https://example.com/docs]')).toBeInTheDocument();

    const researchCard = screen
      .getByText('Research overview')
      .closest('.tool-summary-card') as HTMLElement;
    expect(researchCard).toBeInTheDocument();
    expect(within(researchCard).getByText('4')).toBeInTheDocument();
    expect(within(researchCard).getAllByText('2').length).toBeGreaterThan(0);
    expect(within(researchCard).queryByText('firecrawl')).not.toBeInTheDocument();
    expect(within(researchCard).queryByText('direct_http')).not.toBeInTheDocument();
    expect(screen.getByText('Selected URLs')).toBeInTheDocument();
    expect(screen.getByText('https://example.com/extra-source')).toBeInTheDocument();
    expect(screen.getByText('Search hits')).toBeInTheDocument();
    expect(screen.getByText('Release notes brief')).toBeInTheDocument();
    expect(screen.getByText('Extracted sources')).toBeInTheDocument();
    expect(screen.getByText('Release notes extract')).toBeInTheDocument();
    expect(screen.queryByText('VERY_LONG_SNAPSHOT_BLOCK')).not.toBeInTheDocument();
    expect(screen.queryByText('RAW_CONSOLE_ENTRY')).not.toBeInTheDocument();
    expect(screen.queryByText('RAW_JS_ERROR_ENTRY')).not.toBeInTheDocument();
    expect(screen.queryByText('RAW_SEARCH_HIT')).not.toBeInTheDocument();
    expect(screen.queryByText('VERY_LONG_RAW_SOURCE_TEXT')).not.toBeInTheDocument();
    expect(screen.queryByText('provider_attempts')).not.toBeInTheDocument();
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
  });

  it('prefers structured_result from tool message metadata and renders summary text', () => {
    const toolResultById = new Map<string, Message>([
      ['tool_1', buildToolResultMessage({
        metadata: {
          structured_result: {
            _summary_text: 'Loaded page summary',
            backend: 'chatos_native_extract',
            extract_summary: {
              page_count: 1,
              truncated_page_count: 0,
              total_omitted_chars: 0,
            },
          },
        },
      })],
    ]);

    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          result: undefined,
          finalResult: undefined,
        } as Partial<ToolCall> & { finalResult?: string })}
        toolResultById={toolResultById}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    expect(screen.getAllByText('Loaded page summary')).toHaveLength(1);
    expect(screen.queryByText('_summary_text')).not.toBeInTheDocument();
    expect(screen.queryByText('Web backend')).not.toBeInTheDocument();
    const extractCard = screen
      .getByText('Extract summary')
      .closest('.tool-summary-card') as HTMLElement;
    expect(extractCard).toBeInTheDocument();
    expect(within(extractCard).getByText('1')).toBeInTheDocument();
    expect(within(extractCard).getByText('0')).toBeInTheDocument();
  });
});
