import { beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '../../../../api';
import { buildCatalogModelActions } from './model';

vi.mock('../../../../api', () => ({
  api: {
    createModelProfile: vi.fn(),
    updateModelProfile: vi.fn(),
    deleteModelProfile: vi.fn(),
  },
}));

describe('buildCatalogModelActions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('does not misreport delete success as failure when only the follow-up refresh fails', async () => {
    const deleteModelProfile = vi.mocked(api.deleteModelProfile);
    const loadModels = vi.fn().mockRejectedValue(new Error('refresh failed'));
    const message = {
      success: vi.fn(),
      error: vi.fn(),
    };

    deleteModelProfile.mockResolvedValue(undefined);

    const actions = buildCatalogModelActions(
      {
        message: message as never,
        controls: {
          editingSource: null,
          editingModel: null,
          sourceForm: {} as never,
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
          loadSources: vi.fn(),
          loadModels,
          loadPolicies: vi.fn(),
        },
      },
      vi.fn(),
    );

    await actions.handleDeleteModel({
      id: 'model-1',
      name: 'Model A',
      provider: 'openai',
      model: 'gpt-test',
      supports_images: false,
      supports_reasoning: true,
      supports_responses: true,
      is_default: false,
      enabled: true,
      created_at: '2026-05-20T00:00:00Z',
      updated_at: '2026-05-20T00:00:00Z',
    });

    expect(message.success).toHaveBeenCalledWith('已删除模型配置：Model A');
    expect(message.error).toHaveBeenCalledWith(
      '删除模型配置成功，但刷新模型列表失败：Error: refresh failed',
    );
    expect(message.error).not.toHaveBeenCalledWith(
      expect.stringContaining('删除模型配置失败'),
    );
  });

  it('does not misreport save success as failure when only the overview refresh callback fails', async () => {
    const createModelProfile = vi.mocked(api.createModelProfile);
    const afterModelMutation = vi.fn().mockRejectedValue(new Error('overview failed'));
    const closeModelModal = vi.fn();
    const message = {
      success: vi.fn(),
      error: vi.fn(),
    };

    createModelProfile.mockResolvedValue({
      id: 'model-1',
      name: 'Model A',
      provider: 'openai',
      model: 'gpt-test',
      supports_images: false,
      supports_reasoning: true,
      supports_responses: true,
      is_default: false,
      enabled: true,
      created_at: '2026-05-20T00:00:00Z',
      updated_at: '2026-05-20T00:00:00Z',
    });

    const actions = buildCatalogModelActions(
      {
        message: message as never,
        controls: {
          editingSource: null,
          editingModel: null,
          sourceForm: {} as never,
          modelForm: {
            validateFields: vi.fn().mockResolvedValue({
              name: 'Model A',
              provider: 'openai',
              model: 'gpt-test',
              base_url: '',
              api_key: '',
              supports_images: false,
              supports_reasoning: true,
              supports_responses: true,
              temperature: null,
              thinking_level: '',
              is_default: false,
              enabled: true,
            }),
          } as never,
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
          loadSources: vi.fn(),
          loadModels: vi.fn().mockResolvedValue([]),
          loadPolicies: vi.fn(),
        },
        callbacks: {
          afterModelMutation,
        },
      },
      closeModelModal,
    );

    await actions.handleSubmitModel();

    expect(message.success).toHaveBeenCalledWith('已创建模型配置：Model A');
    expect(closeModelModal).toHaveBeenCalledTimes(1);
    expect(message.error).toHaveBeenCalledWith(
      '保存模型配置成功，但刷新概览统计失败：Error: overview failed',
    );
    expect(message.error).not.toHaveBeenCalledWith(
      expect.stringContaining('保存模型配置失败'),
    );
  });
});
