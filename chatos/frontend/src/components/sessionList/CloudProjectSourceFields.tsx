// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';

export type CloudProjectSourceMethod = 'git' | 'zip' | 'empty';

interface CloudProjectSourceFieldsProps {
  projectName: string;
  gitUrl: string;
  zipFile: File | null;
  sourceMethod: CloudProjectSourceMethod;
  onProjectNameChange: (value: string) => void;
  onGitUrlChange: (value: string) => void;
  onZipFileChange: (value: File | null) => void;
  onSourceMethodChange: (value: CloudProjectSourceMethod) => void;
}

export const CloudProjectSourceFields: React.FC<CloudProjectSourceFieldsProps> = ({
  projectName,
  gitUrl,
  zipFile,
  sourceMethod,
  onProjectNameChange,
  onGitUrlChange,
  onZipFileChange,
  onSourceMethodChange,
}) => {
  const { t } = useI18n();

  return (
    <div className="space-y-3">
      <label className="block text-sm text-muted-foreground">
        {t('sessionList.resource.cloudProjectName')}
        <input
          value={projectName}
          onChange={(event) => onProjectNameChange(event.target.value)}
          className="mt-1 w-full rounded border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          placeholder={t('sessionList.resource.cloudProjectNamePlaceholder')}
          autoFocus
        />
      </label>
      <label className="block text-sm text-muted-foreground">
        {t('sessionList.resource.cloudProjectSourceMethod')}
        <select
          value={sourceMethod}
          onChange={(event) => onSourceMethodChange(event.target.value as CloudProjectSourceMethod)}
          className="mt-1 w-full rounded border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
        >
          <option value="git">{t('sessionList.resource.cloudProjectSourceGit')}</option>
          <option value="zip">{t('sessionList.resource.cloudProjectSourceZip')}</option>
          <option value="empty">{t('sessionList.resource.cloudProjectSourceEmpty')}</option>
        </select>
      </label>
      {sourceMethod === 'git' ? (
        <label className="block text-sm text-muted-foreground">
          {t('sessionList.resource.cloudProjectGitUrl')}
          <input
            value={gitUrl}
            onChange={(event) => onGitUrlChange(event.target.value)}
            className="mt-1 w-full rounded border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
            placeholder="https://github.com/org/repo.git"
          />
        </label>
      ) : null}
      {sourceMethod === 'zip' ? (
        <div className="space-y-1">
          <label className="block text-sm text-muted-foreground" htmlFor="cloud-project-zip">
            {t('sessionList.resource.cloudProjectZip')}
          </label>
          <input
            id="cloud-project-zip"
            type="file"
            accept=".zip,application/zip,application/x-zip-compressed"
            onChange={(event) => onZipFileChange(event.target.files?.[0] || null)}
            className="w-full rounded border border-border bg-background px-3 py-2 text-sm text-foreground file:mr-3 file:rounded file:border-0 file:bg-muted file:px-3 file:py-1.5 file:text-sm file:text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          />
          {zipFile ? (
            <div className="text-xs text-muted-foreground">
              {t('sessionList.resource.cloudProjectZipSelected', { name: zipFile.name })}
            </div>
          ) : null}
        </div>
      ) : null}
      <div className="rounded border border-border bg-background px-3 py-2 text-xs text-muted-foreground">
        {sourceMethod === 'zip'
          ? t('sessionList.resource.cloudProjectZipHint')
          : sourceMethod === 'git'
            ? t('sessionList.resource.cloudProjectGitHint')
            : t('sessionList.resource.cloudProjectEmptyHint')}
      </div>
    </div>
  );
};
