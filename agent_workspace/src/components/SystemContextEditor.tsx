import React from 'react';

import { useChatStore } from '../lib/store';
import { useChatStoreFromContext } from '../lib/store/ChatStoreContext';
import ConfirmDialog from './ui/ConfirmDialog';
import SystemContextSidebar from './systemContextEditor/SystemContextSidebar';
import SystemContextWorkspace from './systemContextEditor/SystemContextWorkspace';
import { useSystemContextEditorController } from './systemContextEditor/useSystemContextEditorController';
import type { SystemContextEditorStoreLike } from './systemContextEditor/types';

interface SystemContextEditorProps {
  onClose?: () => void;
  store?: any;
}

const DocumentIcon = () => (
  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
  </svg>
);

const SaveIcon = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
  </svg>
);

const XMarkIcon = () => (
  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
  </svg>
);

function useResolvedStore(externalStore?: SystemContextEditorProps['store']): SystemContextEditorStoreLike {
  if (externalStore) {
    return externalStore() as SystemContextEditorStoreLike;
  }

  try {
    return useChatStoreFromContext() as SystemContextEditorStoreLike;
  } catch {
    return useChatStore() as SystemContextEditorStoreLike;
  }
}

const SystemContextEditor: React.FC<SystemContextEditorProps> = ({ onClose, store: externalStore }) => {
  const storeData = useResolvedStore(externalStore);
  const {
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
  } = useSystemContextEditorController(storeData);

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
        <SystemContextSidebar
          isLoading={isLoading}
          searchKeyword={searchKeyword}
          selectedContextId={selectedContextId}
          filteredContexts={filteredContexts}
          onSearchKeywordChange={setSearchKeyword}
          onCreate={handleCreate}
          onSelectContext={fillEditorFromContext}
          onDeleteContext={handleDelete}
        />

        <section className="flex-1 min-w-0 flex flex-col">
          <SystemContextWorkspace
            viewMode={viewMode}
            selectedContextName={selectedContextName}
            formData={formData}
            assistantForm={assistantForm}
            assistantError={assistantError}
            qualityReport={qualityReport}
            candidates={candidates}
            actionError={actionError}
            onNameChange={handleNameChange}
            onContentChange={handleContentChange}
            onAssistantFieldChange={handleAssistantFieldChange}
            onSelectCandidate={handleSelectCandidate}
          />

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
