import React, { useState, useEffect } from 'react';
import { useChatStoreFromContext } from '../lib/store/ChatStoreContext';
import { useChatStore } from '../lib/store';
import type { SystemContext } from '../types';
import ConfirmDialog from './ui/ConfirmDialog';
import { useConfirmDialog } from '../hooks/useConfirmDialog';

// 图标组件
const DocumentIcon = () => (
  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
  </svg>
);

const XMarkIcon = () => (
  <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
  </svg>
);

const SaveIcon = () => (
  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 7H5a2 2 0 00-2 2v9a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-3m-1 4l-3 3m0 0l-3-3m3 3V4" />
  </svg>
);

const PlusIcon = () => (
  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
  </svg>
);

const EditIcon = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
  </svg>
);

const TrashIcon = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
  </svg>
);

const CheckIcon = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
  </svg>
);

interface SystemContextEditorProps {
  onClose?: () => void;
  store?: any; // 可选的store参数，用于在没有Context Provider的情况下使用
}

type ViewMode = 'list' | 'create' | 'edit';

const SystemContextEditor: React.FC<SystemContextEditorProps> = ({ onClose, store: externalStore }) => {
  // 尝试使用外部传入的store，如果没有则尝试使用Context，最后回退到默认store
  let storeData;
  
  if (externalStore) {
    // 使用外部传入的store
    storeData = externalStore();
  } else {
    // 尝试使用Context store，如果失败则使用默认store
    try {
      storeData = useChatStoreFromContext();
    } catch (error) {
      // 如果Context不可用，使用默认store
      storeData = useChatStore();
    }
  }

  const { 
    systemContexts, 
    activeSystemContext: _activeSystemContext,
    loadSystemContexts, 
    createSystemContext,
    updateSystemContext, 
    deleteSystemContext,
    activateSystemContext,
    applications,
    loadApplications
  } = storeData;
  
  const [viewMode, setViewMode] = useState<ViewMode>('list');
  const [editingContext, setEditingContext] = useState<SystemContext | null>(null);
  const [formData, setFormData] = useState({ name: '', content: '' });
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [selectedAppIds, setSelectedAppIds] = useState<string[]>([]);
  
  const { dialogState, showConfirmDialog, handleConfirm, handleCancel } = useConfirmDialog();

  // 组件初始化时加载系统上下文列表（StrictMode 下防止重复触发）
  useEffect(() => {
    const key = '__systemContextEditorInitAt__';
    const last = (window as any)[key] || 0;
    const now = Date.now();
    if (typeof last === 'number' && now - last < 1000) {
      return;
    }
    (window as any)[key] = now;
    const loadContexts = async () => {
      setIsLoading(true);
      try {
        await loadSystemContexts();
        try { await (loadApplications?.()); } catch {}
      } catch (error) {
        console.error('Failed to load system contexts:', error);
      } finally {
        setIsLoading(false);
      }
    };
    loadContexts();
  }, [loadSystemContexts]);

  // 创建新上下文
  const handleCreate = () => {
    setFormData({ name: '', content: '' });
    setEditingContext(null);
    setViewMode('create');
  };

  // 编辑上下文
  const handleEdit = (context: SystemContext) => {
    setFormData({ name: context.name, content: context.content });
    setEditingContext(context);
    setSelectedAppIds(Array.isArray((context as any).app_ids) ? (context as any).app_ids : []);
    setViewMode('edit');
  };

  // 保存上下文
  const handleSave = async () => {
    if (!formData.name.trim() || !formData.content.trim()) {
      alert('请填写名称和内容');
      return;
    }

    setIsSaving(true);
    try {
      if (viewMode === 'create') {
        await createSystemContext(formData.name, formData.content, selectedAppIds);
      } else if (viewMode === 'edit' && editingContext) {
        await updateSystemContext(editingContext.id, formData.name, formData.content, selectedAppIds);
      }
      setViewMode('list');
    } catch (error) {
      console.error('Failed to save system context:', error);
      alert('保存失败，请重试');
    } finally {
      setIsSaving(false);
    }
  };

  // 删除上下文
  const handleDelete = async (context: SystemContext) => {
    showConfirmDialog({
      title: '删除确认',
      message: `确定要删除上下文 "${context.name}" 吗？此操作无法撤销。`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
      onConfirm: async () => {
        try {
          await deleteSystemContext(context.id);
        } catch (error) {
          console.error('Failed to delete system context:', error);
          alert('删除失败，请重试');
        }
      }
    });
  };

  // 激活上下文
  const handleActivate = async (context: SystemContext) => {
    try {
      await activateSystemContext(context.id);
      // 重新加载系统上下文列表以更新UI状态
      await loadSystemContexts();
    } catch (error) {
      console.error('Failed to activate system context:', error);
      alert('激活失败，请重试');
    }
  };

  // 处理键盘快捷键
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.ctrlKey && e.key === 's') {
      e.preventDefault();
      if (viewMode !== 'list') {
        handleSave();
      }
    }
  };

  // 渲染列表视图
  const renderListView = () => (
    <div className="p-6 overflow-hidden flex flex-col" style={{ height: 'calc(90vh - 120px)' }}>
      {/* 说明文字 */}
      <div className="mb-4 p-4 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg">
        <p className="text-sm text-blue-800 dark:text-blue-200">
          <strong>系统上下文</strong>将作为每次对话的系统提示词，支持 Markdown 格式。
          您可以创建多个上下文配置，但同一时间只能激活一个。
        </p>
      </div>

      {/* 上下文列表 */}
      <div className="flex-1 overflow-y-auto">
        {isLoading ? (
          <div className="flex items-center justify-center h-32">
            <div className="text-muted-foreground">加载中...</div>
          </div>
        ) : systemContexts.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-32 text-muted-foreground">
            <DocumentIcon />
            <p className="mt-2">暂无系统上下文配置</p>
            <p className="text-sm">点击"新建"按钮创建第一个配置</p>
          </div>
        ) : (
          <div className="space-y-3">
            {systemContexts.map((context: SystemContext) => (
              <div
                key={context.id}
                className={`p-4 border rounded-lg transition-colors ${
                  context.isActive
                    ? 'border-green-500 bg-green-50 dark:bg-green-900/20'
                    : 'border-border bg-card'
                }`}
              >
                <div className="flex items-start justify-between">
                  <div className="flex-1">
                    <div className="flex items-center space-x-2">
                      <h3 className="font-medium text-foreground">
                        {context.name}
                      </h3>
                      {context.isActive && (
                        <span className="inline-flex items-center px-2 py-1 text-xs font-medium bg-green-100 text-green-800 rounded-full dark:bg-green-800 dark:text-green-100">
                          <CheckIcon />
                          <span className="ml-1">已激活</span>
                        </span>
                      )}
                    </div>
                    <p className="mt-1 text-sm text-muted-foreground line-clamp-2">
                      {context.content.substring(0, 100)}
                      {context.content.length > 100 && '...'}
                    </p>
                    <p className="mt-2 text-xs text-muted-foreground">
                      创建时间: {new Date(context.createdAt).toLocaleString()}
                    </p>
                  </div>
                  <div className="flex items-center space-x-2 ml-4">
                    {!context.isActive && (
                      <button
                        onClick={() => handleActivate(context)}
                        className="px-3 py-1 text-sm bg-blue-600 text-white rounded hover:bg-blue-700 transition-colors"
                      >
                        激活
                      </button>
                    )}
                    <button
                      onClick={() => handleEdit(context)}
                      className="p-2 text-muted-foreground hover:text-blue-600 hover:bg-accent rounded transition-colors"
                    >
                      <EditIcon />
                    </button>
                    <button
                      onClick={() => handleDelete(context)}
                      className="p-2 text-muted-foreground hover:text-red-600 hover:bg-accent rounded transition-colors"
                    >
                      <TrashIcon />
                    </button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );

  // 渲染编辑视图
  const renderEditView = () => (
    <div className="p-6 overflow-hidden flex flex-col" style={{ height: 'calc(90vh - 120px)' }}>
      {/* 说明文字 */}
      <div className="mb-4 p-4 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg">
        <p className="text-sm text-blue-800 dark:text-blue-200">
          {viewMode === 'create' ? '创建新的系统上下文配置' : '编辑系统上下文配置'}
        </p>
        <p className="text-xs text-blue-600 dark:text-blue-300 mt-2">
          提示：使用 Ctrl+S 快速保存
        </p>
      </div>

      {/* 表单 */}
      <div className="flex-1 flex flex-col space-y-4">
        {/* 名称输入 */}
        <div>
          <label className="block text-sm font-medium text-foreground mb-2">
            配置名称
          </label>
          <input
            type="text"
            value={formData.name}
            onChange={(e) => setFormData({ ...formData, name: e.target.value })}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-lg focus:outline-none focus:ring-2 focus:ring-ring"
            placeholder="例如：编程助手、翻译专家、创意写作等"
          />
        </div>

        {/* 关联应用（多选） */}
        <div>
          <label className="block text-sm font-medium text-foreground mb-2">关联应用（多选）</label>
          <div className="space-y-2 max-h-32 overflow-y-auto p-2 border rounded-md">
            {(applications || []).map((app: any) => (
              <label key={app.id} className="flex items-center space-x-2">
                <input
                  type="checkbox"
                  checked={selectedAppIds.includes(app.id)}
                  onChange={(e) => {
                    const checked = e.target.checked;
                    setSelectedAppIds((prev) => (
                      checked ? [...prev, app.id] : prev.filter(id => id !== app.id)
                    ));
                  }}
                />
                <span>{app.name}</span>
              </label>
            ))}
            {(applications || []).length === 0 && (
              <div className="text-xs text-muted-foreground">暂无应用，可在“应用管理”中创建。</div>
            )}
          </div>
        </div>

        {/* 内容编辑 */}
        <div className="flex-1 flex flex-col">
          <div className="mb-2 flex items-center justify-between">
            <label className="block text-sm font-medium text-foreground">
              Markdown 内容
            </label>
            <div className="text-xs text-muted-foreground">
              {formData.content.length} 字符
            </div>
          </div>
          
          <textarea
            value={formData.content}
            onChange={(e) => setFormData({ ...formData, content: e.target.value })}
            onKeyDown={handleKeyDown}
            className="flex-1 w-full px-4 py-3 border border-input bg-background text-foreground rounded-lg focus:outline-none focus:ring-2 focus:ring-ring resize-none font-mono text-sm"
            placeholder="请输入系统上下文内容，支持 Markdown 格式...\n\n例如：\n# AI 助手角色设定\n\n你是一个专业的编程助手，具有以下特点：\n- 提供准确、简洁的代码解决方案\n- 遵循最佳实践和代码规范\n- 耐心解答技术问题\n\n## 回答风格\n- 使用中文回答\n- 代码示例要完整可运行\n- 提供必要的解释说明"
            spellCheck={false}
          />
        </div>

        {/* 底部提示 */}
        <div className="text-xs text-muted-foreground">
          <p>• 支持标准 Markdown 语法：标题、列表、代码块、链接等</p>
          <p>• 内容将在每次对话开始时自动发送给 AI</p>
          <p>• 留空则使用默认系统提示词</p>
        </div>
      </div>
    </div>
  );

  return (
    <div className="modal-container">
      <div className="modal-content w-full max-w-4xl max-h-[90vh] overflow-hidden">
        {/* 头部 */}
        <div className="flex items-center justify-between p-6 border-b border-border">
          <div className="flex items-center space-x-3">
            <DocumentIcon />
            <h2 className="text-xl font-semibold text-foreground">
              系统上下文设置
            </h2>
          </div>
          <div className="flex items-center space-x-2">
            {viewMode === 'list' ? (
              <button
                onClick={handleCreate}
                className="flex items-center space-x-2 px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 transition-colors"
              >
                <PlusIcon />
                <span>新建</span>
              </button>
            ) : (
              <>
                <button
                  onClick={handleSave}
                  disabled={isSaving}
                  className="flex items-center space-x-2 px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                >
                  <SaveIcon />
                  <span>{isSaving ? '保存中...' : '保存'}</span>
                </button>
                <button
                  onClick={() => setViewMode('list')}
                  className="px-4 py-2 text-secondary-foreground border border-border rounded-md hover:bg-accent focus:outline-none focus:ring-2 focus:ring-ring transition-colors"
                >
                  取消
                </button>
              </>
            )}
            <button
              onClick={onClose}
              className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            >
              <XMarkIcon />
            </button>
          </div>
        </div>

        {/* 内容区域 */}
        {viewMode === 'list' ? renderListView() : renderEditView()}
      </div>

      {/* 确认对话框 */}
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