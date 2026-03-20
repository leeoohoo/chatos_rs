import React from 'react';

import { cn } from '../../lib/utils';

interface SessionBusyBadgeProps {
  busy: boolean;
  idleText?: string;
  busyText?: string;
  className?: string;
}

const SessionBusyBadge: React.FC<SessionBusyBadgeProps> = ({
  busy,
  idleText = '空闲',
  busyText = '执行中',
  className,
}) => (
  <span
    className={cn(
      'inline-flex items-center gap-1',
      busy ? 'text-amber-600' : 'text-muted-foreground',
      className,
    )}
  >
    <span
      className={cn(
        'inline-block w-2 h-2 rounded-full',
        busy ? 'bg-amber-500' : 'bg-muted-foreground/40',
      )}
    />
    {busy ? busyText : idleText}
  </span>
);

export default SessionBusyBadge;
