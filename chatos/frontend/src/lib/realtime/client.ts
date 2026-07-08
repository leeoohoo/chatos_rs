// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { buildWsUrl } from './buildWsUrl';
import { debugLog } from '../utils';
import type {
  RealtimeAckMessage,
  RealtimeConnectionState,
  RealtimeDebugEventRecord,
  RealtimeDebugSnapshot,
  RealtimeEventEnvelope,
  RealtimeErrorMessage,
  RealtimeTopic,
} from './types';

type EventListener = (event: RealtimeEventEnvelope) => void;
type StateListener = (state: RealtimeConnectionState) => void;
type TopicListener = () => void;
type DebugListener = (snapshot: RealtimeDebugSnapshot) => void;

const RECONNECT_BASE_DELAY_MS = 1000;
const RECONNECT_MAX_DELAY_MS = 5000;
const RECENT_DEBUG_EVENT_LIMIT = 30;

export class RealtimeClient {
  private baseUrl: string;
  private accessToken: string | null = null;
  private socket: WebSocket | null = null;
  private issueWebSocketTicket: (() => Promise<string>) | null = null;
  private connectInFlight: Promise<void> | null = null;
  private connectAttemptId = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectAttempt = 0;
  private manuallyClosed = false;
  private state: RealtimeConnectionState = 'idle';
  private readonly eventListeners = new Set<EventListener>();
  private readonly stateListeners = new Set<StateListener>();
  private readonly topicListeners = new Set<TopicListener>();
  private readonly debugListeners = new Set<DebugListener>();
  private readonly topicRefs = new Map<string, { topic: RealtimeTopic; count: number }>();
  private lastSentTopicKeys = new Set<string>();
  private lastAck: RealtimeAckMessage | null = null;
  private lastError: RealtimeErrorMessage | null = null;
  private lastPongTs: string | null = null;
  private lastControlMessageAt: string | null = null;
  private lastEventAt: string | null = null;
  private recentEvents: RealtimeDebugEventRecord[] = [];

  constructor(baseUrl: string, issueWebSocketTicket?: (() => Promise<string>) | null) {
    this.baseUrl = baseUrl;
    this.issueWebSocketTicket = issueWebSocketTicket || null;
  }

  getConnectionState(): RealtimeConnectionState {
    return this.state;
  }

  setBaseUrl(baseUrl: string): void {
    if (this.baseUrl === baseUrl) {
      return;
    }
    this.baseUrl = baseUrl;
    if (this.accessToken) {
      this.reconnect();
    }
  }

  setWebSocketTicketFactory(issueWebSocketTicket?: (() => Promise<string>) | null): void {
    this.issueWebSocketTicket = issueWebSocketTicket || null;
    if (this.accessToken) {
      this.reconnect();
    }
  }

  setAccessToken(token?: string | null): void {
    const trimmed = (token || '').trim();
    const normalized = trimmed.length > 0 ? trimmed : null;
    if (this.accessToken === normalized) {
      if (normalized) {
        this.connect();
      }
      return;
    }
    this.accessToken = normalized;
    if (!this.accessToken) {
      this.close(true);
      this.setState('idle');
      return;
    }
    this.connect();
  }

  subscribe(listener: EventListener): () => void {
    this.eventListeners.add(listener);
    if (this.accessToken) {
      this.connect();
    }
    return () => {
      this.eventListeners.delete(listener);
    };
  }

  subscribeTopic(topic: RealtimeTopic): () => void {
    const key = this.getTopicKey(topic);
    const existing = this.topicRefs.get(key);
    if (existing) {
      existing.count += 1;
    } else {
      this.topicRefs.set(key, {
        topic: {
          scope: topic.scope,
          id: typeof topic.id === 'string' ? topic.id.trim() : topic.id ?? null,
        },
        count: 1,
      });
    }
    this.notifyTopicListeners();
    if (this.accessToken) {
      this.connect();
      this.syncTopics();
    }
    return () => {
      const current = this.topicRefs.get(key);
      if (!current) {
        return;
      }
      if (current.count <= 1) {
        this.topicRefs.delete(key);
      } else {
        current.count -= 1;
      }
      this.notifyTopicListeners();
      this.syncTopics();
    };
  }

  subscribeTopics(listener: TopicListener): () => void {
    this.topicListeners.add(listener);
    listener();
    return () => {
      this.topicListeners.delete(listener);
    };
  }

  getTopics(): RealtimeTopic[] {
    return Array.from(this.topicRefs.values()).map(({ topic }) => ({
      scope: topic.scope,
      id: topic.id ?? null,
    }));
  }

  getDebugSnapshot(): RealtimeDebugSnapshot {
    return {
      connectionState: this.state,
      activeTopics: this.getTopics(),
      lastAck: this.lastAck,
      lastError: this.lastError,
      lastPongTs: this.lastPongTs,
      lastControlMessageAt: this.lastControlMessageAt,
      lastEventAt: this.lastEventAt,
      recentEvents: this.recentEvents.slice(),
    };
  }

