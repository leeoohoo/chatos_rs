import { useEffect, type Dispatch, type MutableRefObject, type SetStateAction } from 'react';

import type { DemoTask } from '../types';
import {
  API_BASE_URL,
  apiRequest,
  formatRelativeTime,
  mapTask,
  type BridgeStatus,
  type RawProjectContact,
  type RawSession,
  type RealtimeEnvelope,
  type StoredAuth,
  type WebSocketStatus,
} from './support';

interface BridgeRealtimeOptions {
  activeProjectId: string | null;
  auth: StoredAuth | null;
  conversationId: string | null;
  pageVisible: boolean;
  rawSessions: RawSession[];
  status: BridgeStatus;
  activeTurnIdRef: MutableRefObject<string | null>;
  refreshTimerRef: MutableRefObject<number | null>;
  loadProjectContactRows: (token: string, projectId: string | null) => Promise<RawProjectContact[]>;
  loadWorkspaceTasks: (token: string, sessions: RawSession[]) => Promise<DemoTask[]>;
  refresh: () => Promise<void>;
  refreshContactList: (token: string) => Promise<void>;
  refreshProjectList: (token: string) => Promise<void>;
  refreshSessionList: (token: string) => Promise<RawSession[]>;
  scheduleConversationRefresh: () => void;
  setError: Dispatch<SetStateAction<string | null>>;
  setIsStopping: Dispatch<SetStateAction<boolean>>;
  setPageVisible: Dispatch<SetStateAction<boolean>>;
  setRunningTasks: Dispatch<SetStateAction<DemoTask[]>>;
  setStreamingText: Dispatch<SetStateAction<string>>;
  setThinking: Dispatch<SetStateAction<boolean>>;
  setWebSocketStatus: Dispatch<SetStateAction<WebSocketStatus>>;
}

