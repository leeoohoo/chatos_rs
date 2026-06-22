import { useI18n } from '../../i18n/I18nProvider';
import { formatFileSize } from '../../lib/utils';

interface InputAreaDragOverlayProps {
  maxFileBytes: number;
  maxTotalBytes: number;
  maxAttachments: number;
}

export default function InputAreaDragOverlay({
  maxFileBytes,
  maxTotalBytes,
  maxAttachments,
}: InputAreaDragOverlayProps) {
  const { t } = useI18n();

  return (
    <div className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center rounded-lg border-2 border-dashed border-primary bg-primary/10">
      <div className="text-center">
        <svg className="w-8 h-8 mx-auto text-primary mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12" />
        </svg>
        <p className="text-sm font-medium text-primary">{t('inputArea.drag.dropHere')}</p>
        <p className="text-[11px] text-muted-foreground mt-1">
          {t('inputArea.drag.limit', {
            fileLimit: formatFileSize(maxFileBytes),
            totalLimit: formatFileSize(maxTotalBytes),
            maxCount: maxAttachments,
          })}
        </p>
      </div>
    </div>
  );
}
