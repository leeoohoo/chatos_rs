import { useI18n } from '../../i18n/I18nProvider';
import type { AgentConversationState } from './types';
import { formatMessageTime, getMessageRoleLabel } from './sessionHelpers';

interface AgentConversationPanelProps {
  state: AgentConversationState;
  onClose: () => void;
  onSelectSession: (sessionId: string) => Promise<void>;
}

const AgentConversationPanel = ({
  state,
  onClose,
  onSelectSession,
}: AgentConversationPanelProps) => {
  const { t } = useI18n();
  if (!state.open) {
    return null;
  }

  return (
    <>
      <div className="fixed inset-0 z-[70] bg-black/40" onClick={onClose} />
      <div className="fixed right-0 top-0 z-[71] h-full w-full max-w-6xl border-l border-border bg-card shadow-2xl">
        <div className="flex h-full flex-col">
          <div className="flex items-center justify-between border-b border-border px-5 py-4">
            <div className="min-w-0">
              <div className="text-base font-semibold text-foreground">{t('agentManager.conversation.title')}</div>
              <div className="truncate text-sm text-muted-foreground">
                {state.agent?.name || '-'}
              </div>
            </div>
            <button
              onClick={onClose}
              className="rounded-lg bg-muted px-3 py-2 text-sm hover:bg-accent transition-colors"
            >
              {t('common.close')}
            </button>
          </div>

          <div className="grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-[320px_minmax(0,1fr)]">
            <div className="border-b border-border lg:border-b-0 lg:border-r border-border min-h-0 overflow-y-auto p-4">
              {state.loading ? (
                <div className="rounded-xl border border-dashed border-border p-4 text-sm text-muted-foreground">
                  {t('agentManager.conversation.loadingSessions')}
                </div>
              ) : state.groupedSessions.length === 0 ? (
                <div className="rounded-xl border border-dashed border-border p-4 text-sm text-muted-foreground">
                  {t('agentManager.conversation.emptySessions')}
                </div>
              ) : (
                <div className="space-y-2">
                  {state.groupedSessions.map((group) => {
                    const selected = group.session.id === state.selectedSessionId;
                    return (
                      <button
                        key={group.session.id}
                        onClick={() => {
                          void onSelectSession(group.session.id);
                        }}
                        className={`w-full rounded-xl border p-3 text-left transition-colors ${
                          selected
                            ? 'border-primary bg-primary/5'
                            : 'border-border bg-background/40 hover:bg-accent/40'
                        }`}
                      >
                        <div className="text-sm font-medium text-foreground">{group.projectName}</div>
                        <div className="mt-1 truncate text-xs text-muted-foreground">
                          {group.session.title?.trim() || group.session.id}
                        </div>
                        <div className="mt-2 text-[11px] text-muted-foreground">
                          {t('agentManager.conversation.updatedAt', { time: group.session.updatedAt.toLocaleString() })}
                        </div>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>

            <div className="min-h-0 overflow-y-auto p-4">
              {state.messagesLoading ? (
                <div className="rounded-xl border border-dashed border-border p-4 text-sm text-muted-foreground">
                  {t('agentManager.conversation.loadingMessages')}
                </div>
              ) : state.messages.length === 0 ? (
                <div className="rounded-xl border border-dashed border-border p-4 text-sm text-muted-foreground">
                  {t('agentManager.conversation.emptyMessages')}
                </div>
              ) : (
                <div className="space-y-3">
                  {state.messages.map((message) => (
                    <div key={message.id} className="rounded-xl border border-border bg-background/40 p-4">
                      <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                        <span className="rounded-full bg-muted px-2 py-0.5 text-foreground">
                          {getMessageRoleLabel(message.role, t)}
                        </span>
                        <span>{formatMessageTime(message, t)}</span>
                      </div>
                      <div className="mt-3 whitespace-pre-wrap break-words text-sm text-foreground">
                        {message.content?.trim() || t('agentManager.conversation.emptyContent')}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </>
  );
};

export default AgentConversationPanel;
