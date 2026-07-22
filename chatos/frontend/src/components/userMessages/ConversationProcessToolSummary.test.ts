// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { buildToolActionSummary, toolActionText } from './ConversationProcessToolSummary';

describe('ConversationProcessToolSummary', () => {
  it('summarizes read, search, execute and modify tools without exposing tool names', () => {
    expect(toolActionText(buildToolActionSummary(
      'code_maintainer_read_read_file_raw',
      { path: 'src/model.ts' },
    ), 'completed')).toBe('已读取 src/model.ts');
    expect(toolActionText(buildToolActionSummary(
      'code_maintainer_read_search_text',
      { path: 'src', pattern: 'completed' },
    ), 'completed')).toBe('已在 src 中搜索「completed」');
    expect(toolActionText(buildToolActionSummary(
      'code_maintainer_read_search_text',
      { path: 'src', pattern: 'completed' },
    ), 'pending')).toBe('正在 src 中搜索「completed」');
    expect(toolActionText(buildToolActionSummary(
      'terminal_controller_execute_command',
      { command: 'npm test -- --run' },
    ), 'completed')).toBe('已执行 npm test -- --run');
    expect(toolActionText(buildToolActionSummary(
      'code_maintainer_write_edit_file',
      { path: 'src/model.ts' },
    ), 'completed')).toBe('已修改 src/model.ts');
  });

  it('summarizes multi-file patches and status variants', () => {
    const summary = buildToolActionSummary('apply_patch', {
      patch: [
        '*** Begin Patch',
        '*** Update File: src/a.ts',
        '*** Add File: src/b.ts',
        '*** End Patch',
      ].join('\n'),
    });
    expect(toolActionText(summary, 'completed')).toBe('已修改 2 个文件');
    expect(toolActionText(summary, 'pending')).toBe('正在修改 2 个文件');
    expect(toolActionText(summary, 'error')).toBe('修改 2 个文件 失败');
  });

  it('keeps remote context in user-facing read summaries', () => {
    expect(toolActionText(buildToolActionSummary(
      'remote_connection_controller_read_file',
      { path: '/srv/app/config.toml' },
    ), 'completed')).toBe('已读取远端文件 /srv/app/config.toml');
  });

  it('covers known remote, browser, notepad and agent actions', () => {
    expect(toolActionText(buildToolActionSummary(
      'remote_connection_controller_test_connection',
      { connection_name: '生产机' },
    ), 'completed')).toBe('已测试远端连接 生产机');
    expect(toolActionText(buildToolActionSummary(
      'browser_tools_browser_press',
      { key: 'Enter' },
    ), 'completed')).toBe('已按下 Enter');
    expect(toolActionText(buildToolActionSummary(
      'notepad_create_folder',
      { name: '调研' },
    ), 'completed')).toBe('已创建笔记文件夹「调研」');
    expect(toolActionText(buildToolActionSummary(
      'agent_builder_list_available_skills',
      {},
    ), 'completed')).toBe('已读取可用技能列表');
  });
});