  subscribeDebug(listener: DebugListener): () => void {
    this.debugListeners.add(listener);
    listener(this.getDebugSnapshot());
    return () => {
      this.debugListeners.delete(listener);
    };
  }

  subscribeState(listener: StateListener): () => void {
    this.stateListeners.add(listener);
    listener(this.state);
    return () => {
      this.stateListeners.delete(listener);
    };
  }

  destroy(): void {
    this.clearReconnectTimer();
    this.close(true);
    this.eventListeners.clear();
    this.stateListeners.clear();
    this.topicListeners.clear();
    this.debugListeners.clear();
    this.lastSentTopicKeys = new Set();
    this.setState('idle');
  }

  private connect(): void {
    if (!this.accessToken || !this.issueWebSocketTicket) {
      return;
    }
    if (
      this.socket
      && (this.socket.readyState === WebSocket.OPEN || this.socket.readyState === WebSocket.CONNECTING)
    ) {
      return;
    }
    if (this.connectInFlight) {
      return;
    }

    this.manuallyClosed = false;
    this.setState('connecting');

    const attemptId = this.nextConnectAttemptId();
    this.connectInFlight = this.openSocket(attemptId);
  }

  private reconnect(): void {
    this.close(false);
    this.clearReconnectTimer();
    this.connect();
  }

