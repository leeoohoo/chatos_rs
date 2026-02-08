import React, { useEffect, useState } from 'react';
import { useChatStoreFromContext } from '../lib/store/ChatStoreContext';
import { useChatStore } from '../lib/store';
import type { Application } from '../types';

interface ApplicationsPanelProps {
    isOpen?: boolean;
    onClose?: () => void;
    // 合并 ApplicationManager 逻辑：支持仅管理模式与自定义标题
    manageOnly?: boolean;
    title?: string;
    // 新增布局控制：embedded（嵌入左侧面板）或 modal（居中弹窗）
    layout?: 'embedded' | 'modal';
    // 应用选择回调：当用户点击应用时调用
    onApplicationSelect?: (app: Application) => void;
}

const ApplicationsPanel: React.FC<ApplicationsPanelProps> = ({ isOpen, onClose, manageOnly = false, title, layout = 'modal', onApplicationSelect }) => {
    let storeData: any;
    try {
        storeData = useChatStoreFromContext();
    } catch (e) {
        storeData = useChatStore();
    }

    const {
        applications,
        loadApplications,
        createApplication,
        updateApplication,
        deleteApplication,
    } = storeData;

    // 已移除 iframe 降级与选择逻辑，仅保留弹窗打开
    const [showManageMode, setShowManageMode] = useState(manageOnly ? true : false);
    const [showAddForm, setShowAddForm] = useState(false);
    const [editingId, setEditingId] = useState<string | null>(null);
    const [formData, setFormData] = useState<{ name: string; url: string; iconUrl: string }>(
        { name: '', url: '', iconUrl: '' }
    );

    useEffect(() => {
        if (layout === 'modal' && !isOpen) return;
        try {
            loadApplications?.();
        } catch (err) {
            console.error('[ApplicationsPanel] loadApplications error:', err);
        }
    }, [isOpen, layout, loadApplications]);

    const resetForm = () => {
        setEditingId(null);
        setFormData({ name: '', url: '', iconUrl: '' });
        setShowAddForm(false);
    };

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!formData.name.trim()) return;
        if (editingId) {
            await updateApplication?.(editingId, {
                name: formData.name.trim(),
                url: formData.url.trim(),
                iconUrl: formData.iconUrl.trim() || undefined,
            });
        } else {
            await createApplication?.(formData.name.trim(), formData.url.trim(), formData.iconUrl.trim() || undefined);
        }
        resetForm();
    };



    // 处理应用点击 - 调用外部提供的回调
    const handleAppClick = (app: any) => {
        // 如果提供了回调，调用它
        if (onApplicationSelect) {
            onApplicationSelect(app);
        }

        // 点击应用后自动关闭面板
        if (onClose) {
            onClose();
        }
    };

    // 已移除 iframe 错误监听与降级逻辑

    const shouldRender = layout === 'modal' ? !!isOpen : true;
    if (!shouldRender) return null;
    const effectiveManageMode = manageOnly ? true : showManageMode;
    const effectiveTitle = title ?? (effectiveManageMode ? '应用管理' : '应用列表');

    // modal 布局：保留原来的居中弹窗
    if (layout === 'modal') {
        return (
            <>
                <div className="fixed inset-0 bg-black/50 backdrop-blur-sm z-40" onClick={onClose} />
                <div className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-[90vw] max-w-4xl h-[85vh] bg-card z-50 shadow-xl breathing-border flex flex-col rounded-lg">
                    <div className="flex items-center justify-between p-4 border-b border-border">
                        <div className="flex items-center space-x-3">
                            <span className="inline-block w-2.5 h-2.5 rounded-full bg-blue-500" />
                            <h2 className="text-lg font-semibold text-foreground">{effectiveTitle}</h2>
                        </div>
                        <div className="flex items-center space-x-2">
                            {!manageOnly && (
                                <button
                                    onClick={() => {
                                        setShowManageMode(!showManageMode);
                                        setShowAddForm(false);
                                        resetForm();
                                    }}
                                    className="px-3 py-1.5 text-sm rounded bg-muted hover:bg-accent transition-colors"
                                >
                                    {effectiveManageMode ? '浏览模式' : '管理模式'}
                                </button>
                            )}
                            <button
                                onClick={onClose}
                                className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
                                title="关闭"
                            >
                                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12"/>
                                </svg>
                            </button>
                        </div>
                    </div>
                    <div className="flex-1 overflow-y-auto p-6">
                        {effectiveManageMode ? (
                            /* 管理模式 */
                            <div className="space-y-4">
                                {/* 添加按钮 */}
                                {!showAddForm && (
                                    <button
                                        type="button"
                                        onClick={() => setShowAddForm(true)}
                                        className="w-full p-4 border-2 border-dashed border-border rounded-lg hover:border-blue-500 transition-colors flex items-center justify-center space-x-2 text-muted-foreground hover:text-blue-600"
                                    >
                                        <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                                            <path strokeLinecap="round" strokeLinejoin="round" d="M12 6v12m6-6H6"/>
                                        </svg>
                                        <span>新增应用</span>
                                    </button>
                                )}

                                {/* 添加/编辑表单 */}
                                {showAddForm && (
                                    <form onSubmit={handleSubmit} className="p-4 bg-muted rounded-lg space-y-4">
                                        <div>
                                            <label className="block text-sm font-medium text-foreground mb-2">名称</label>
                                            <input
                                                type="text"
                                                value={formData.name}
                                                onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                                                className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                                                placeholder="例如：Jira、GitHub"
                                                required
                                            />
                                        </div>
                                        <div>
                                            <label className="block text-sm font-medium text-foreground mb-2">URL</label>
                                            <input
                                                type="text"
                                                value={formData.url}
                                                onChange={(e) => setFormData({ ...formData, url: e.target.value })}
                                                className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                                                placeholder="https://app.example.com"
                                            />
                                        </div>
                                        <div>
                                            <label className="block text-sm font-medium text-foreground mb-2">图标URL</label>
                                            <input
                                                type="text"
                                                value={formData.iconUrl}
                                                onChange={(e) => setFormData({ ...formData, iconUrl: e.target.value })}
                                                className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                                                placeholder="https://app.example.com/icon.png"
                                            />
                                        </div>
                                        <div className="flex items-center justify-end space-x-2">
                                            <button type="button" className="px-3 py-2 rounded bg-muted hover:bg-accent" onClick={resetForm}>取消</button>
                                            <button type="submit" className="px-3 py-2 rounded bg-blue-600 text-white hover:bg-blue-700">
                                                {editingId ? '保存' : '创建'}
                                            </button>
                                        </div>
                                    </form>
                                )}

                                {/* 应用列表（管理模式） */}
                                <div className="space-y-2">
                                    {applications?.map((app: any) => (
                                        <div key={app.id} className="flex items-center justify-between p-3 rounded border border-border hover:bg-muted transition-colors">
                                            <div className="flex items-center space-x-3">
                                                <div className="w-10 h-10 rounded-full bg-gradient-to-br from-blue-500 to-purple-500 flex items-center justify-center overflow-hidden shrink-0">
                                                    {app.iconUrl ? (
                                                        <img src={app.iconUrl} alt={app.name} className="w-full h-full object-cover"/>
                                                    ) : (
                                                        <span className="text-white text-sm font-bold">
                            {app.name.charAt(0).toUpperCase()}
                          </span>
                                                    )}
                                                </div>
                                                <div>
                                                    <div className="text-sm font-medium text-foreground">{app.name}</div>
                                                    {app.url && <div className="text-xs text-muted-foreground truncate max-w-md">{app.url}</div>}
                                                </div>
                                            </div>
                                            <div className="flex items-center space-x-2">
                                                <button
                                                    className="px-2 py-1 text-xs bg-muted rounded hover:bg-accent"
                                                    onClick={() => {
                                                        setEditingId(app.id);
                                                        setShowAddForm(true);
                                                        setFormData({ name: app.name, url: app.url || '', iconUrl: app.iconUrl || '' });
                                                    }}
                                                >编辑</button>
                                                <button
                                                    className="px-2 py-1 text-xs bg-destructive text-destructive-foreground rounded hover:bg-destructive/90"
                                                    onClick={() => deleteApplication?.(app.id)}
                                                >删除</button>
                                            </div>
                                        </div>
                                    ))}
                                    {applications?.length === 0 && (
                                        <div className="text-center py-12 text-muted-foreground">
                                            <svg className="w-16 h-16 mx-auto mb-3 opacity-30" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
                                            </svg>
                                            <div className="text-sm">暂无应用，点击上方按钮添加</div>
                                        </div>
                                    )}
                                </div>
                            </div>
                        ) : (
                            /* 浏览模式 - 圆形图标网格 */
                            <div className="grid grid-cols-4 md:grid-cols-5 lg:grid-cols-6 gap-6">
                                {applications?.map((app: any) => (
                                    <div key={app.id} className="relative group/item">
                                        <button
                                            className="w-full flex flex-col items-center space-y-2 p-2 rounded-lg transition-all hover:bg-muted"
                                            onClick={() => handleAppClick(app)}
                                            title={app.url || ''}
                                        >
                                            <div className="relative w-16 h-16 rounded-full flex items-center justify-center overflow-hidden transition-all bg-gradient-to-br from-blue-500/20 to-purple-500/20 group-hover/item:scale-105">
                                                {app.iconUrl ? (
                                                    <img src={app.iconUrl} alt={app.name} className="w-full h-full object-cover" />
                                                ) : (
                                                    <div className="w-full h-full bg-gradient-to-br from-blue-500 to-purple-500 flex items-center justify-center">
                          <span className="text-white text-xl font-bold">
                            {app.name.charAt(0).toUpperCase()}
                          </span>
                                                    </div>
                                                )}
                                            </div>
                                            <div className="text-xs font-medium text-foreground text-center truncate w-full px-1">
                                                {app.name}
                                            </div>
                                        </button>
                                    </div>
                                ))}
                                {(applications?.length ?? 0) === 0 && (
                                    <div className="col-span-full flex flex-col items-center justify-center py-20 text-center">
                                        <svg className="w-20 h-20 text-muted-foreground/30 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
                                        </svg>
                                        <div className="text-sm text-muted-foreground mb-2">暂无应用</div>
                                        <button
                                            onClick={() => setShowManageMode(true)}
                                            className="text-sm text-blue-500 hover:text-blue-600 underline"
                                        >
                                            点击切换到管理模式添加应用
                                        </button>
                                    </div>
                                )}
                            </div>
                        )}
                    </div>
                </div>
            </>
        );
    }

    // embedded 布局：用于左侧面板嵌入显示
    return (
        <div className="flex flex-col h-full">
            <div className="flex items-center justify-between p-3 border-b border-border bg-card/50">
                <div className="flex items-center space-x-2">
                    <span className="inline-block w-2 h-2 rounded-full bg-blue-500" />
                    <span className="text-sm font-medium text-foreground">{effectiveTitle}</span>
                </div>
                <div className="flex items-center space-x-2">
                    {!manageOnly && (
                        <button
                            onClick={() => {
                                setShowManageMode(!showManageMode);
                                setShowAddForm(false);
                                resetForm();
                            }}
                            className="px-2 py-1 text-xs rounded bg-muted hover:bg-accent transition-colors"
                        >
                            {effectiveManageMode ? '浏览模式' : '管理模式'}
                        </button>
                    )}
                    {onClose && (
                        <button
                            onClick={onClose}
                            className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-accent rounded transition-colors"
                            title="关闭"
                        >
                            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12"/>
                            </svg>
                        </button>
                    )}
                </div>
            </div>
            <div className="flex-1 overflow-y-auto p-4">
                {/* 复用原有内容区域（管理模式 / 浏览模式） */}
                {effectiveManageMode ? (
                    <>
                        {/* 添加/编辑表单开关 */}
                        <div className="mb-4 flex items-center justify-between">
                            <button
                                type="button"
                                onClick={() => {
                                    setEditingId(null);
                                    setFormData({ name: '', url: '', iconUrl: '' });
                                    setShowAddForm(s => !s);
                                }}
                                className="px-3 py-1.5 text-sm rounded bg-primary text-primary-foreground hover:opacity-90"
                            >{showAddForm ? '取消' : '新增应用'}</button>
                        </div>

                        {/* 添加/编辑表单 */}
                        {showAddForm && (
                            <form
                                onSubmit={(e) => {
                                    e.preventDefault();
                                    const name = formData.name?.trim() || '';
                                    const url = formData.url?.trim() || '';
                                    const iconUrl = (formData.iconUrl?.trim() || undefined);
                                    if (!name) return;
                                    if (editingId) {
                                        updateApplication?.(editingId, { name, url, iconUrl });
                                    } else {
                                        createApplication?.(name, url, iconUrl);
                                    }
                                    setShowAddForm(false);
                                    resetForm();
                                }}
                                className="p-4 bg-muted rounded-lg space-y-3"
                            >
                                <div>
                                    <label className="block text-xs font-medium text-foreground mb-1">名称</label>
                                    <input
                                        value={formData.name}
                                        onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                                        className="w-full px-2 py-1 text-sm rounded border border-border bg-background"
                                        placeholder="例如：飞书"
                                    />
                                </div>
                                <div>
                                    <label className="block text-xs font-medium text-foreground mb-1">URL</label>
                                    <input
                                        value={formData.url}
                                        onChange={(e) => setFormData({ ...formData, url: e.target.value })}
                                        className="w-full px-2 py-1 text-sm rounded border border-border bg-background"
                                        placeholder="https://example.com"
                                    />
                                </div>
                                <div>
                                    <label className="block text-xs font-medium text-foreground mb-1">图标地址</label>
                                    <input
                                        value={formData.iconUrl}
                                        onChange={(e) => setFormData({ ...formData, iconUrl: e.target.value })}
                                        className="w-full px-2 py-1 text-sm rounded border border-border bg-background"
                                        placeholder="https://.../icon.png"
                                    />
                                </div>
                                <div className="flex justify-end space-x-2">
                                    <button
                                        type="button"
                                        onClick={() => { setShowAddForm(false); resetForm(); setEditingId(null); }}
                                        className="px-3 py-1.5 text-sm rounded bg-muted hover:bg-accent"
                                    >取消</button>
                                    <button
                                        type="submit"
                                        className="px-3 py-1.5 text-sm rounded bg-primary text-primary-foreground hover:opacity-90"
                                    >{editingId ? '保存' : '创建'}</button>
                                </div>
                            </form>
                        )}

                        {/* 应用列表（管理模式） */}
                        <div className="space-y-2">
                            {applications?.map((app: any) => (
                                <div key={app.id} className="flex items-center justify-between p-3 rounded border border-border hover:bg-muted transition-colors">
                                    <div className="flex items-center space-x-3">
                                        <div className="w-10 h-10 rounded-full bg-gradient-to-br from-blue-500 to-purple-500 flex items-center justify-center overflow-hidden shrink-0">
                                            {app.iconUrl ? (
                                                <img src={app.iconUrl} alt={app.name} className="w-full h-full object-cover"/>
                                            ) : (
                                                <span className="text-white text-sm font-bold">
                          {app.name.charAt(0).toUpperCase()}
                        </span>
                                            )}
                                        </div>
                                        <div>
                                            <div className="text-sm font-medium text-foreground">{app.name}</div>
                                            {app.url && <div className="text-xs text-muted-foreground truncate max-w-md">{app.url}</div>}
                                        </div>
                                    </div>
                                    <div className="flex items-center space-x-2">
                                        <button
                                            className="px-2 py-1 text-xs rounded bg-background hover:bg-accent"
                                            onClick={() => { setShowAddForm(true); setEditingId(app.id); setFormData({ name: app.name || '', url: app.url || '', iconUrl: app.iconUrl || '' }); }}
                                        >编辑</button>
                                        <button
                                            className="px-2 py-1 text-xs rounded bg-destructive text-destructive-foreground hover:opacity-90"
                                            onClick={() => deleteApplication?.(app.id)}
                                        >删除</button>
                                    </div>
                                </div>
                            ))}
                            {(applications?.length ?? 0) === 0 && (
                                <div className="flex flex-col items-center justify-center py-10 text-center">
                                    <svg className="w-16 h-16 text-muted-foreground/30 mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
                                    </svg>
                                    <div className="text-sm text-muted-foreground mb-2">暂无应用，点击上方按钮添加。</div>
                                </div>
                            )}
                        </div>
                    </>
                ) : (
                    <div className="grid grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
                        {applications?.map((app: any) => (
                            <div key={app.id} className="relative group/item">
                                <button
                                    className="w-full flex flex-col items-center space-y-2 p-2 rounded-lg transition-all hover:bg-muted"
                                    onClick={() => handleAppClick(app)}
                                    title={app.url || ''}
                                >
                                    <div className="relative w-14 h-14 rounded-full flex items-center justify-center overflow-hidden transition-all bg-gradient-to-br from-blue-500/20 to-purple-500/20 group-hover/item:scale-105">
                                        {app.iconUrl ? (
                                            <img src={app.iconUrl} alt={app.name} className="w-full h-full object-cover" />
                                        ) : (
                                            <div className="w-full h-full bg-gradient-to-br from-blue-500 to-purple-500 flex items-center justify-center">
                          <span className="text-white text-lg font-bold">
                            {app.name.charAt(0).toUpperCase()}
                          </span>
                                            </div>
                                        )}
                                    </div>
                                    <div className="text-xs font-medium text-foreground truncate max-w-full">{app.name}</div>
                                    {app.url && <div className="text-[10px] text-muted-foreground truncate max-w-full">{app.url}</div>}
                                </button>
                            </div>
                        ))}
                        {(applications?.length ?? 0) === 0 && (
                            <div className="col-span-full flex flex-col items-center justify-center py-10 text-center">
                                <svg className="w-16 h-16 text-muted-foreground/30 mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
                                </svg>
                                <div className="text-sm text-muted-foreground mb-2">暂无应用</div>
                                <button
                                    onClick={() => setShowManageMode(true)}
                                    className="text-sm text-blue-500 hover:text-blue-600 underline"
                                >
                                    切换到管理模式以添加应用
                                </button>
                            </div>
                        )}
                    </div>
                )}
            </div>
        </div>
    );
};

export default ApplicationsPanel;
