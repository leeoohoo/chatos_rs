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

  it('normalizes model profiles with backend-compatible defaults', async () => {
    vi.mocked(client.get).mockResolvedValueOnce({
      data: {
        items: [
          {
            id: 'model-1',
            name: 'Model A',
            provider: 'openai',
            model: 'gpt-test',
            created_at: '2026-05-21T00:00:00Z',
            updated_at: '2026-05-21T00:00:00Z',
          },
        ],
      },
    } as never);

    await expect(adminApi.listModelProfiles()).resolves.toEqual([
      {
        id: 'model-1',
        owner_user_id: null,
        owner_username: null,
        name: 'Model A',
        provider: 'openai',
        model: 'gpt-test',
        base_url: null,
        api_key: null,
        supports_images: false,
        supports_reasoning: false,
        supports_responses: false,
        temperature: null,
        thinking_level: null,
        is_default: false,
        enabled: true,
        created_at: '2026-05-21T00:00:00Z',
        updated_at: '2026-05-21T00:00:00Z',
      },
    ]);
  });

  it('encodes dynamic model and policy identifiers in admin paths', async () => {
    vi.mocked(client.put).mockResolvedValue({ data: {} } as never);
    vi.mocked(client.delete).mockResolvedValue({} as never);

    await adminApi.updateModelProfile('model/1', {
      name: 'Model A',
      provider: 'openai',
      model: 'gpt-test',
    });
    await adminApi.deleteModelProfile('model/1');
    await adminApi.updateJobPolicy('summary/rollup', {});

    expect(client.put).toHaveBeenNthCalledWith(
      1,
      '/admin/model-profiles/model%2F1',
      expect.any(Object),
    );
    expect(client.delete).toHaveBeenCalledWith('/admin/model-profiles/model%2F1');
    expect(client.put).toHaveBeenNthCalledWith(
      2,
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
        model_profile_id: null,
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
