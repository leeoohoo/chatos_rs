// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { UI_MESSAGES } from '../../i18n/messages';
import { cn } from '../../lib/utils';
import type { FsEntry } from '../../types';

interface DirectoryPickerPathDisplayProps {
  currentPath: string | null;
  emptyText: string;
  label?: string;
  className?: string;
  formatPath?: (path: string) => string;
}

export const DirectoryPickerPathDisplay: React.FC<DirectoryPickerPathDisplayProps> = ({
  currentPath,
  emptyText,
  label = UI_MESSAGES['zh-CN']['sessionList.picker.currentPathLabel'] || 'Current path: ',
  className,
  formatPath,
}) => {
  const displayPath = currentPath ? (formatPath?.(currentPath) || currentPath) : emptyText;

  return (
    <div
      className={cn('text-xs text-muted-foreground break-all', className)}
      title={displayPath}
    >
      {label}
      <span className="text-foreground">{displayPath}</span>
    </div>
  );
};

interface DirectoryPickerEntryListProps {
  loading: boolean;
  items: FsEntry[];
  emptyText: string;
  loadingText?: string;
  onOpenEntry: (path: string) => void;
  showFolderIcon?: boolean;
  className?: string;
  loadingClassName?: string;
  emptyClassName?: string;
  listClassName?: string;
  itemClassName?: string;
  nameClassName?: string;
  formatEntryName?: (entry: FsEntry) => string;
  formatEntryTitle?: (entry: FsEntry) => string;
}

export const DirectoryPickerEntryList: React.FC<DirectoryPickerEntryListProps> = ({
  loading,
  items,
  emptyText,
  loadingText = UI_MESSAGES['zh-CN']['common.loading'] || 'Loading...',
  onOpenEntry,
  showFolderIcon = false,
  className,
  loadingClassName,
  emptyClassName,
  listClassName,
  itemClassName,
  nameClassName,
  formatEntryName,
  formatEntryTitle,
}) => (
  <div className={className}>
    {loading && (
      <div className={cn('p-4 text-sm text-muted-foreground', loadingClassName)}>
        {loadingText}
      </div>
    )}
    {!loading && items.length === 0 && (
      <div className={cn('p-4 text-sm text-muted-foreground', emptyClassName)}>
        {emptyText}
      </div>
    )}
    {!loading && items.length > 0 && (
      <div className={listClassName}>
        {items.map((entry) => {
          const entryName = formatEntryName?.(entry) || entry.name;
          return (
            <button
              key={entry.path}
              type="button"
              onClick={() => onOpenEntry(entry.path)}
              title={formatEntryTitle?.(entry) || entryName}
              className={cn(
                'flex w-full items-center gap-2 text-left hover:bg-accent',
                itemClassName,
              )}
            >
              {showFolderIcon && (
                <svg
                  className="h-4 w-4 shrink-0 text-muted-foreground"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth={2}
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7z"
                  />
                </svg>
              )}
              <span className={cn('truncate text-foreground', nameClassName)}>
                {entryName}
              </span>
            </button>
          );
        })}
      </div>
    )}
  </div>
);

interface DirectoryPickerActionButtonProps {
  type?: 'button' | 'submit' | 'reset';
  disabled?: boolean;
  onClick?: () => void;
  children: React.ReactNode;
  className?: string;
}

export const DirectoryPickerActionButton: React.FC<DirectoryPickerActionButtonProps> = ({
  type = 'button',
  disabled = false,
  onClick,
  children,
  className,
}) => (
  <button
    type={type}
    onClick={onClick}
    disabled={disabled}
    className={cn(
      'rounded px-3 py-1.5 text-muted-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50',
      className,
    )}
  >
    {children}
  </button>
);
