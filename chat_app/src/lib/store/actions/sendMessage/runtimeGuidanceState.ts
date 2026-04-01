import type { Message } from '../../../../types';
import type {
  ChatStoreDraft,
  RuntimeGuidanceItem,
  SessionRuntimeGuidanceState,
} from '../../types';

const MAX_RUNTIME_GUIDANCE_ITEMS = 20;

type RuntimeGuidanceStatus = RuntimeGuidanceItem['status'];

const createRuntimeGuidanceItem = ({
  guidanceId,
  turnId,
  content,
  status,
  createdAt,
  appliedAt,
}: {
  guidanceId: string;
  turnId: string | null;
  content: string;
  status: RuntimeGuidanceStatus;
  createdAt: string;
  appliedAt: string | null;
}): RuntimeGuidanceItem => ({
  guidanceId,
  turnId,
  content,
  status,
  createdAt,
  appliedAt,
});

const asRecord = (value: unknown): Record<string, unknown> | null => (
  value && typeof value === 'object'
    ? value as Record<string, unknown>
    : null
);

const toFiniteNumber = (value: unknown): number | null => {
  const next = Number(value);
  return Number.isFinite(next) ? next : null;
};

const resolveRuntimeGuidanceContent = (
  data: Record<string, unknown> | null,
): string | null => {
  if (!data) {
    return null;
  }

  const contentCandidates = [
    data.content,
    data.instruction,
    data.message,
    data.text,
  ];
  const content = contentCandidates.find(
    (value) => typeof value === 'string' && value.trim().length > 0,
  );
  return typeof content === 'string' ? content : null;
};

const limitRuntimeGuidanceItems = (
  items: RuntimeGuidanceItem[],
): RuntimeGuidanceItem[] => items.slice(0, MAX_RUNTIME_GUIDANCE_ITEMS);

export const createEmptySessionRuntimeGuidanceState =
  (): SessionRuntimeGuidanceState => ({
    pendingCount: 0,
    appliedCount: 0,
    lastGuidanceAt: null,
    lastAppliedAt: null,
    items: [],
  });

const ensureSessionRuntimeGuidanceState = (
  state: ChatStoreDraft,
  sessionId: string,
): SessionRuntimeGuidanceState => {
  if (!state.sessionRuntimeGuidanceState) {
    state.sessionRuntimeGuidanceState = {};
  }

  const existing = state.sessionRuntimeGuidanceState[sessionId];
  if (existing) {
    return existing;
  }

  const next = createEmptySessionRuntimeGuidanceState();
  state.sessionRuntimeGuidanceState[sessionId] = next;
  return next;
};

const updateVisibleRuntimeGuidanceMessage = (
  state: ChatStoreDraft,
  sessionId: string,
  optimisticGuidanceId: string,
  guidanceId: string,
  status: RuntimeGuidanceStatus,
  guidanceAt: string,
) => {
  if (state.currentSessionId !== sessionId || !Array.isArray(state.messages)) {
    return;
  }

  const guidanceMessageIndex = state.messages.findIndex((message: Message) => (
    message?.id === optimisticGuidanceId
    || (guidanceId && message?.id === guidanceId)
  ));
  if (guidanceMessageIndex < 0) {
    return;
  }

  const nextMetadata = {
    ...(state.messages[guidanceMessageIndex]?.metadata || {}),
    runtime_guidance: {
      guidance_id: guidanceId,
      status,
      created_at: guidanceAt,
    },
  };

  state.messages[guidanceMessageIndex] = {
    ...state.messages[guidanceMessageIndex],
    metadata: nextMetadata,
  };
};

