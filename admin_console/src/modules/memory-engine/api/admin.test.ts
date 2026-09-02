// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { client } from './client';
import { adminApi } from './admin';

vi.mock('./client', () => ({
  client: {
    get: vi.fn(),
    post: vi.fn(),
    put: vi.fn(),
    delete: vi.fn(),
  },
}));

describe('adminApi', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('encodes dynamic policy identifiers in admin paths', async () => {
    vi.mocked(client.put).mockResolvedValue({ data: {} } as never);
    await adminApi.updateJobPolicy('summary/rollup', {});

    expect(client.put).toHaveBeenCalledWith(
      '/admin/job-policies/summary%2Frollup',
      {},
    );
  });

  it('normalizes source and policy responses with stable defaults', async () => {
    vi.mocked(client.get)
      .mockResolvedValueOnce({
        data: {
          items: [
            {
              id: 'src-1',
              source_id: 'source-a',
              source_type: 'sdk_system',
              name: 'Source A',
              created_at: '2026-05-21T00:00:00Z',
              updated_at: '2026-05-21T00:00:00Z',
            },
          ],
        },
      } as never)
      .mockResolvedValueOnce({
        data: {
          items: [
            {
              job_type: 'thread_summary',
              updated_at: '2026-05-21T00:00:00Z',
            },
          ],
        },
      } as never);

    await expect(adminApi.listSources()).resolves.toEqual([
      {
        id: 'src-1',
        tenant_id: null,
        source_id: 'source-a',
        source_type: 'sdk_system',
        name: 'Source A',
        description: null,
        config: null,
        status: 'active',
        sdk_enabled: false,
        secret_key_hint: null,
        key_last_rotated_at: null,
        created_at: '2026-05-21T00:00:00Z',
        updated_at: '2026-05-21T00:00:00Z',
      },
    ]);

    await expect(adminApi.listJobPolicies()).resolves.toEqual([
      {
        job_type: 'thread_summary',
        enabled: true,
        summary_prompt: null,
        summary_prompt_zh: null,
        summary_prompt_en: null,
        summary_prompt_language: 'zh',
        rollup_summary_prompt: null,
        rollup_summary_prompt_zh: null,
        rollup_summary_prompt_en: null,
        rollup_summary_prompt_language: 'zh',
        token_limit: null,
        target_summary_tokens: null,
        interval_seconds: null,
        max_threads_per_tick: null,
        count_limit: null,
        keep_level0_count: null,
        max_level: null,
        updated_at: '2026-05-21T00:00:00Z',
      },
    ]);
  });
});
