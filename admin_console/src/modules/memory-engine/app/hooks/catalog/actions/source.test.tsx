// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '../../../../api';
import { buildCatalogSourceActions } from './source';

vi.mock('../../../../api', () => ({
  api: {
    upsertSource: vi.fn(),
    rotateSourceSecret: vi.fn(),
  },
}));

describe('buildCatalogSourceActions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('passes tenant_id when rotating a source secret', async () => {
    const rotateSourceSecret = vi.mocked(api.rotateSourceSecret);
    const loadSources = vi.fn().mockResolvedValue([]);
    const setRotatedSecret = vi.fn();
    const message = {
      success: vi.fn(),
      error: vi.fn(),
    };

    rotateSourceSecret.mockResolvedValue({
      source: {
        id: 'source-1',
        tenant_id: 'tenant-a',
        source_id: 'source-a',
        source_type: 'sdk',
        name: 'Source A',
        status: 'active',
        sdk_enabled: true,
        created_at: '2026-05-20T00:00:00Z',
        updated_at: '2026-05-20T00:00:00Z',
      },
      secret_key: 'secret-value',
    });

    const actions = buildCatalogSourceActions(
      {
        message: message as never,
        controls: {
          editingSource: null,
          editingModel: null,
          sourceForm: {} as never,
          modelForm: {} as never,
          setRotatedSecret,
          setSourceSubmitting: vi.fn(),
          setModelSubmitting: vi.fn(),
          setSourceModalOpen: vi.fn(),
          setModelModalOpen: vi.fn(),
          setEditingSource: vi.fn(),
          setEditingModel: vi.fn(),
          setSavingPolicyJobType: vi.fn(),
          setGeneratingPolicyJobType: vi.fn(),
        },
        loaders: {
          loadSources,
          loadModels: vi.fn(),
          loadPolicies: vi.fn(),
        },
      },
      vi.fn(),
    );

    await actions.handleRotateSourceSecret({
      id: 'source-1',
      tenant_id: 'tenant-a',
      source_id: 'source-a',
      source_type: 'sdk',
      name: 'Source A',
      status: 'active',
      sdk_enabled: true,
      created_at: '2026-05-20T00:00:00Z',
      updated_at: '2026-05-20T00:00:00Z',
    });

    expect(rotateSourceSecret).toHaveBeenCalledWith('source-a', 'tenant-a');
    expect(setRotatedSecret).toHaveBeenCalledTimes(1);
    expect(loadSources).toHaveBeenCalledTimes(1);
  });

  it('does not misreport rotate success as failure when only the follow-up refresh fails', async () => {
    const rotateSourceSecret = vi.mocked(api.rotateSourceSecret);
    const loadSources = vi.fn().mockRejectedValue(new Error('refresh failed'));
    const setRotatedSecret = vi.fn();
    const message = {
      success: vi.fn(),
      error: vi.fn(),
    };

    rotateSourceSecret.mockResolvedValue({
      source: {
        id: 'source-1',
        tenant_id: 'tenant-a',
        source_id: 'source-a',
        source_type: 'sdk',
        name: 'Source A',
        status: 'active',
        sdk_enabled: true,
        created_at: '2026-05-20T00:00:00Z',
        updated_at: '2026-05-20T00:00:00Z',
      },
      secret_key: 'secret-value',
    });

    const actions = buildCatalogSourceActions(
      {
        message: message as never,
        controls: {
          editingSource: null,
          editingModel: null,
          sourceForm: {} as never,
          modelForm: {} as never,
          setRotatedSecret,
          setSourceSubmitting: vi.fn(),
          setModelSubmitting: vi.fn(),
          setSourceModalOpen: vi.fn(),
          setModelModalOpen: vi.fn(),
          setEditingSource: vi.fn(),
          setEditingModel: vi.fn(),
          setSavingPolicyJobType: vi.fn(),
          setGeneratingPolicyJobType: vi.fn(),
        },
        loaders: {
          loadSources,
          loadModels: vi.fn(),
          loadPolicies: vi.fn(),
        },
      },
      vi.fn(),
    );

    await actions.handleRotateSourceSecret({
      id: 'source-1',
      tenant_id: 'tenant-a',
      source_id: 'source-a',
      source_type: 'sdk',
      name: 'Source A',
      status: 'active',
      sdk_enabled: true,
      created_at: '2026-05-20T00:00:00Z',
      updated_at: '2026-05-20T00:00:00Z',
    });

    expect(message.success).toHaveBeenCalledWith('已轮换 SDK Secret：Source A');
    expect(message.error).toHaveBeenCalledWith(
      '轮换 SDK Secret成功，但刷新接入系统列表失败：Error: refresh failed',
    );
    expect(message.error).not.toHaveBeenCalledWith(
      expect.stringContaining('轮换密钥失败'),
    );
  });

  it('does not misreport save success as failure when only the overview refresh callback fails', async () => {
    const upsertSource = vi.mocked(api.upsertSource);
    const afterSourceMutation = vi.fn().mockRejectedValue(new Error('overview failed'));
    const closeSourceModal = vi.fn();
    const message = {
      success: vi.fn(),
      error: vi.fn(),
    };

    upsertSource.mockResolvedValue({
      id: 'source-1',
      tenant_id: 'tenant-a',
      source_id: 'source-a',
      source_type: 'sdk_system',
      name: 'Source A',
      status: 'active',
      sdk_enabled: true,
      created_at: '2026-05-20T00:00:00Z',
      updated_at: '2026-05-20T00:00:00Z',
    });

    const actions = buildCatalogSourceActions(
      {
        message: message as never,
        controls: {
          editingSource: null,
          editingModel: null,
          sourceForm: {
            validateFields: vi.fn().mockResolvedValue({
              tenant_id: 'tenant-a',
              source_id: 'source-a',
              name: 'Source A',
              description: '',
              enabled: true,
            }),
          } as never,
          modelForm: {} as never,
          setRotatedSecret: vi.fn(),
          setSourceSubmitting: vi.fn(),
          setModelSubmitting: vi.fn(),
          setSourceModalOpen: vi.fn(),
          setModelModalOpen: vi.fn(),
          setEditingSource: vi.fn(),
          setEditingModel: vi.fn(),
          setSavingPolicyJobType: vi.fn(),
          setGeneratingPolicyJobType: vi.fn(),
        },
        loaders: {
          loadSources: vi.fn().mockResolvedValue([]),
          loadModels: vi.fn(),
          loadPolicies: vi.fn(),
        },
        callbacks: {
          afterSourceMutation,
        },
      },
      closeSourceModal,
    );

    await actions.handleSubmitSource();

    expect(message.success).toHaveBeenCalledWith('已创建接入系统：Source A');
    expect(closeSourceModal).toHaveBeenCalledTimes(1);
    expect(message.error).toHaveBeenCalledWith(
      '保存接入系统成功，但刷新概览统计失败：Error: overview failed',
    );
    expect(message.error).not.toHaveBeenCalledWith(
      expect.stringContaining('保存接入系统失败'),
    );
  });
});
