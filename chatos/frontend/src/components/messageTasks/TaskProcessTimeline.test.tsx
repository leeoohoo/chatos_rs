// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import {
  buildTaskProcessTimelineItems,
  parseTaskProcessLog,
  TaskProcessTimeline,
} from './TaskProcessTimeline';

describe('TaskProcessTimeline', () => {
  afterEach(cleanup);

  it('parses timestamped process entries into ordered timeline nodes', () => {
    const entries = parseTaskProcessLog([
      '[2026-08-06T09:44:30.216806+00:00] 开始执行',
      '读取项目配置并确认现有脚本。',
      '',
      '[2026-08-06T09:44:53.226931+00:00] 实施方案',
      '补齐 lint、测试和质量验证。',
    ].join('\n'));

    expect(entries).toEqual([
      {
        title: '开始执行',
        description: '读取项目配置并确认现有脚本。',
        occurredAt: '2026-08-06T09:44:30.216806+00:00',
      },
      {
        title: '实施方案',
        description: '补齐 lint、测试和质量验证。',
        occurredAt: '2026-08-06T09:44:53.226931+00:00',
      },
    ]);
  });

  it('combines task and process-task notes without showing raw log syntax', () => {
    const items = buildTaskProcessTimelineItems(
      '[2026-08-06T09:44:30Z] 开始执行\n读取项目结构。',
      [{
        id: 'process-1',
        title: '补齐质量脚本',
        status: 'doing',
        process_log: '确认现有 build 脚本后增加测试配置。',
        updated_at: '2026-08-06T09:45:00Z',
      }],
      'running',
    );

    render(<TaskProcessTimeline items={items} />);

    expect(screen.getByText('执行时间线')).toBeInTheDocument();
    expect(screen.getByText('2 个节点')).toBeInTheDocument();
    expect(screen.getByText('开始执行')).toBeInTheDocument();
    expect(screen.getByText('补齐质量脚本')).toBeInTheDocument();
    expect(screen.getByText('确认现有 build 脚本后增加测试配置。')).toBeInTheDocument();
    expect(screen.queryByText(/\[2026-08-06T09:44:30Z\]/)).not.toBeInTheDocument();
  });

  it('shows a dedicated empty state', () => {
    render(<TaskProcessTimeline items={[]} />);

    expect(screen.getByText('暂无执行过程')).toBeInTheDocument();
    expect(screen.getByText('任务写入关键执行节点后会按时间展示在这里。')).toBeInTheDocument();
  });
});
