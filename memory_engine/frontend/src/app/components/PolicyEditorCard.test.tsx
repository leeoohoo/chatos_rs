import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { PIPELINE_POLICY_META } from '../constants';
import { PolicyEditorCard } from './PolicyEditorCard';

const basePolicy = {
  job_type: 'summary',
  enabled: true,
  model_profile_id: 'model-1',
  summary_prompt: 'initial summary prompt',
  summary_prompt_zh: 'initial summary prompt',
  summary_prompt_en: 'initial english prompt',
  summary_prompt_language: 'zh',
  rollup_summary_prompt: null,
  rollup_summary_prompt_zh: null,
  rollup_summary_prompt_en: null,
  rollup_summary_prompt_language: 'zh',
  token_limit: 1000,
  target_summary_tokens: 500,
  interval_seconds: 30,
  max_threads_per_tick: 5,
  count_limit: null,
  keep_level0_count: null,
  max_level: null,
  updated_at: '2026-05-20T00:00:00Z',
} as const;

describe('PolicyEditorCard', () => {
  it('preserves unsaved edits across unrelated rerenders', async () => {
    const onSave = vi.fn();
    const onGeneratePrompt = vi.fn();
    const { rerender } = render(
      <PolicyEditorCard
        policy={basePolicy}
        meta={PIPELINE_POLICY_META.summary}
        viewKey="summary"
        modelOptions={[{ label: 'Model 1', value: 'model-1' }]}
        saving={false}
        generatingPrompt={false}
        onSave={onSave}
        onGeneratePrompt={onGeneratePrompt}
      />,
    );

    const textarea = await screen.findByPlaceholderText('输入或生成中文总结提示词');

    fireEvent.change(textarea, { target: { value: 'draft prompt from user' } });
    expect((textarea as HTMLTextAreaElement).value).toBe('draft prompt from user');

    rerender(
      <PolicyEditorCard
        policy={basePolicy}
        meta={PIPELINE_POLICY_META.summary}
        viewKey="summary"
        modelOptions={[
          { label: 'Model 1', value: 'model-1' },
          { label: 'Model 2', value: 'model-2' },
        ]}
        saving={false}
        generatingPrompt={false}
        onSave={onSave}
        onGeneratePrompt={onGeneratePrompt}
      />,
    );

    expect(
      (screen.getByPlaceholderText('输入或生成中文总结提示词') as HTMLTextAreaElement).value,
    ).toBe('draft prompt from user');
  });

  it('resets form values when a newer policy payload arrives', async () => {
    const onSave = vi.fn();
    const onGeneratePrompt = vi.fn();
    const { rerender } = render(
      <PolicyEditorCard
        policy={basePolicy}
        meta={PIPELINE_POLICY_META.summary}
        viewKey="summary"
        modelOptions={[{ label: 'Model 1', value: 'model-1' }]}
        saving={false}
        generatingPrompt={false}
        onSave={onSave}
        onGeneratePrompt={onGeneratePrompt}
      />,
    );

    const textarea = await screen.findByPlaceholderText('输入或生成中文总结提示词');

    fireEvent.change(textarea, { target: { value: 'draft prompt from user' } });
    expect((textarea as HTMLTextAreaElement).value).toBe('draft prompt from user');

    rerender(
      <PolicyEditorCard
        policy={{
          ...basePolicy,
          summary_prompt: 'server refreshed prompt',
          summary_prompt_zh: 'server refreshed prompt',
          updated_at: '2026-05-21T00:00:00Z',
        }}
        meta={PIPELINE_POLICY_META.summary}
        viewKey="summary"
        modelOptions={[{ label: 'Model 1', value: 'model-1' }]}
        saving={false}
        generatingPrompt={false}
        onSave={onSave}
        onGeneratePrompt={onGeneratePrompt}
      />,
    );

    expect(
      (screen.getByPlaceholderText('输入或生成中文总结提示词') as HTMLTextAreaElement).value,
    ).toBe('server refreshed prompt');
  });
});
