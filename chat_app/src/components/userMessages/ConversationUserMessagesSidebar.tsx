import React, { useEffect, useRef, useState } from 'react';
import { MessageSquareText, RefreshCw } from 'lucide-react';

import { useI18n } from '../../i18n/I18nProvider';
import { cn } from '../../lib/utils';
import type { Message } from '../../types';
import { ConversationUserMessageItem } from './ConversationUserMessageItem';
import { useConversationUserMessages } from './useConversationUserMessages';

interface ConversationUserMessagesSidebarProps {
  sessionId: string | null | undefined;
  contactName?: string | null;
  className?: string;
  onSelectMessage?: (message: Message) => void;
  onLoadMoreHistory?: (oldestLoadedMessage: Message | null) => void | Promise<void>;
  onOpenTasks: (message: Message) => void;
}

const selectOldestLoadedMessage = (loadedItems: Array<{ userMessage: Message }>): Message | null => (
  loadedItems.reduce<Message | null>((oldest, item) => {
    const candidate = item.userMessage;
    if (!oldest) {
      return candidate;
    }
    const candidateTime = candidate.createdAt instanceof Date ? candidate.createdAt.getTime() : Number.NaN;
    const oldestTime = oldest.createdAt instanceof Date ? oldest.createdAt.getTime() : Number.NaN;
    if (Number.isNaN(candidateTime)) {
      return oldest;
    }
    if (Number.isNaN(oldestTime)) {
      return candidate;
    }
    return candidateTime < oldestTime ? candidate : oldest;
  }, null)
);

const ConversationUserMessagesSidebar: React.FC<ConversationUserMessagesSidebarProps> = ({
  sessionId,
  contactName,
  className,
  onSelectMessage,
  onLoadMoreHistory,
  onOpenTasks,
}) => {
  const { t } = useI18n();
  const {
    items,
    loading,
    loadingMore,
    error,
    hasMore,
    reload,
    loadMore,
  } = useConversationUserMessages(sessionId);
  const [selectedMessageId, setSelectedMessageId] = useState<string | null>(null);
  const [syncingHistory, setSyncingHistory] = useState(false);
  const listScrollRef = useRef<HTMLDivElement | null>(null);
  const loadingOlderRef = useRef(false);

  useEffect(() => {
    setSelectedMessageId(null);
  }, [sessionId]);

  useEffect(() => {
    if (selectedMessageId && items.some((item) => item.userMessage.id === selectedMessageId)) {
      return;
    }
    setSelectedMessageId(items[items.length - 1]?.userMessage.id || null);
  }, [items, selectedMessageId]);

  useEffect(() => {
    if (items.length === 0) {
      return;
    }
    if (loadingOlderRef.current) {
      loadingOlderRef.current = false;
      return;
    }
    const frame = window.requestAnimationFrame(() => {
      const node = listScrollRef.current;
      if (node) {
        node.scrollTop = node.scrollHeight;
      }
    });
    return () => window.cancelAnimationFrame(frame);
  }, [items, sessionId]);

  return (
    <aside className={cn('flex shrink-0 flex-col border-r border-border bg-background', className)}>
      <div className="border-b border-border px-4 py-3">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <h2 className="text-sm font-semibold text-foreground">{t('projectUserMessages.title')}</h2>
            <p className="mt-0.5 truncate text-xs text-muted-foreground">
              {contactName
                ? t('projectUserMessages.descriptionWithContact', { name: contactName })
                : t('projectUserMessages.description')}
            </p>
          </div>
          <button
            type="button"
            className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-60"
            disabled={!sessionId || loading}
            onClick={reload}
            aria-label={t('projectUserMessages.refresh')}
            title={t('projectUserMessages.refresh')}
          >
            <RefreshCw className={cn('h-4 w-4', loading && 'animate-spin')} />
          </button>
        </div>
        {error ? (
          <div className="mt-3 rounded-md border border-destructive/30 bg-destructive/10 px-2 py-2 text-xs text-destructive">
            {error}
          </div>
        ) : null}
      </div>

      <div ref={listScrollRef} className="min-h-0 flex-1 overflow-y-auto">
        {!sessionId ? (
          <div className="px-2 py-8 text-center">
            <div className="mx-auto flex h-10 w-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
              <MessageSquareText className="h-5 w-5" />
            </div>
            <div className="mt-3 text-sm font-medium text-foreground">
              {t('projectUserMessages.noContactTitle')}
            </div>
            <div className="mt-1 text-xs leading-5 text-muted-foreground">
              {t('projectUserMessages.noContactDescription')}
            </div>
          </div>
        ) : loading && items.length === 0 ? (
          <div className="px-4 py-3 text-xs text-muted-foreground">
            {t('projectUserMessages.loading')}
          </div>
        ) : items.length === 0 ? (
          <div className="px-2 py-8 text-center">
            <div className="mx-auto flex h-10 w-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
              <MessageSquareText className="h-5 w-5" />
            </div>
            <div className="mt-3 text-sm font-medium text-foreground">
              {t('projectUserMessages.emptyTitle')}
            </div>
            <div className="mt-1 text-xs leading-5 text-muted-foreground">
              {t('projectUserMessages.emptyDescription')}
            </div>
          </div>
        ) : (
          <div className="divide-y divide-border/70 border-b border-border/70">
            {hasMore ? (
              <button
                type="button"
                className="w-full px-4 py-2.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-60"
                disabled={loadingMore || syncingHistory}
                onClick={() => {
                  loadingOlderRef.current = true;
                  setSyncingHistory(true);
                  void loadMore()
                    .then((loadedItems) => {
                      const oldestLoadedMessage = selectOldestLoadedMessage(loadedItems);
                      if (!oldestLoadedMessage) {
                        loadingOlderRef.current = false;
                      }
                      return onLoadMoreHistory?.(oldestLoadedMessage);
                    })
                    .finally(() => {
                      setSyncingHistory(false);
                    });
                }}
              >
                {loadingMore || syncingHistory ? t('projectUserMessages.loadingMore') : t('projectUserMessages.loadMore')}
              </button>
            ) : null}
            {items.map((item) => (
              <ConversationUserMessageItem
                key={item.turnId || item.userMessage.id}
                item={item}
                active={selectedMessageId === item.userMessage.id}
                onSelect={() => {
                  setSelectedMessageId(item.userMessage.id);
                  onSelectMessage?.(item.userMessage);
                }}
                onOpenTasks={onOpenTasks}
              />
            ))}
          </div>
        )}
      </div>
    </aside>
  );
};

export default ConversationUserMessagesSidebar;
