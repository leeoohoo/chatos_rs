import React from 'react';

import { cn } from '../../lib/utils';
import { useI18n } from '../../i18n/I18nProvider';

interface SessionBusyBadgeProps {
  phase?: 'thinking' | 'reviewing' | null;
  busy?: boolean;
  idleText?: string;
  thinkingText?: string;
  reviewingText?: string;
  className?: string;
}

const SessionBusyBadge: React.FC<SessionBusyBadgeProps> = ({
  phase = null,
  busy = false,
  idleText = '空闲',
  thinkingText,
  reviewingText,
  className,
}) => {
  const { t } = useI18n();
  const resolvedPhase: 'thinking' | 'reviewing' | null = (
    phase === 'thinking' || phase === 'reviewing'
      ? phase
      : (busy ? 'thinking' : null)
  );

  const label = resolvedPhase === 'reviewing'
    ? (reviewingText || t('sessionStatus.reviewing'))
    : (resolvedPhase === 'thinking'
      ? (thinkingText || t('sessionStatus.thinking'))
      : idleText);
  const textClassName = resolvedPhase === 'reviewing'
    ? 'text-blue-600'
    : (resolvedPhase === 'thinking' ? 'text-amber-600' : 'text-muted-foreground');
  const dotClassName = resolvedPhase === 'reviewing'
    ? 'bg-blue-500'
    : (resolvedPhase === 'thinking' ? 'bg-amber-500' : 'bg-muted-foreground/40');

  return (
    <span
      className={cn(
        'inline-flex items-center gap-1',
        textClassName,
        className,
      )}
    >
      <span
        className={cn(
          'inline-block w-2 h-2 rounded-full',
          dotClassName,
        )}
      />
      {label}
    </span>
  );
};

export default SessionBusyBadge;
