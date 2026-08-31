// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { PIPELINE_POLICY_META } from '../constants';
import { PolicyEditorCard } from './PolicyEditorCard';

const basePolicy = {
  job_type: 'summary',
  enabled: true,
  model_profile_id: null,
  summary_prompt: null,
  summary_prompt_zh: null,
  summary_prompt_en: null,
  summary_prompt_language: 'zh',
  rollup_summary_prompt: null,
  rollup_summary_prompt_zh: null,
  rollup_summary_prompt_en: null,
  rollup_summary_prompt_language: 'zh',
  token_limit: 6000,
  target_summary_tokens: 700,
  interval_seconds: 60,
  max_threads_per_tick: 10,
  count_limit: null,
  keep_level0_count: null,
  max_level: null,
  updated_at: '2026-07-17T00:00:00Z',
} as const;

describe('PolicyEditorCard', () => {
  it('renders configuration-center managed values without a local save form', () => {
    render(
      <PolicyEditorCard
        policy={basePolicy}
        meta={PIPELINE_POLICY_META.summary}
      />,
    );

    expect(screen.getByText('运行参数已由全局配置中心统一管理')).toBeTruthy();
    expect(screen.getByText('6000')).toBeTruthy();
    expect(screen.getByText('700')).toBeTruthy();
    expect(screen.getByRole('link', { name: '打开配置中心' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: '保存' })).toBeNull();
    expect(screen.queryByRole('textbox')).toBeNull();
  });

  it('keeps Prompt ownership in Plugin Management', () => {
    render(
      <PolicyEditorCard
        policy={basePolicy}
        meta={PIPELINE_POLICY_META.summary}
      />,
    );

    expect(screen.getByText('消息总结 Prompt 已由系统 Agent 统一管理')).toBeTruthy();
    expect(screen.getByText(/memory_engine_summary_agent/)).toBeTruthy();
  });
});