export const queueOptimisticRuntimeGuidance = (
  state: ChatStoreDraft,
  {
    sessionId,
    turnId,
    guidanceId,
    content,
    guidanceAt,
  }: {
    sessionId: string;
    turnId: string;
    guidanceId: string;
    content: string;
    guidanceAt: string;
  },
) => {
  const prev = ensureSessionRuntimeGuidanceState(state, sessionId);
  const prevItems = Array.isArray(prev.items) ? prev.items : [];

  state.sessionRuntimeGuidanceState[sessionId] = {
    ...prev,
    pendingCount: Math.max(0, Number(prev.pendingCount || 0) + 1),
    lastGuidanceAt: guidanceAt,
    items: limitRuntimeGuidanceItems([
      createRuntimeGuidanceItem({
        guidanceId,
        turnId,
        content,
        status: 'queued',
        createdAt: guidanceAt,
        appliedAt: null,
      }),
      ...prevItems,
    ]),
  };

  if (state.currentSessionId !== sessionId) {
    return;
  }

  const exists = state.messages.some((message: Message) => message?.id === guidanceId);
  if (exists) {
    return;
  }

  state.messages.push({
    id: guidanceId,
    sessionId,
    role: 'user',
    content,
    status: 'completed',
    createdAt: new Date(guidanceAt),
    metadata: {
      conversation_turn_id: turnId,
      message_mode: 'runtime_guidance',
      message_source: 'runtime_guidance',
      runtime_guidance: {
        guidance_id: guidanceId,
        status: 'queued',
        created_at: guidanceAt,
      },
    },
  });
};

export const reconcileSubmittedRuntimeGuidance = (
  state: ChatStoreDraft,
  {
    sessionId,
    turnId,
    optimisticGuidanceId,
    responseGuidanceId,
    content,
    guidanceAt,
    status,
    pendingCount,
  }: {
    sessionId: string;
    turnId: string;
    optimisticGuidanceId: string;
    responseGuidanceId?: string | null;
    content: string;
    guidanceAt: string;
    status?: string | null;
    pendingCount?: number | null;
  },
) => {
  const prev = ensureSessionRuntimeGuidanceState(state, sessionId);
  const guidanceId = String(responseGuidanceId || '').trim() || optimisticGuidanceId;
  const nextStatus: RuntimeGuidanceStatus = status === 'applied'
    ? 'applied'
    : (status === 'dropped' ? 'dropped' : 'queued');
  const prevItems = Array.isArray(prev.items) ? [...prev.items] : [];
  const existingIndex = prevItems.findIndex(
    (item) => item.guidanceId === optimisticGuidanceId || item.guidanceId === guidanceId,
  );

  if (existingIndex >= 0) {
    prevItems[existingIndex] = {
      ...prevItems[existingIndex],
      guidanceId,
      turnId,
      content: prevItems[existingIndex]?.content || content,
      status: nextStatus,
    };
  } else {
    prevItems.unshift(createRuntimeGuidanceItem({
      guidanceId,
      turnId,
      content,
      status: nextStatus,
      createdAt: guidanceAt,
      appliedAt: null,
    }));
  }

  state.sessionRuntimeGuidanceState[sessionId] = {
    ...prev,
    pendingCount: pendingCount ?? prev.pendingCount,
    lastGuidanceAt: guidanceAt,
    items: limitRuntimeGuidanceItems(prevItems),
  };

  updateVisibleRuntimeGuidanceMessage(
    state,
    sessionId,
    optimisticGuidanceId,
    guidanceId,
    nextStatus,
    guidanceAt,
  );
};

export const applyQueuedRuntimeGuidanceEvent = (
  state: ChatStoreDraft,
  sessionId: string,
  payload: unknown,
) => {
  const prev = ensureSessionRuntimeGuidanceState(state, sessionId);
  const event = asRecord(payload);
  const data = asRecord(event?.data);
  const pendingFromPayload = toFiniteNumber(data?.pending_count);
  const guidanceId = String(data?.guidance_id || '').trim();
  const guidanceContent = resolveRuntimeGuidanceContent(data);
  const queuedAt = typeof data?.created_at === 'string'
    ? data.created_at
    : (typeof event?.timestamp === 'string' ? event.timestamp : new Date().toISOString());
  const prevItems = Array.isArray(prev.items) ? [...prev.items] : [];

  if (guidanceId && guidanceContent) {
    const existingIndex = prevItems.findIndex((item) => item.guidanceId === guidanceId);
    if (existingIndex >= 0) {
      prevItems[existingIndex] = {
        ...prevItems[existingIndex],
        turnId: String(data?.turn_id || prevItems[existingIndex]?.turnId || '').trim() || null,
        content: guidanceContent,
        status: 'queued',
        createdAt: typeof prevItems[existingIndex]?.createdAt === 'string'
          ? prevItems[existingIndex].createdAt
          : queuedAt,
      };
    } else {
      prevItems.unshift(createRuntimeGuidanceItem({
        guidanceId,
        turnId: String(data?.turn_id || '').trim() || null,
        content: guidanceContent,
        status: 'queued',
        createdAt: queuedAt,
        appliedAt: null,
      }));
    }
  }

  state.sessionRuntimeGuidanceState[sessionId] = {
    ...prev,
    pendingCount: pendingFromPayload !== null
      ? Math.max(0, pendingFromPayload)
      : Math.max(0, Number(prev.pendingCount || 0) + 1),
    lastGuidanceAt: typeof event?.timestamp === 'string'
      ? event.timestamp
      : prev.lastGuidanceAt,
    items: limitRuntimeGuidanceItems(prevItems),
  };
};

