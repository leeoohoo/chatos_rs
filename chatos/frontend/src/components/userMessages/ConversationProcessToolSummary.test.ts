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
      'code_maintainer_write_stage_edit_batch',
      {
        session_id: 'session-1',
        operations: [{ kind: 'replace_text', path: 'src/model.ts' }],
      },
    ), 'completed')).toBe('已暂存修改 src/model.ts');
    expect(toolActionText(buildToolActionSummary(
      'code_maintainer_write_commit_edit_session',
      { session_id: 'session-1' },
    ), 'completed')).toBe('已提交项目修改');
  });

  it('keeps remote context in user-facing read summaries', () => {
    expect(toolActionText(buildToolActionSummary(
      'remote_connection_controller_read_file',
      { path: '/srv/app/config.toml' },
    ), 'completed')).toBe('已读取远端文件 /srv/app/config.toml');
  });

});
