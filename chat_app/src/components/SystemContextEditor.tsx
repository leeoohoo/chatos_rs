import React, { useEffect, useMemo, useState } from 'react';
import { useChatStoreFromContext } from '../lib/store/ChatStoreContext';
import { useChatStore } from '../lib/store';
import type { SystemContext } from '../types';
import ConfirmDialog from './ui/ConfirmDialog';
import { useConfirmDialog } from '../hooks/useConfirmDialog';

interface SystemContextEditorProps {
  onClose?: () => void;
  store?: any;
}

type ViewMode = 'list' | 'create' | 'edit';

type PromptQualityReport = {
  clarity?: number;
  constraint_completeness?: number;
  conflict_risk?: number;
  verbosity?: number;
  overall?: number;
  warnings?: string[];
};

type PromptCandidate = {
  title?: string;
  content: string;
  score?: number;
  report?: PromptQualityReport;
};

const DocumentIcon = () => (
  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
  </svg>
);

const PlusIcon = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
  </svg>
);

const SaveIcon = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
  </svg>
);

const TrashIcon = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
  </svg>
);

const XMarkIcon = () => (
  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
  </svg>
);

function splitLines(value: string): string[] {
  return value
    .split(/\r?\n/)
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
}

function readContextContent(context: any): string {
  if (typeof context?.content === 'string') {
    return context.content;
  }
  return '';
}

function readContextName(context: any): string {
  if (typeof context?.name === 'string') {
    return context.name;
  }
  return '';
}

function readContextUpdatedAt(context: any): string {
  const raw = context?.updatedAt || context?.updated_at || context?.createdAt || context?.created_at;
  if (!raw) {
    return '-';
  }
  const date = new Date(raw);
  if (Number.isNaN(date.getTime())) {
    return String(raw);
  }
  return date.toLocaleString();
}

