import { useCallback, useEffect, useMemo, useState } from 'react';

import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { splitLines, readContextContent, readContextName } from './helpers';
import type {
  AssistantFormState,
  PromptCandidate,
  PromptQualityReport,
  SystemContextEditorStoreLike,
  SystemContextFormData,
  SystemContextLike,
  ViewMode,
} from './types';

const DEFAULT_ASSISTANT_FORM: AssistantFormState = {
  scene: '',
  style: '专业、简洁',
  language: '中文',
  outputFormat: '结构化：结论/步骤/代码/风险',
  constraintsText: '',
  forbiddenText: '',
  optimizeGoal: '提升约束完整性与可执行性',
};

export function useSystemContextEditorController(storeData: SystemContextEditorStoreLike) {
  const {
    systemContexts,
    loadSystemContexts,
    createSystemContext,
    updateSystemContext,
    deleteSystemContext,
    generateSystemContextDraft,
    optimizeSystemContextDraft,
    evaluateSystemContextDraft,
    aiModelConfigs,
    selectedModelId,
  } = storeData;

  const [viewMode, setViewMode] = useState<ViewMode>('list');
  const [selectedContextId, setSelectedContextId] = useState<string | null>(null);
  const [searchKeyword, setSearchKeyword] = useState('');
  const [formData, setFormData] = useState<SystemContextFormData>({ name: '', content: '' });
  const [selectedAppIds, setSelectedAppIds] = useState<string[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [assistantForm, setAssistantForm] = useState<AssistantFormState>(DEFAULT_ASSISTANT_FORM);
  const [assistantBusy, setAssistantBusy] = useState(false);
  const [assistantError, setAssistantError] = useState<string | null>(null);
  const [candidates, setCandidates] = useState<PromptCandidate[]>([]);
  const [qualityReport, setQualityReport] = useState<PromptQualityReport | null>(null);
  const { dialogState, showConfirmDialog, handleConfirm, handleCancel } = useConfirmDialog();

  useEffect(() => {
    const loadData = async () => {
      setIsLoading(true);
      try {
        await loadSystemContexts();
      } finally {
        setIsLoading(false);
      }
    };

    void loadData();
  }, [loadSystemContexts]);

  const normalizedContexts = useMemo(
    () => (Array.isArray(systemContexts) ? systemContexts : []),
    [systemContexts],
  );

  const filteredContexts = useMemo(() => {
    const keyword = searchKeyword.trim().toLowerCase();
    if (!keyword) {
      return normalizedContexts;
    }

    return normalizedContexts.filter((context) => {
      const name = readContextName(context).toLowerCase();
      const content = readContextContent(context).toLowerCase();
      return name.includes(keyword) || content.includes(keyword);
    });
  }, [normalizedContexts, searchKeyword]);

  const selectedModelConfig = useMemo(() => {
    if (!selectedModelId || !Array.isArray(aiModelConfigs)) {
      return null;
    }
    return aiModelConfigs.find((model) => model.id === selectedModelId) || null;
  }, [aiModelConfigs, selectedModelId]);

  const selectedContextName = useMemo(() => {
    const current = normalizedContexts.find((item) => item.id === selectedContextId);
    return current ? readContextName(current) : '';
  }, [normalizedContexts, selectedContextId]);

  const resetAssistantOutputs = useCallback(() => {
    setCandidates([]);
    setQualityReport(null);
    setActionError(null);
    setAssistantError(null);
  }, []);

  const getModelPayload = useCallback(() => {
    if (!selectedModelConfig) {
      return undefined;
    }

    return {
      model_name: selectedModelConfig.model_name || selectedModelConfig.model,
      model: selectedModelConfig.model,
      provider: selectedModelConfig.provider,
      api_key: selectedModelConfig.api_key,
      base_url: selectedModelConfig.base_url,
      temperature: 0.5,
    };
  }, [selectedModelConfig]);

  const fillEditorFromContext = useCallback((context: SystemContextLike) => {
    setSelectedContextId(context.id || null);
    setFormData({
      name: readContextName(context),
      content: readContextContent(context),
    });
    setSelectedAppIds(Array.isArray(context.app_ids) ? context.app_ids : []);
    setAssistantForm((prev) => ({
      ...prev,
      scene: readContextName(context) || prev.scene,
    }));
    setViewMode('edit');
    resetAssistantOutputs();
  }, [resetAssistantOutputs]);

  const handleCreate = useCallback(() => {
    setViewMode('create');
    setSelectedContextId(null);
    setFormData({ name: '', content: '' });
    setSelectedAppIds([]);
    setAssistantForm((prev) => ({
      ...prev,
      scene: '',
    }));
    resetAssistantOutputs();
  }, [resetAssistantOutputs]);

  const handleSave = useCallback(async () => {
    const name = formData.name.trim();
    const content = formData.content.trim();
    if (!name || !content) {
      setActionError('名称和内容不能为空。');
      return;
    }

    setActionError(null);
    setIsSaving(true);
    try {
      if (viewMode === 'create') {
        const created = await createSystemContext(name, content, selectedAppIds);
        if (created?.id) {
          setSelectedContextId(created.id);
        }
      } else if (viewMode === 'edit' && selectedContextId) {
        await updateSystemContext(selectedContextId, name, content, selectedAppIds);
      }

      await loadSystemContexts();
      setViewMode('list');
    } catch (error) {
      setActionError(error instanceof Error ? error.message : '保存失败。');
    } finally {
      setIsSaving(false);
    }
  }, [
    createSystemContext,
    formData.content,
    formData.name,
    loadSystemContexts,
    selectedAppIds,
    selectedContextId,
    updateSystemContext,
    viewMode,
  ]);

  const handleDelete = useCallback((context: SystemContextLike) => {
    showConfirmDialog({
      title: '删除系统提示词',
      message: `确定删除 "${readContextName(context)}" 吗？此操作无法撤销。`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
      onConfirm: async () => {
        if (!context.id) {
          return;
        }

        try {
          await deleteSystemContext(context.id);
          if (context.id === selectedContextId) {
            setSelectedContextId(null);
            setFormData({ name: '', content: '' });
            setSelectedAppIds([]);
            setViewMode('list');
          }
          await loadSystemContexts();
        } catch (error) {
          setActionError(error instanceof Error ? error.message : '删除失败。');
        }
      },
    });
  }, [deleteSystemContext, loadSystemContexts, selectedContextId, showConfirmDialog]);

  const handleBackToList = useCallback(() => {
    setViewMode('list');
    setActionError(null);
  }, []);

  const handleSelectCandidate = useCallback((candidate: PromptCandidate) => {
    setFormData((prev) => ({
      ...prev,
      content: candidate.content,
      name: prev.name || assistantForm.scene || prev.name,
    }));
    if (candidate.report) {
      setQualityReport(candidate.report);
    }
  }, [assistantForm.scene]);

  const handleAiGenerate = useCallback(async () => {
    if (typeof generateSystemContextDraft !== 'function') {
      setAssistantError('当前环境不可用 AI 生成功能。');
      return;
    }

    const scene = assistantForm.scene.trim() || formData.name.trim();
    if (!scene) {
      setAssistantError('请先填写 AI 场景。');
      return;
    }

    setAssistantBusy(true);
    setAssistantError(null);
    try {
      const response = await generateSystemContextDraft({
        scene,
        style: assistantForm.style.trim() || undefined,
        language: assistantForm.language.trim() || undefined,
        output_format: assistantForm.outputFormat.trim() || undefined,
        constraints: splitLines(assistantForm.constraintsText),
        forbidden: splitLines(assistantForm.forbiddenText),
        candidate_count: 3,
        ai_model_config: getModelPayload(),
      });

      const resultCandidates = Array.isArray(response?.candidates)
        ? response.candidates.filter((item) => (
          typeof item?.content === 'string' && item.content.trim().length > 0
        ))
        : [];

      if (resultCandidates.length === 0) {
        setAssistantError('AI 未返回可用候选内容。');
        return;
      }

      setCandidates(resultCandidates);
      if (!formData.content.trim()) {
        handleSelectCandidate(resultCandidates[0]);
      }
    } catch (error) {
      setAssistantError(error instanceof Error ? error.message : 'AI 生成失败。');
    } finally {
      setAssistantBusy(false);
    }
  }, [
    assistantForm.constraintsText,
    assistantForm.forbiddenText,
    assistantForm.language,
    assistantForm.outputFormat,
    assistantForm.scene,
    assistantForm.style,
    formData.content,
    formData.name,
    generateSystemContextDraft,
    getModelPayload,
    handleSelectCandidate,
  ]);

  const handleAiOptimize = useCallback(async () => {
    if (typeof optimizeSystemContextDraft !== 'function') {
      setAssistantError('当前环境不可用 AI 优化功能。');
      return;
    }

    const content = formData.content.trim();
    if (!content) {
      setAssistantError('请先输入内容再进行 AI 优化。');
      return;
    }

    setAssistantBusy(true);
    setAssistantError(null);
    try {
      const response = await optimizeSystemContextDraft({
        content,
        goal: assistantForm.optimizeGoal.trim() || undefined,
        keep_intent: true,
        ai_model_config: getModelPayload(),
      });

      const optimized = typeof response?.optimized_content === 'string'
        ? response.optimized_content.trim()
        : '';
      if (!optimized) {
        setAssistantError('AI 优化返回为空。');
        return;
      }

      setFormData((prev) => ({
        ...prev,
        content: optimized,
      }));
      setCandidates([
        {
          title: '优化结果',
          content: optimized,
          score: response?.score_after,
          report: response?.report_after,
        },
      ]);
      if (response?.report_after) {
        setQualityReport(response.report_after);
      }
    } catch (error) {
      setAssistantError(error instanceof Error ? error.message : 'AI 优化失败。');
    } finally {
      setAssistantBusy(false);
    }
  }, [assistantForm.optimizeGoal, formData.content, getModelPayload, optimizeSystemContextDraft]);

  const handleAiEvaluate = useCallback(async () => {
    if (typeof evaluateSystemContextDraft !== 'function') {
      setAssistantError('当前环境不可用 AI 评估功能。');
      return;
    }

    const content = formData.content.trim();
    if (!content) {
      setAssistantError('请先输入内容再进行 AI 评估。');
      return;
    }

    setAssistantBusy(true);
    setAssistantError(null);
    try {
      const response = await evaluateSystemContextDraft({ content });
      if (response?.report) {
        setQualityReport(response.report);
      } else {
        setAssistantError('AI 评估未返回报告。');
      }
    } catch (error) {
      setAssistantError(error instanceof Error ? error.message : 'AI 评估失败。');
    } finally {
      setAssistantBusy(false);
    }
  }, [evaluateSystemContextDraft, formData.content]);

  const handleNameChange = useCallback((value: string) => {
    setFormData((prev) => ({ ...prev, name: value }));
  }, []);

  const handleContentChange = useCallback((value: string) => {
    setFormData((prev) => ({ ...prev, content: value }));
  }, []);

  const handleAssistantFieldChange = useCallback(<K extends keyof AssistantFormState>(
    field: K,
    value: AssistantFormState[K],
  ) => {
    setAssistantForm((prev) => ({ ...prev, [field]: value }));
  }, []);

  return {
    viewMode,
    selectedContextId,
    searchKeyword,
    formData,
    isLoading,
    isSaving,
    actionError,
    assistantForm,
    assistantBusy,
    assistantError,
    candidates,
    qualityReport,
    filteredContexts,
    selectedContextName,
    dialogState,
    handleConfirm,
    handleCancel,
    setSearchKeyword,
    fillEditorFromContext,
    handleCreate,
    handleSave,
    handleDelete,
    handleBackToList,
    handleSelectCandidate,
    handleAiGenerate,
    handleAiOptimize,
    handleAiEvaluate,
    handleNameChange,
    handleContentChange,
    handleAssistantFieldChange,
  };
}
