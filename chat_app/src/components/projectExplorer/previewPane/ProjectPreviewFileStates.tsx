import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import type { FsReadResult } from '../../../types';

export const ProjectPreviewLoadingState: React.FC = () => {
  const { t } = useI18n();
  return <div className="p-4 text-sm text-muted-foreground">{t('projectExplorer.preview.loading')}</div>;
};

export const ProjectPreviewDeletedPathState: React.FC = () => {
  const { t } = useI18n();
  return (
    <div className="p-4 text-sm text-muted-foreground">
      {t('projectExplorer.preview.deleted')}
    </div>
  );
};

export const ProjectPreviewEmptyState: React.FC = () => {
  const { t } = useI18n();
  return <div className="p-4 text-sm text-muted-foreground">{t('projectExplorer.preview.empty')}</div>;
};

export const ProjectPreviewImageContent: React.FC<{
  selectedFile: FsReadResult;
}> = ({ selectedFile }) => {
  const src = `data:${selectedFile.contentType};base64,${selectedFile.content}`;
  return (
    <div className="h-full overflow-auto p-4">
      <img
        src={src}
        alt={selectedFile.name}
        className="max-h-full max-w-full rounded border border-border"
      />
    </div>
  );
};

export const ProjectPreviewBinaryDownload: React.FC<{
  selectedFile: FsReadResult;
}> = ({ selectedFile }) => {
  const { t } = useI18n();
  const downloadHref = `data:${selectedFile.contentType};base64,${selectedFile.content}`;
  return (
    <div className="space-y-2 p-4 text-sm text-muted-foreground">
      <div>{t('projectExplorer.preview.binary')}</div>
      <a
        href={downloadHref}
        download={selectedFile.name || 'file'}
        className="inline-flex items-center rounded bg-primary px-3 py-1.5 text-primary-foreground transition-colors hover:bg-primary/90"
      >
        {t('projectExplorer.preview.downloadFile')}
      </a>
    </div>
  );
};
