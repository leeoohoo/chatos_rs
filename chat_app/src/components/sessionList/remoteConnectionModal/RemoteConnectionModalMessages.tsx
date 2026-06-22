import type { FC } from 'react';
import { useI18n } from '../../../i18n/I18nProvider';

interface RemoteConnectionModalMessagesProps {
  remoteError: string | null;
  remoteErrorAction: string | null;
  remoteSuccess: string | null;
}

export const RemoteConnectionModalMessages: FC<RemoteConnectionModalMessagesProps> = ({
  remoteError,
  remoteErrorAction,
  remoteSuccess,
}) => {
  const { t } = useI18n();

  return (
    <>
      {remoteError && (
        <div className="rounded border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {remoteError}
        </div>
      )}
      {remoteErrorAction && (
        <div className="rounded border border-amber-400/40 bg-amber-50 px-3 py-2 text-xs text-amber-800 dark:bg-amber-950/30 dark:text-amber-200">
          <div className="mb-1 text-[11px] font-semibold uppercase tracking-wide text-amber-700 dark:text-amber-300">
            {t('remoteConnection.suggestedAction')}
          </div>
          <div>{remoteErrorAction}</div>
        </div>
      )}
      {remoteSuccess && <div className="text-xs text-emerald-600">{remoteSuccess}</div>}
    </>
  );
};
