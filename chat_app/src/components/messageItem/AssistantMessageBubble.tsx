import React, { useEffect, useRef, useState } from 'react';
import { cn } from '../../lib/utils';
import { useI18n } from '../../i18n/I18nProvider';

const ASSISTANT_BUBBLE_COLLAPSED_HEIGHT = 520;

interface AssistantMessageBubbleProps {
  children: React.ReactNode;
  messageId: string;
}

export const AssistantMessageBubble: React.FC<AssistantMessageBubbleProps> = ({
  children,
  messageId,
}) => {
  const { t } = useI18n();
  const contentRef = useRef<HTMLDivElement | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [canToggle, setCanToggle] = useState(false);

  useEffect(() => {
    setExpanded(false);
  }, [messageId]);

  useEffect(() => {
    const node = contentRef.current;
    if (!node) {
      return undefined;
    }

    const updateOverflow = () => {
      const hasOverflow = node.scrollHeight > ASSISTANT_BUBBLE_COLLAPSED_HEIGHT + 8;
      setCanToggle(hasOverflow);
    };

    updateOverflow();
    const observer = new ResizeObserver(updateOverflow);
    observer.observe(node);
    return () => observer.disconnect();
  }, [children]);

  return (
    <div className="relative rounded-lg border border-border bg-card/70 shadow-sm">
      <div
        ref={contentRef}
        className={cn(
          'overflow-hidden px-5 py-4 sm:px-6',
          canToggle && !expanded && 'max-h-[520px]',
        )}
      >
        {children}
      </div>
      {canToggle && !expanded ? (
        <div className="pointer-events-none absolute inset-x-0 bottom-10 h-16 bg-gradient-to-t from-card/95 to-transparent" />
      ) : null}
      {canToggle ? (
        <div className="border-t border-border bg-card/90 px-5 py-2 sm:px-6">
          <button
            type="button"
            className="text-xs font-medium text-primary hover:text-primary/80"
            onClick={() => setExpanded((value) => !value)}
            aria-expanded={expanded}
          >
            {expanded ? t('taskDraft.collapse') : t('taskDraft.expand')}
          </button>
        </div>
      ) : null}
    </div>
  );
};