  private close(manual: boolean): void {
    this.manuallyClosed = manual;
    this.cancelPendingConnect();
    if (this.socket) {
      const socket = this.socket;
      this.socket = null;
      try {
        socket.close();
      } catch (error) {
        console.error('Failed to close realtime socket:', error);
      }
    }
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer || !this.accessToken) {
      return;
    }
    const delay = Math.min(
      RECONNECT_BASE_DELAY_MS * Math.max(1, 2 ** this.reconnectAttempt),
      RECONNECT_MAX_DELAY_MS,
    );
    this.reconnectAttempt += 1;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.connect();
    }, delay);
  }

  private syncTopics(force = false): void {
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
      return;
    }
    const topicEntries = Array.from(this.topicRefs.entries()).map(([key, value]) => ({
      key,
      topic: value.topic,
    }));
    const nextKeys = new Set(topicEntries.map((entry) => entry.key));
    const addedTopics = topicEntries
      .filter((entry) => force || !this.lastSentTopicKeys.has(entry.key))
      .map((entry) => entry.topic);
    const removedTopics = Array.from(this.lastSentTopicKeys)
      .filter((key) => !nextKeys.has(key))
      .map((key) => this.parseTopicKey(key))
      .filter((topic): topic is RealtimeTopic => Boolean(topic));

    if (removedTopics.length > 0) {
      this.sendControlMessage({
        type: 'unsubscribe',
        topics: removedTopics,
      });
    }
    if (addedTopics.length > 0) {
      this.sendControlMessage({
        type: 'subscribe',
        topics: addedTopics,
      });
    }
    this.lastSentTopicKeys = nextKeys;
    this.notifyDebugListeners();
  }

  private sendControlMessage(payload: Record<string, unknown>): void {
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
      return;
    }
    try {
      this.lastControlMessageAt = new Date().toISOString();
      debugLog('[realtime] control', payload);
      this.socket.send(JSON.stringify(payload));
      this.notifyDebugListeners();
    } catch (error) {
      console.error('Failed to send realtime control message:', error);
    }
  }

  private recordDebugEvent(event: RealtimeEventEnvelope): void {
    const payload = this.asRecord(event.payload);
    const entry: RealtimeDebugEventRecord = {
      event: event.event,
      ts: this.getOptionalString(event.ts) || new Date().toISOString(),
      conversation_id: event.conversation_id ?? null,
      project_id: event.project_id ?? null,
      payloadKind: this.getOptionalString(payload?.kind),
      payloadReason: this.getOptionalString(payload?.reason),
      payloadAction: this.getOptionalString(payload?.action),
      streamType: this.getOptionalString(payload?.stream_type),
    };
    this.lastEventAt = entry.ts;
    this.recentEvents = [entry, ...this.recentEvents].slice(0, RECENT_DEBUG_EVENT_LIMIT);
  }

  private asRecord(value: unknown): Record<string, unknown> | null {
    if (!value || typeof value !== 'object') {
      return null;
    }
    return value as Record<string, unknown>;
  }

  private getOptionalString(value: unknown): string | null {
    if (typeof value !== 'string') {
      return null;
    }
    const trimmed = value.trim();
    return trimmed || null;
  }

  private getTopicKey(topic: RealtimeTopic): string {
    const scope = String(topic.scope || '').trim();
    const id = typeof topic.id === 'string' ? topic.id.trim() : '';
    return `${scope}:${id}`;
  }

  private parseTopicKey(key: string): RealtimeTopic | null {
    const separatorIndex = key.indexOf(':');
    if (separatorIndex < 0) {
      return null;
    }
    const scope = key.slice(0, separatorIndex).trim();
    const id = key.slice(separatorIndex + 1).trim();
    if (!scope) {
      return null;
    }
    return {
      scope: scope as RealtimeTopic['scope'],
      id: id || null,
    };
  }

  private notifyTopicListeners(): void {
    this.topicListeners.forEach((listener) => {
      try {
        listener();
      } catch (error) {
        console.error('Realtime topic listener failed:', error);
      }
    });
    this.notifyDebugListeners();
  }

  private clearReconnectTimer(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  private setState(state: RealtimeConnectionState): void {
    if (this.state === state) {
      return;
    }
    this.state = state;
    this.stateListeners.forEach((listener) => {
      try {
        listener(state);
      } catch (error) {
        console.error('Realtime state listener failed:', error);
      }
    });
    this.notifyDebugListeners();
  }

  private nextConnectAttemptId(): number {
    this.connectAttemptId += 1;
    return this.connectAttemptId;
  }

  private cancelPendingConnect(): void {
    this.connectAttemptId += 1;
    this.connectInFlight = null;
  }

  private isCurrentConnectAttempt(attemptId: number): boolean {
    return this.connectAttemptId === attemptId;
  }

  private async openSocket(attemptId: number): Promise<void> {
    try {
      const ticket = await this.issueWebSocketTicket?.();
      if (!ticket?.trim()) {
        throw new Error('缺少 WebSocket 连接票据');
      }
      if (!this.isCurrentConnectAttempt(attemptId) || !this.accessToken || this.manuallyClosed) {
        return;
      }

      const socket = new WebSocket(buildWsUrl(this.baseUrl, '/realtime/ws', ticket));
      this.socket = socket;

      socket.onopen = () => {
        if (this.socket !== socket) {
          return;
        }
        this.reconnectAttempt = 0;
        this.lastSentTopicKeys = new Set();
        this.setState('connected');
        this.syncTopics(true);
      };

      socket.onmessage = (message) => {
        if (this.socket !== socket) {
          return;
        }
        try {
          const parsed = JSON.parse(String(message.data || '')) as
            | RealtimeEventEnvelope
            | RealtimeAckMessage
            | RealtimeErrorMessage
            | { type?: string; ts?: string };
          if (parsed && parsed.type === 'event') {
            this.recordDebugEvent(parsed as RealtimeEventEnvelope);
            this.notifyDebugListeners();
            this.eventListeners.forEach((listener) => {
              try {
                listener(parsed as RealtimeEventEnvelope);
              } catch (error) {
                console.error('Realtime listener failed:', error);
              }
            });
            return;
          }
          if (parsed && parsed.type === 'ack') {
            this.lastAck = parsed as RealtimeAckMessage;
            this.lastError = null;
            debugLog('[realtime] ack', this.lastAck);
            this.notifyDebugListeners();
            return;
          }
          if (parsed && parsed.type === 'error') {
            this.lastError = parsed as RealtimeErrorMessage;
            debugLog('[realtime] error', this.lastError);
            this.notifyDebugListeners();
            return;
          }
          if (parsed && parsed.type === 'pong') {
            this.lastPongTs = typeof parsed.ts === 'string' ? parsed.ts : null;
            this.notifyDebugListeners();
          }
        } catch (error) {
          console.error('Failed to parse realtime event:', error);
        }
      };

      socket.onerror = () => {
        if (this.socket !== socket) {
          return;
        }
        this.setState('error');
      };

      socket.onclose = () => {
        if (this.socket === socket) {
          this.socket = null;
        }
        if (this.manuallyClosed || !this.accessToken) {
          this.setState(this.accessToken ? 'disconnected' : 'idle');
          return;
        }
        this.setState('disconnected');
        this.scheduleReconnect();
      };
    } catch (error) {
      if (!this.isCurrentConnectAttempt(attemptId) || this.manuallyClosed || !this.accessToken) {
        return;
      }
      console.error('Failed to issue realtime websocket ticket:', error);
      this.lastError = {
        type: 'error',
        code: 'ws_ticket_failed',
        message: error instanceof Error ? error.message : 'WebSocket 票据签发失败',
      };
      this.notifyDebugListeners();
      this.setState('error');
      this.scheduleReconnect();
    } finally {
      if (this.isCurrentConnectAttempt(attemptId)) {
        this.connectInFlight = null;
      }
    }
  }

  private notifyDebugListeners(): void {
    const snapshot = this.getDebugSnapshot();
    this.debugListeners.forEach((listener) => {
      try {
        listener(snapshot);
      } catch (error) {
        console.error('Realtime debug listener failed:', error);
      }
    });
  }
}