export function useBridgeRealtime(options: BridgeRealtimeOptions) {
  const {
    activeProjectId,
    auth,
    conversationId,
    pageVisible,
    rawSessions,
    status,
    activeTurnIdRef,
    refreshTimerRef,
    loadProjectContactRows,
    loadWorkspaceTasks,
    refresh,
    refreshContactList,
    refreshProjectList,
    refreshSessionList,
    scheduleConversationRefresh,
    setError,
    setIsStopping,
    setPageVisible,
    setRunningTasks,
    setStreamingText,
    setThinking,
    setWebSocketStatus,
  } = options;

  useEffect(() => {
    void refresh();
  }, [auth?.accessToken]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => () => {
    if (refreshTimerRef.current !== null) window.clearTimeout(refreshTimerRef.current);
  }, [refreshTimerRef]);

  useEffect(() => {
    const handleVisibilityChange = () => setPageVisible(document.visibilityState !== 'hidden');
    document.addEventListener('visibilitychange', handleVisibilityChange);
    return () => document.removeEventListener('visibilitychange', handleVisibilityChange);
  }, [setPageVisible]);

  useEffect(() => {
    if (!auth?.accessToken || status !== 'live' || !conversationId || !pageVisible) {
      setWebSocketStatus(auth?.accessToken ? 'disconnected' : 'idle');
      return undefined;
    }
    let disposed = false;
    let socket: WebSocket | null = null;
    let retryTimer: number | null = null;

    const connect = async () => {
      setWebSocketStatus('connecting');
      try {
        const ticketResponse = await apiRequest<{ ticket?: string }>('/auth/ws-ticket', auth.accessToken, { method: 'POST' });
        const ticket = String(ticketResponse.ticket || '').trim();
        if (!ticket || disposed) return;
        const wsUrl = new URL(`${API_BASE_URL.replace(/^http/, 'ws')}/realtime/ws`);
        wsUrl.searchParams.set('ws_ticket', ticket);
        socket = new WebSocket(wsUrl.toString());
        socket.onopen = () => {
          if (disposed || !socket) return;
          setWebSocketStatus('connected');
          socket.send(JSON.stringify({
            type: 'subscribe',
            topics: [
              { scope: 'projects' },
              { scope: 'sessions' },
              ...rawSessions.filter((session) => !session.archived).map((session) => ({ scope: 'conversation', id: session.id })),
              ...(activeProjectId ? [{ scope: 'project', id: activeProjectId }] : []),
            ],
          }));
        };
        socket.onmessage = (message) => {
          let envelope: RealtimeEnvelope;
          try {
            envelope = JSON.parse(String(message.data || '')) as RealtimeEnvelope;
          } catch {
            return;
          }
          if (envelope.type !== 'event' || !envelope.payload) return;
          const kind = envelope.payload.kind;
          if (kind === 'projects_updated') {
            void refreshProjectList(auth.accessToken);
            return;
          }
          if (kind === 'sessions_updated') {
            void refreshSessionList(auth.accessToken);
            scheduleConversationRefresh();
            return;
          }
          if (kind === 'contacts_updated') {
            void refreshContactList(auth.accessToken);
            return;
          }
          if (kind === 'project_members_updated' || kind === 'project_contacts_updated') {
            void loadProjectContactRows(auth.accessToken, activeProjectId);
            return;
          }
          const eventConversationId = String(envelope.conversation_id || envelope.payload.conversation_id || '').trim();
          if (kind === 'task_board') {
            const rawTask = envelope.payload.task;
            const eventTaskId = String(rawTask?.id || envelope.payload.task_id || '').trim();
            if (eventConversationId && eventTaskId) {
              const session = rawSessions.find((item) => item.id === eventConversationId);
              const mappedId = `${eventConversationId}:${eventTaskId}`;
              setRunningTasks((current) => {
                const remaining = current.filter((task) => task.id !== mappedId);
                if (!rawTask) return remaining;
                return [{
                  ...mapTask(rawTask),
                  id: mappedId,
                  conversationId: eventConversationId,
                  conversationTitle: session?.title || '未命名会话',
                  updatedAt: formatRelativeTime(rawTask.updated_at || rawTask.created_at),
                }, ...remaining];
              });
            } else {
              void loadWorkspaceTasks(auth.accessToken, rawSessions);
            }
            if (!eventConversationId || eventConversationId === conversationId) scheduleConversationRefresh();
            return;
          }
          if (eventConversationId && eventConversationId !== conversationId) return;
          if (kind !== 'chat_stream') return;
          const streamType = String(envelope.payload.raw?.type || envelope.payload.stream_type || envelope.event || '').toLowerCase();
          if (streamType.includes('start')) {
            setThinking(true);
            setStreamingText('');
            return;
          }
          if (streamType.includes('chunk') || streamType.includes('delta')) {
            const chunk = typeof envelope.payload.raw?.content === 'string' ? envelope.payload.raw.content : '';
            if (chunk) {
              setThinking(false);
              setStreamingText((current) => current + chunk);
            }
            return;
          }
          if (streamType.includes('complete') || streamType.includes('error') || streamType.includes('cancel')) {
            setThinking(false);
            setIsStopping(false);
            activeTurnIdRef.current = null;
            if (streamType.includes('error')) setError(String(envelope.payload.raw?.message || 'AI 回复失败'));
            scheduleConversationRefresh();
          }
        };
        socket.onerror = () => setWebSocketStatus('error');
        socket.onclose = () => {
          if (disposed) return;
          setWebSocketStatus('disconnected');
          retryTimer = window.setTimeout(() => void connect(), 1800);
        };
      } catch (cause) {
        if (disposed) return;
        setWebSocketStatus('error');
        setError(cause instanceof Error ? cause.message : String(cause));
        retryTimer = window.setTimeout(() => void connect(), 2500);
      }
    };

    void connect();
    return () => {
      disposed = true;
      if (retryTimer !== null) window.clearTimeout(retryTimer);
      socket?.close();
    };
  }, [activeProjectId, activeTurnIdRef, auth, conversationId, loadProjectContactRows, loadWorkspaceTasks, pageVisible, rawSessions, refreshContactList, refreshProjectList, refreshSessionList, scheduleConversationRefresh, setError, setIsStopping, setRunningTasks, setStreamingText, setThinking, setWebSocketStatus, status]);
}