const SystemContextEditor: React.FC<SystemContextEditorProps> = ({ onClose, store: externalStore }) => {
  let storeData: any;

  if (externalStore) {
    storeData = externalStore();
  } else {
    try {
      storeData = useChatStoreFromContext();
    } catch {
      storeData = useChatStore();
    }
  }

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
  const [formData, setFormData] = useState({ name: '', content: '' });
  const [selectedAppIds, setSelectedAppIds] = useState<string[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  const [assistantForm, setAssistantForm] = useState({
    scene: '',
    style: '专业、简洁',
    language: '中文',
    outputFormat: '结构化：结论/步骤/代码/风险',
    constraintsText: '',
    forbiddenText: '',
    optimizeGoal: '提升约束完整性与可执行性',
  });
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

  const normalizedContexts = useMemo(() => {
    return Array.isArray(systemContexts) ? systemContexts : [];
  }, [systemContexts]);

  const filteredContexts = useMemo(() => {
    const keyword = searchKeyword.trim().toLowerCase();
    if (!keyword) {
      return normalizedContexts;
    }
    return normalizedContexts.filter((context: any) => {
      const name = readContextName(context).toLowerCase();
      const content = readContextContent(context).toLowerCase();
      return name.includes(keyword) || content.includes(keyword);
    });
  }, [normalizedContexts, searchKeyword]);

  const selectedModelConfig = useMemo(() => {
    if (!selectedModelId || !Array.isArray(aiModelConfigs)) {
      return null;
    }
    return aiModelConfigs.find((model: any) => model.id === selectedModelId) || null;
  }, [aiModelConfigs, selectedModelId]);

  const fillEditorFromContext = (context: any) => {
    setSelectedContextId(context?.id || null);
    setFormData({
      name: readContextName(context),
      content: readContextContent(context),
    });
    const appIds = Array.isArray(context?.app_ids) ? context.app_ids : [];
    setSelectedAppIds(appIds);
    setAssistantForm((prev) => ({
      ...prev,
      scene: readContextName(context) || prev.scene,
    }));
    setCandidates([]);
    setQualityReport(null);
    setActionError(null);
    setAssistantError(null);
    setViewMode('edit');
  };

  const handleCreate = () => {
    setViewMode('create');
    setSelectedContextId(null);
    setFormData({ name: '', content: '' });
    setSelectedAppIds([]);
    setCandidates([]);
    setQualityReport(null);
    setActionError(null);
    setAssistantError(null);
    setAssistantForm((prev) => ({
      ...prev,
      scene: '',
    }));
  };

  const handleSave = async () => {
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
  };

  const handleDelete = (context: SystemContext) => {
    showConfirmDialog({
      title: '删除系统提示词',
      message: `确定删除 "${(context as any).name || ''}" 吗？此操作无法撤销。`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
      onConfirm: async () => {
        try {
          await deleteSystemContext((context as any).id);
          if ((context as any).id === selectedContextId) {
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
  };

  const handleBackToList = () => {
    setViewMode('list');
    setActionError(null);
  };

  const handleSelectCandidate = (candidate: PromptCandidate) => {
    setFormData((prev) => ({
      ...prev,
      content: candidate.content,
      name: prev.name || assistantForm.scene || prev.name,
    }));
    if (candidate.report) {
      setQualityReport(candidate.report);
    }
  };

  const getModelPayload = () => {
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
  };

  const handleAiGenerate = async () => {
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
        ? response.candidates.filter((item: any) => typeof item?.content === 'string' && item.content.trim().length > 0)
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
  };

  const handleAiOptimize = async () => {
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

      const optimized = typeof response?.optimized_content === 'string' ? response.optimized_content.trim() : '';
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
  };

  const handleAiEvaluate = async () => {
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
  };

  const selectedContextName = useMemo(() => {
    const current = normalizedContexts.find((item: any) => item.id === selectedContextId);
    return current ? readContextName(current) : '';
  }, [normalizedContexts, selectedContextId]);

  return (
    <div className="h-screen w-full bg-background text-foreground flex flex-col">
      <div className="flex items-center justify-between px-6 py-4 border-b border-border">
        <div className="flex items-center gap-3">
          <DocumentIcon />
          <div>
            <h2 className="text-xl font-semibold">系统提示词管理</h2>
            <p className="text-xs text-muted-foreground">全屏工作区（AI 生成 / 优化 / 评估）</p>
          </div>
        </div>
        <button
          onClick={onClose}
          className="inline-flex items-center gap-2 px-3 py-2 text-sm border border-border rounded-md hover:bg-accent"
        >
          <XMarkIcon />
          <span>返回</span>
        </button>
      </div>

      <div className="flex-1 min-h-0 flex">
        <aside className="w-80 min-w-80 border-r border-border flex flex-col">
          <div className="p-4 border-b border-border space-y-3">
            <button
              onClick={handleCreate}
              className="w-full inline-flex items-center justify-center gap-2 px-3 py-2 text-sm bg-blue-600 text-white rounded-md hover:bg-blue-700"
            >
              <PlusIcon />
              <span>新建提示词</span>
            </button>
            <input
              type="text"
              value={searchKeyword}
              onChange={(e) => setSearchKeyword(e.target.value)}
              placeholder="搜索提示词"
              className="w-full px-3 py-2 text-sm border border-input bg-background rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
            />
          </div>

          <div className="flex-1 overflow-y-auto">
            {isLoading ? (
              <div className="p-4 text-sm text-muted-foreground">加载中...</div>
            ) : filteredContexts.length === 0 ? (
              <div className="p-4 text-sm text-muted-foreground">暂无提示词</div>
            ) : (
              <ul className="divide-y divide-border">
                {filteredContexts.map((context: any) => {
                  const active = context.id === selectedContextId;
                  return (
                    <li key={context.id} className={active ? 'bg-blue-50 dark:bg-blue-950/20' : ''}>
                      <div className="flex items-center justify-between gap-2 px-4 py-3">
                        <button
                          onClick={() => fillEditorFromContext(context)}
                          className="flex-1 text-left"
                        >
                          <p className="text-sm font-medium truncate">{readContextName(context)}</p>
                          <p className="text-xs text-muted-foreground truncate">更新时间：{readContextUpdatedAt(context)}</p>
                        </button>
                        <button
                          onClick={() => handleDelete(context)}
                          className="p-1 text-muted-foreground hover:text-red-600"
                          title="删除"
                        >
                          <TrashIcon />
                        </button>
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
        </aside>

        <section className="flex-1 min-w-0 flex flex-col">
          <div className="px-6 py-4 border-b border-border">
            <div className="flex flex-wrap items-center gap-3">
              <span className="text-sm text-muted-foreground">模式：</span>
              <span className="text-sm font-medium">{viewMode === 'create' ? '新建' : viewMode === 'edit' ? '编辑' : '列表'}</span>
              {selectedContextName ? (
                <span className="text-xs px-2 py-1 rounded-full bg-accent text-secondary-foreground">{selectedContextName}</span>
              ) : null}
            </div>
          </div>

          <div className="flex-1 min-h-0 overflow-y-auto px-6 py-5 space-y-4">
            <div className="grid grid-cols-1 gap-4">
              <div className="max-w-xl">
                <label className="block text-sm font-medium mb-2">名称</label>
                <input
                  type="text"
                  value={formData.name}
                  onChange={(e) => setFormData((prev) => ({ ...prev, name: e.target.value }))}
                  placeholder="例如：编程助手"
                  className="w-full px-3 py-2 border border-input bg-background rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                />
              </div>
            </div>

            <div className="grid grid-cols-1 lg:grid-cols-4 gap-3">
              <input
                value={assistantForm.scene}
                onChange={(e) => setAssistantForm((prev) => ({ ...prev, scene: e.target.value }))}
                placeholder="AI 场景"
                className="px-3 py-2 text-sm border border-input bg-background rounded-md"
              />
              <input
                value={assistantForm.style}
                onChange={(e) => setAssistantForm((prev) => ({ ...prev, style: e.target.value }))}
                placeholder="AI 风格"
                className="px-3 py-2 text-sm border border-input bg-background rounded-md"
              />
              <input
                value={assistantForm.language}
                onChange={(e) => setAssistantForm((prev) => ({ ...prev, language: e.target.value }))}
                placeholder="AI 语言"
                className="px-3 py-2 text-sm border border-input bg-background rounded-md"
              />
              <input
                value={assistantForm.outputFormat}
                onChange={(e) => setAssistantForm((prev) => ({ ...prev, outputFormat: e.target.value }))}
                placeholder="AI 输出格式"
                className="px-3 py-2 text-sm border border-input bg-background rounded-md"
              />
            </div>

            <div className="grid grid-cols-1 lg:grid-cols-2 gap-3">
              <textarea
                value={assistantForm.constraintsText}
                onChange={(e) => setAssistantForm((prev) => ({ ...prev, constraintsText: e.target.value }))}
                placeholder="AI 约束（每行一条）"
                rows={3}
                className="w-full px-3 py-2 text-sm border border-input bg-background rounded-md resize-none"
              />
              <textarea
                value={assistantForm.forbiddenText}
                onChange={(e) => setAssistantForm((prev) => ({ ...prev, forbiddenText: e.target.value }))}
                placeholder="AI 禁止项（每行一条）"
                rows={3}
                className="w-full px-3 py-2 text-sm border border-input bg-background rounded-md resize-none"
              />
            </div>

            <div>
              <label className="block text-sm font-medium mb-2">优化目标</label>
              <input
                value={assistantForm.optimizeGoal}
                onChange={(e) => setAssistantForm((prev) => ({ ...prev, optimizeGoal: e.target.value }))}
                placeholder="希望 AI 优化什么？"
                className="w-full px-3 py-2 text-sm border border-input bg-background rounded-md"
              />
            </div>

            {assistantError ? (
              <div className="text-sm text-red-600 bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-900 rounded-md px-3 py-2">
                {assistantError}
              </div>
            ) : null}

            {qualityReport ? (
              <div className="grid grid-cols-2 md:grid-cols-5 gap-2 text-xs">
                <div className="px-2 py-2 border rounded-md">清晰度: {qualityReport.clarity ?? '-'}</div>
                <div className="px-2 py-2 border rounded-md">约束完整度: {qualityReport.constraint_completeness ?? '-'}</div>
                <div className="px-2 py-2 border rounded-md">冲突风险: {qualityReport.conflict_risk ?? '-'}</div>
                <div className="px-2 py-2 border rounded-md">冗长度: {qualityReport.verbosity ?? '-'}</div>
                <div className="px-2 py-2 border rounded-md font-medium">总分: {qualityReport.overall ?? '-'}</div>
              </div>
            ) : null}

            {candidates.length > 0 ? (
              <div className="space-y-2">
                <p className="text-sm font-medium">AI 候选</p>
                <div className="space-y-2 max-h-48 overflow-y-auto">
                  {candidates.map((candidate, index) => (
                    <div key={`${candidate.title || 'candidate'}-${index}`} className="border border-border rounded-md p-3">
                      <div className="flex items-center justify-between gap-3">
                        <div className="text-sm font-medium">
                          {candidate.title || `候选-${index + 1}`}
                          {typeof candidate.score === 'number' ? ` - 评分 ${candidate.score}` : ''}
                        </div>
                        <button
                          onClick={() => handleSelectCandidate(candidate)}
                          className="px-2 py-1 text-xs border border-border rounded hover:bg-accent"
                        >
                          使用此版本
                        </button>
                      </div>
                      <p className="mt-2 text-xs text-muted-foreground line-clamp-2">{candidate.content}</p>
                    </div>
                  ))}
                </div>
              </div>
            ) : null}

            <div className="min-h-[360px] flex flex-col">
              <div className="mb-2 flex items-center justify-between">
                <label className="text-sm font-medium">提示词内容</label>
                <span className="text-xs text-muted-foreground">{formData.content.length} 字符</span>
              </div>
              <textarea
                value={formData.content}
                onChange={(e) => setFormData((prev) => ({ ...prev, content: e.target.value }))}
                className="flex-1 w-full min-h-[360px] px-4 py-3 border border-input bg-background rounded-md resize-none font-mono text-sm"
                placeholder="在这里编写或让 AI 生成系统提示词内容..."
              />
            </div>

            {actionError ? (
              <div className="text-sm text-red-600">{actionError}</div>
            ) : null}
          </div>

          <div className="px-6 py-3 border-t border-border flex items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <button
                onClick={handleAiGenerate}
                disabled={assistantBusy}
                className="px-3 py-2 text-sm bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50"
              >
                {assistantBusy ? '执行中...' : 'AI 生成'}
              </button>
              <button
                onClick={handleAiOptimize}
                disabled={assistantBusy}
                className="px-3 py-2 text-sm border border-border rounded-md hover:bg-accent disabled:opacity-50"
              >
                AI 优化
              </button>
              <button
                onClick={handleAiEvaluate}
                disabled={assistantBusy}
                className="px-3 py-2 text-sm border border-border rounded-md hover:bg-accent disabled:opacity-50"
              >
                AI 评估
              </button>
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={handleBackToList}
                className="px-3 py-2 text-sm border border-border rounded-md hover:bg-accent"
              >
                取消
              </button>
              <button
                onClick={handleSave}
                disabled={isSaving}
                className="inline-flex items-center gap-2 px-4 py-2 text-sm bg-green-600 text-white rounded-md hover:bg-green-700 disabled:opacity-50"
              >
                <SaveIcon />
                <span>{isSaving ? '保存中...' : '保存'}</span>
              </button>
            </div>
          </div>
        </section>
      </div>

      <ConfirmDialog
        isOpen={dialogState.isOpen}
        title={dialogState.title}
        message={dialogState.message}
        confirmText={dialogState.confirmText}
        cancelText={dialogState.cancelText}
        type={dialogState.type}
        onConfirm={handleConfirm}
        onCancel={handleCancel}
      />
    </div>
  );
};

export default SystemContextEditor;
