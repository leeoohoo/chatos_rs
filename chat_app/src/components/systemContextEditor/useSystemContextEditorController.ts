import { useCallback, useEffect, useMemo, useState } from 'react';

import { useI18n, type TranslateFn } from '../../i18n/I18nProvider';
import { useDialogService } from '../ui/DialogProvider';
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

const buildDefaultAssistantForm = (t: TranslateFn): AssistantFormState => ({
  scene: '',
  style: t('systemContext.assistant.defaultStyle'),
  language: t('systemContext.assistant.defaultLanguage'),
  outputFormat: t('systemContext.assistant.defaultOutputFormat'),
  constraintsText: '',
  forbiddenText: '',
  optimizeGoal: t('systemContext.assistant.defaultOptimizeGoal'),
});

export function useSystemContextEditorController(storeData: SystemContextEditorStoreLike) {
  const { t } = useI18n();
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
  const [assistantForm, setAssistantForm] = useState<AssistantFormState>(() => buildDefaultAssistantForm(t));
  const [assistantBusy, setAssistantBusy] = useState(false);
  const [assistantError, setAssistantError] = useState<string | null>(null);
  const [candidates, setCandidates] = useState<PromptCandidate[]>([]);
  const [qualityReport, setQualityReport] = useState<PromptQualityReport | null>(null);
  const { confirm } = useDialogService();

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
      setActionError(t('systemContext.error.required'));
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
      setActionError(error instanceof Error ? error.message : t('systemContext.error.save'));
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
    t,
  ]);

  const handleDelete = useCallback(async (context: SystemContextLike) => {
    const confirmed = await confirm({
      title: t('systemContext.confirm.deleteTitle'),
      message: t('systemContext.confirm.deleteMessage', { name: readContextName(context) }),
      confirmText: t('aiModelManager.action.delete'),
      cancelText: t('common.cancel'),
      type: 'danger',
    });
    if (!confirmed || !context.id) {
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
      setActionError(error instanceof Error ? error.message : t('systemContext.error.delete'));
    }
  }, [confirm, deleteSystemContext, loadSystemContexts, selectedContextId, t]);

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
      setAssistantError(t('systemContext.error.generateUnavailable'));
      return;
    }

    const scene = assistantForm.scene.trim() || formData.name.trim();
    if (!scene) {
      setAssistantError(t('systemContext.error.generateMissingScene'));
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
        model_config_id: selectedModelConfig?.id,
        ai_model_config: getModelPayload(),
      });

      const resultCandidates = Array.isArray(response?.candidates)
        ? response.candidates.filter((item) => (
          typeof item?.content === 'string' && item.content.trim().length > 0
        ))
        : [];

      if (resultCandidates.length === 0) {
        setAssistantError(t('systemContext.error.generateEmpty'));
        return;
      }

      setCandidates(resultCandidates);
      if (!formData.content.trim()) {
        handleSelectCandidate(resultCandidates[0]);
      }
    } catch (error) {
      setAssistantError(error instanceof Error ? error.message : t('systemContext.error.generateFailed'));
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
      setAssistantError(t('systemContext.error.optimizeUnavailable'));
      return;
    }

    const content = formData.content.trim();
    if (!content) {
      setAssistantError(t('systemContext.error.optimizeMissingContent'));
      return;
    }

    setAssistantBusy(true);
    setAssistantError(null);
    try {
      const response = await optimizeSystemContextDraft({
        content,
        goal: assistantForm.optimizeGoal.trim() || undefined,
        keep_intent: true,
        model_config_id: selectedModelConfig?.id,
        ai_model_config: getModelPayload(),
      });

      const optimized = typeof response?.optimized_content === 'string'
        ? response.optimized_content.trim()
        : '';
      if (!optimized) {
        setAssistantError(t('systemContext.error.optimizeEmpty'));
        return;
      }

      setFormData((prev) => ({
        ...prev,
        content: optimized,
      }));
      setCandidates([
        {
          title: t('systemContext.optimizeResult'),
          content: optimized,
          score: response?.score_after,
          report: response?.report_after,
        },
      ]);
      if (response?.report_after) {
        setQualityReport(response.report_after);
      }
    } catch (error) {
      setAssistantError(error instanceof Error ? error.message : t('systemContext.error.optimizeFailed'));
    } finally {
      setAssistantBusy(false);
    }
  }, [assistantForm.optimizeGoal, formData.content, getModelPayload, optimizeSystemContextDraft, t]);

  const handleAiEvaluate = useCallback(async () => {
    if (typeof evaluateSystemContextDraft !== 'function') {
      setAssistantError(t('systemContext.error.evaluateUnavailable'));
      return;
    }

    const content = formData.content.trim();
    if (!content) {
      setAssistantError(t('systemContext.error.evaluateMissingContent'));
      return;
    }

    setAssistantBusy(true);
    setAssistantError(null);
    try {
      const response = await evaluateSystemContextDraft({
        content,
        model_config_id: selectedModelConfig?.id,
      });
      if (response?.report) {
        setQualityReport(response.report);
      } else {
        setAssistantError(t('systemContext.error.evaluateEmpty'));
      }
    } catch (error) {
      setAssistantError(error instanceof Error ? error.message : t('systemContext.error.evaluateFailed'));
    } finally {
      setAssistantBusy(false);
    }
  }, [evaluateSystemContextDraft, formData.content, selectedModelConfig?.id, t]);

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
