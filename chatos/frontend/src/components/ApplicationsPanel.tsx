// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useEffect, useState } from 'react';

import { useI18n } from '../i18n/I18nProvider';
import { useChatStoreResolved } from '../lib/store/ChatStoreContext';
import type { Application } from '../types';
import ApplicationsBrowseView from './applicationsPanel/ApplicationsBrowseView';
import ApplicationsManageView from './applicationsPanel/ApplicationsManageView';
import ManagerFormDialog from './ui/ManagerFormDialog';
import {
  canSubmitApplicationForm,
  getDefaultApplicationFormData,
  toApplicationFormData,
} from './applicationsPanel/helpers';
import { XMarkIcon } from './applicationsPanel/icons';
import type {
  ApplicationFormData,
  ApplicationPanelStore,
  ApplicationsPanelProps,
} from './applicationsPanel/types';

const ApplicationsPanel: React.FC<ApplicationsPanelProps> = ({
  isOpen,
  onClose,
  manageOnly = false,
  title,
  layout = 'modal',
  onApplicationSelect,
}) => {
    const { t } = useI18n();
    const storeData: ApplicationPanelStore = useChatStoreResolved();

    const {
        applications,
        loadApplications,
        createApplication,
        updateApplication,
        deleteApplication,
    } = storeData;

    // 已移除 iframe 降级与选择逻辑，仅保留弹窗打开
    const [showManageMode, setShowManageMode] = useState(manageOnly ? true : false);
    const [isFormDialogOpen, setIsFormDialogOpen] = useState(false);
    const [editingId, setEditingId] = useState<string | null>(null);
    const [formData, setFormData] = useState<ApplicationFormData>(getDefaultApplicationFormData());

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
        setFormData(getDefaultApplicationFormData());
        setIsFormDialogOpen(false);
    };

    const openCreateDialog = () => {
        setEditingId(null);
        setFormData(getDefaultApplicationFormData());
        setIsFormDialogOpen(true);
    };

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!canSubmitApplicationForm(formData)) return;
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

    const handleAppClick = (app: Application) => {
        if (onApplicationSelect) {
            onApplicationSelect(app);
        }
        if (onClose) {
            onClose();
        }
    };

    const handleEditApp = (app: Application) => {
        setEditingId(app.id);
        setFormData(toApplicationFormData(app));
        setIsFormDialogOpen(true);
    };

    const handleToggleManageMode = () => {
        setShowManageMode(!showManageMode);
        resetForm();
    };

    const handleFormDataChange = (patch: Partial<ApplicationFormData>) => {
        setFormData((current) => ({
            ...current,
            ...patch,
        }));
    };

    const shouldRender = layout === 'modal' ? !!isOpen : true;
    if (!shouldRender) return null;
    const effectiveManageMode = manageOnly ? true : showManageMode;
    const effectiveTitle = title ?? (effectiveManageMode ? t('applications.manageTitle') : t('applications.title'));
    const formDialog = (
        <ManagerFormDialog
            open={isFormDialogOpen}
            title={editingId ? t('applications.form.titleEdit') : t('applications.form.titleCreate')}
            widthClassName="max-w-lg"
            onClose={resetForm}
        >
            <form onSubmit={(event) => void handleSubmit(event)} className="space-y-4">
                <div className="space-y-4 rounded-xl border border-border bg-muted/40 p-4">
                    <div>
                        <label className="mb-2 block text-sm font-medium text-foreground">
                            {t('applications.form.name')}
                        </label>
                        <input
                            type="text"
                            value={formData.name}
                            onChange={(event) => handleFormDataChange({ name: event.target.value })}
                            className="w-full rounded-md border border-input bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                            placeholder={t('applications.form.namePlaceholder')}
                            autoFocus
                            required
                        />
                    </div>
                    <div>
                        <label className="mb-2 block text-sm font-medium text-foreground">
                            {t('applications.form.url')}
                        </label>
                        <input
                            type="text"
                            value={formData.url}
                            onChange={(event) => handleFormDataChange({ url: event.target.value })}
                            className="w-full rounded-md border border-input bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                            placeholder={t('applications.form.urlPlaceholder')}
                        />
                    </div>
                    <div>
                        <label className="mb-2 block text-sm font-medium text-foreground">
                            {t('applications.form.iconUrl')}
                        </label>
                        <input
                            type="text"
                            value={formData.iconUrl}
                            onChange={(event) => handleFormDataChange({ iconUrl: event.target.value })}
                            className="w-full rounded-md border border-input bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                            placeholder={t('applications.form.iconUrlPlaceholder')}
                        />
                    </div>
                </div>
                <div className="flex items-center justify-end space-x-2">
                    <button
                        type="button"
                        className="rounded-lg bg-muted px-3 py-2 text-sm transition-colors hover:bg-accent"
                        onClick={resetForm}
                    >
                        {t('common.cancel')}
                    </button>
                    <button
                        type="submit"
                        className="rounded-lg bg-primary px-3 py-2 text-sm text-primary-foreground transition-opacity hover:opacity-90"
                    >
                        {editingId ? t('applications.form.submitEdit') : t('applications.form.submitCreate')}
                    </button>
                </div>
            </form>
        </ManagerFormDialog>
    );

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
                                    onClick={handleToggleManageMode}
                                    className="px-3 py-1.5 text-sm rounded bg-muted hover:bg-accent transition-colors"
                                >
                                    {effectiveManageMode ? t('applications.mode.browse') : t('applications.mode.manage')}
                                </button>
                            )}
                            <button
                                onClick={onClose}
                                className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
                                title={t('common.close')}
                            >
                                <XMarkIcon />
                            </button>
                        </div>
                    </div>
                    <div className="flex-1 overflow-y-auto p-6">
                        {effectiveManageMode ? (
                            <ApplicationsManageView
                                applications={applications || []}
                                onCreate={openCreateDialog}
                                onEdit={handleEditApp}
                                onDelete={async (id) => deleteApplication?.(id)}
                            />
                        ) : (
                            <ApplicationsBrowseView
                                applications={applications || []}
                                onApplicationSelect={handleAppClick}
                                onSwitchToManageMode={() => setShowManageMode(true)}
                            />
                        )}
                    </div>
                </div>
                {formDialog}
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
                            onClick={handleToggleManageMode}
                            className="px-2 py-1 text-xs rounded bg-muted hover:bg-accent transition-colors"
                        >
                            {effectiveManageMode ? t('applications.mode.browse') : t('applications.mode.manage')}
                        </button>
                    )}
                    {onClose && (
                        <button
                            onClick={onClose}
                            className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-accent rounded transition-colors"
                            title={t('common.close')}
                        >
                            <XMarkIcon className="w-4 h-4" />
                        </button>
                    )}
                </div>
            </div>
            <div className="flex-1 overflow-y-auto p-4">
                {effectiveManageMode ? (
                    <ApplicationsManageView
                        applications={applications || []}
                        compact
                        onCreate={openCreateDialog}
                        onEdit={handleEditApp}
                        onDelete={async (id) => deleteApplication?.(id)}
                    />
                ) : (
                    <ApplicationsBrowseView
                        applications={applications || []}
                        compact
                        onApplicationSelect={handleAppClick}
                        onSwitchToManageMode={() => setShowManageMode(true)}
                    />
                )}
            </div>
            {formDialog}
        </div>
    );
};

export default ApplicationsPanel;