export const applyAppliedRuntimeGuidanceEvent = (
  state: ChatStoreDraft,
  sessionId: string,
  payload: unknown,
) => {
  const prev = ensureSessionRuntimeGuidanceState(state, sessionId);
  const event = asRecord(payload);
  const data = asRecord(event?.data);
  const pendingFromPayload = toFiniteNumber(data?.pending_count);
  const nextPending = pendingFromPayload !== null
    ? Math.max(0, pendingFromPayload)
    : Math.max(0, Number(prev.pendingCount || 0) - 1);
  const appliedAt = typeof data?.applied_at === 'string'
    ? data.applied_at
    : (typeof event?.timestamp === 'string' ? event.timestamp : prev.lastAppliedAt);
  const guidanceId = String(data?.guidance_id || '').trim();
  const guidanceContent = resolveRuntimeGuidanceContent(data);
  const prevItems = Array.isArray(prev.items) ? [...prev.items] : [];

  if (guidanceId) {
    const existingIndex = prevItems.findIndex((item) => item.guidanceId === guidanceId);
    if (existingIndex >= 0) {
      prevItems[existingIndex] = {
        ...prevItems[existingIndex],
        status: 'applied',
        appliedAt,
        turnId: String(data?.turn_id || prevItems[existingIndex]?.turnId || '').trim() || null,
      };
    } else {
      const fallbackTurnId = String(data?.turn_id || '').trim();
      const optimisticIndex = prevItems.findIndex((item) => {
        if ((item?.status || '') !== 'queued') {
          return false;
        }
        if (!String(item?.guidanceId || '').startsWith('local_')) {
          return false;
        }
        if (!fallbackTurnId) {
          return true;
        }
        return String(item?.turnId || '').trim() === fallbackTurnId;
      });
      if (optimisticIndex >= 0) {
        prevItems[optimisticIndex] = {
          ...prevItems[optimisticIndex],
          guidanceId,
          status: 'applied',
          appliedAt,
          turnId: fallbackTurnId || String(prevItems[optimisticIndex]?.turnId || '').trim() || null,
        };
      } else if (guidanceContent) {
        prevItems.unshift(createRuntimeGuidanceItem({
          guidanceId,
          turnId: fallbackTurnId || null,
          content: guidanceContent,
          status: 'applied',
          createdAt: typeof data?.created_at === 'string'
            ? data.created_at
            : appliedAt || new Date().toISOString(),
          appliedAt,
        }));
      }
    }
  }

  state.sessionRuntimeGuidanceState[sessionId] = {
    ...prev,
    pendingCount: nextPending,
    appliedCount: Math.max(0, Number(prev.appliedCount || 0) + 1),
    lastAppliedAt: appliedAt,
    items: limitRuntimeGuidanceItems(prevItems),
  };
};

export const resetRuntimeGuidancePendingCount = (
  state: ChatStoreDraft,
  sessionId: string,
) => {
  const prev = ensureSessionRuntimeGuidanceState(state, sessionId);
  state.sessionRuntimeGuidanceState[sessionId] = {
    ...prev,
    pendingCount: 0,
  };
};

export const rollbackRuntimeGuidanceSubmission = (
  state: ChatStoreDraft,
  {
    sessionId,
    guidanceId,
  }: {
    sessionId: string;
    guidanceId: string;
  },
) => {
  const prev = ensureSessionRuntimeGuidanceState(state, sessionId);
  const prevItems = Array.isArray(prev.items) ? prev.items : [];

  state.sessionRuntimeGuidanceState[sessionId] = {
    ...prev,
    pendingCount: Math.max(0, Number(prev.pendingCount || 0) - 1),
    items: prevItems.filter((item) => item.guidanceId !== guidanceId),
  };

  if (state.currentSessionId === sessionId && Array.isArray(state.messages)) {
    state.messages = state.messages.filter((message) => message?.id !== guidanceId);
  }
};
