// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { getUserVisiblePath } from '../../lib/domain/filesystem';
import {
  DirectoryPickerActionButton,
  DirectoryPickerPathDisplay,
} from '../ui/DirectoryPickerShared';
import type { FsEntry } from '../../types';

interface KeyFilePickerDialogProps {
  isOpen: boolean;
  title: string;
  currentPath: string | null;
  parentPath: string | null;
  loading: boolean;
  items: FsEntry[];
  error: string | null;
  onClose: () => void;
  onBack: () => void;
  onRefresh: () => void;
  onEntryClick: (entry: FsEntry) => void;
  onSelectFile: (path: string) => void;
}

export const KeyFilePickerDialog: React.FC<KeyFilePickerDialogProps> = ({
  isOpen,
  title,
  currentPath,
  parentPath,
  loading,
  items,
  error,
  onClose,
  onBack,
  onRefresh,
  onEntryClick,
  onSelectFile,
}) => {
  const { t } = useI18n();
  const formatPath = React.useCallback((path: string) => getUserVisiblePath(path), []);

  if (!isOpen) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-[80] flex items-center justify-center">
      <div className="fixed inset-0 bg-black/50" onClick={onClose} />
      <div className="relative bg-card border border-border rounded-lg shadow-xl w-[680px] max-h-[82vh] p-6 flex flex-col">
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-lg font-semibold text-foreground">{title}</h3>
          <button
            onClick={onClose}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
          >
            <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <DirectoryPickerPathDisplay
          currentPath={currentPath}
          emptyText={t('sessionList.picker.chooseDiskOrDirectory')}
          label={t('sessionList.picker.currentPathLabel')}
          formatPath={formatPath}
        />
        <div className="mt-3 flex items-center gap-2">
          <DirectoryPickerActionButton
            onClick={onBack}
            disabled={!parentPath}
            className="bg-muted"
          >
            {t('sessionList.picker.backParent')}
          </DirectoryPickerActionButton>
          <DirectoryPickerActionButton
            onClick={onRefresh}
            className="bg-muted"
          >
            {t('common.refresh')}
          </DirectoryPickerActionButton>
        </div>
        <div className="mt-3 flex-1 overflow-y-auto border border-border rounded">
          {loading && <div className="p-4 text-sm text-muted-foreground">{t('common.loading')}</div>}
          {!loading && items.length === 0 && (
            <div className="p-4 text-sm text-muted-foreground">{t('sessionList.picker.noFiles')}</div>
          )}
          {!loading && items.length > 0 && (
            <div className="divide-y divide-border">
              {items.map((entry) => (
                <div
                  key={entry.path}
                  className="px-4 py-2 hover:bg-accent flex items-center justify-between gap-3"
                >
                  <button
                    type="button"
                    onClick={() => onEntryClick(entry)}
                    className="flex-1 text-left"
                  >
                    <span className="text-foreground truncate block">
                      {entry.isDir ? '📁' : '🔑'} {entry.name || entry.path}
                    </span>
                    <span className="text-[11px] text-muted-foreground truncate block">
                      {entry.displayPath || formatPath(entry.path)}
                    </span>
                  </button>
                  {!entry.isDir && (
                    <button
                      type="button"
                      onClick={() => onSelectFile(entry.path)}
                      className="px-2.5 py-1 rounded border border-border text-xs text-foreground hover:bg-accent"
                    >
                      {t('common.select')}
                    </button>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
        {error && <div className="mt-2 text-xs text-destructive">{error}</div>}
      </div>
    </div>
  );
};
