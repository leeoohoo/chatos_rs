import type { Message } from '../../../../types';
import {
  getConversationTurnId,
  normalizeTurnId,
} from '../messageNormalization';
import { createDefaultHistoryProcessState } from '../../actions/sendMessage/types';
import {
  resolveFinalAssistantProcessKey,
  resolveProcessMessageKey,
  resolveUserProcessKey,
} from './turnProcessKeys';

export type TurnProcessState = {
  expanded: boolean;
  loaded: boolean;
  loading: boolean;
};

const withUserProcessMeta = (
  message: Message,
  state?: Partial<TurnProcessState>,
): Message => {
  if (message.role !== 'user') {
    return message;
  }

  const historyProcess = message.metadata?.historyProcess;
  if (!historyProcess || typeof historyProcess !== 'object') {
    return message;
  }

  const nextHistoryProcess = {
    ...createDefaultHistoryProcessState({
      userMessageId: historyProcess.userMessageId || message.id,
      turnId: historyProcess.turnId || getConversationTurnId(message),
      finalAssistantMessageId: historyProcess.finalAssistantMessageId || null,
    }),
    ...historyProcess,
    ...(state || {}),
  };

  return {
    ...message,
    metadata: {
      ...(message.metadata || {}),
      historyProcess: nextHistoryProcess,
    },
  };
};

export const setTurnProcessExpanded = (
  messages: Message[],
  userMessageId: string,
  expanded: boolean,
  options: { processKey?: string } = {},
): Message[] => {
  const processKey = normalizeTurnId(options.processKey) || userMessageId;
  const hasTurnProcessKey = Boolean(processKey && processKey !== userMessageId);

  return messages.map((message) => {
    if (message.id === userMessageId) {
      return withUserProcessMeta(message, { expanded });
    }

    const finalForUserMessageId = message.metadata?.historyFinalForUserMessageId;
    const finalProcessKey = resolveFinalAssistantProcessKey(message);
    const isFinalMatch = finalForUserMessageId === userMessageId
      || (Boolean(processKey) && finalProcessKey === processKey);
    if (isFinalMatch) {
      return {
        ...message,
        metadata: {
          ...(message.metadata || {}),
          historyProcessExpanded: expanded,
          historyFinalForUserMessageId: finalForUserMessageId || userMessageId,
          ...(hasTurnProcessKey ? { historyFinalForTurnId: processKey } : {}),
        },
      };
    }

    const turnUserMessageId = message.metadata?.historyProcessUserMessageId;
    const turnProcessKey = resolveProcessMessageKey(message);
    const isProcessMatch = turnUserMessageId === userMessageId
      || (Boolean(processKey) && turnProcessKey === processKey);
    if (!isProcessMatch) {
      return message;
    }

    return {
      ...message,
      metadata: {
        ...(message.metadata || {}),
        hidden: !expanded,
        historyProcessUserMessageId: turnUserMessageId || userMessageId,
        ...(hasTurnProcessKey ? { historyProcessTurnId: processKey } : {}),
        historyProcessExpanded: expanded,
      },
    };
  });
};

export const mergeTurnProcessMessages = (
  messages: Message[],
  userMessageId: string,
  processMessages: Message[],
  expanded: boolean,
  options: { processKey?: string } = {},
): Message[] => {
  const processKey = normalizeTurnId(options.processKey) || userMessageId;
  const hasTurnProcessKey = Boolean(processKey && processKey !== userMessageId);

  const processById = new Map<string, Message>();
  processMessages.forEach((message) => {
    processById.set(message.id, message);
  });

  const merged = messages.map((message) => {
    if (message.id === userMessageId) {
      return withUserProcessMeta(
        {
          ...message,
          metadata: {
            ...(message.metadata || {}),
            historyProcess: {
              ...createDefaultHistoryProcessState({
                userMessageId,
                turnId: hasTurnProcessKey ? processKey : getConversationTurnId(message),
                finalAssistantMessageId: message.metadata?.historyProcess?.finalAssistantMessageId || null,
              }),
              ...(message.metadata?.historyProcess || {}),
              userMessageId,
              ...(hasTurnProcessKey ? { turnId: processKey } : {}),
            },
          },
        },
        { expanded, loaded: true, loading: false },
      );
    }

    const finalForUserMessageId = message.metadata?.historyFinalForUserMessageId;
    const finalProcessKey = resolveFinalAssistantProcessKey(message);
    const isFinalMatch = finalForUserMessageId === userMessageId
      || (Boolean(processKey) && finalProcessKey === processKey);
    if (isFinalMatch) {
      return {
        ...message,
        metadata: {
          ...(message.metadata || {}),
          historyProcessExpanded: expanded,
          historyFinalForUserMessageId: finalForUserMessageId || userMessageId,
          ...(hasTurnProcessKey ? { historyFinalForTurnId: processKey } : {}),
        },
      };
    }

    const turnUserMessageId = message.metadata?.historyProcessUserMessageId;
    const turnProcessKey = resolveProcessMessageKey(message);
    const isProcessMatch = turnUserMessageId === userMessageId
      || (Boolean(processKey) && turnProcessKey === processKey);
    if (!isProcessMatch) {
      return message;
    }

    const hydrated = processById.get(message.id) || message;
    return {
      ...hydrated,
      metadata: {
        ...(hydrated.metadata || {}),
        hidden: !expanded,
        historyProcessPlaceholder: false,
        historyProcessLoaded: true,
        historyProcessUserMessageId: turnUserMessageId || userMessageId,
        ...(hasTurnProcessKey ? { historyProcessTurnId: processKey } : {}),
        historyProcessExpanded: expanded,
      },
    };
  });

  const existingIds = new Set(merged.map((message) => message.id));
  const missingMessages = processMessages.filter((message) => !existingIds.has(message.id));
  if (missingMessages.length === 0) {
    return merged;
  }

  const insertionIndex = merged.findIndex(
    (message) => (
      message.metadata?.historyFinalForUserMessageId === userMessageId
      || resolveFinalAssistantProcessKey(message) === processKey
    ),
  );

  const normalizedMissing = missingMessages.map((message) => ({
    ...message,
    metadata: {
      ...(message.metadata || {}),
      hidden: !expanded,
      historyProcessPlaceholder: false,
      historyProcessLoaded: true,
      historyProcessUserMessageId: userMessageId,
      ...(hasTurnProcessKey ? { historyProcessTurnId: processKey } : {}),
      historyProcessExpanded: expanded,
    },
  }));

  if (insertionIndex < 0) {
    return [...merged, ...normalizedMissing];
  }

  return [
    ...merged.slice(0, insertionIndex),
    ...normalizedMissing,
    ...merged.slice(insertionIndex),
  ];
};

export const applyTurnProcessCache = (
  messages: Message[],
  processCache?: Record<string, Message[]>,
  processState?: Record<string, TurnProcessState>,
): Message[] => {
  if (!processCache && !processState) {
    return messages;
  }

  const resolveState = (processKey: string, fallbackUserMessageId: string): TurnProcessState | undefined => {
    if (!processState) {
      return undefined;
    }
    if (processKey && processState[processKey]) {
      return processState[processKey];
    }
    if (fallbackUserMessageId && processState[fallbackUserMessageId]) {
      return processState[fallbackUserMessageId];
    }
    return undefined;
  };

  const resolveCache = (processKey: string, fallbackUserMessageId: string): Message[] | undefined => {
    if (!processCache) {
      return undefined;
    }
    if (processKey && processCache[processKey]) {
      return processCache[processKey];
    }
    if (fallbackUserMessageId && processCache[fallbackUserMessageId]) {
      return processCache[fallbackUserMessageId];
    }
    return undefined;
  };

  return messages.map((message) => {
    if (message.role === 'user') {
      const userMessageId = message.id;
      const processKey = resolveUserProcessKey(message);
      const turnId = getConversationTurnId(message);
      const state = resolveState(processKey, userMessageId);
      if (!state) {
        return message;
      }
      const withTurnId = {
        ...message,
        metadata: {
          ...(message.metadata || {}),
          historyProcess: {
            ...createDefaultHistoryProcessState({
              userMessageId,
              turnId: turnId || getConversationTurnId(message),
              finalAssistantMessageId: message.metadata?.historyProcess?.finalAssistantMessageId || null,
            }),
            ...(message.metadata?.historyProcess || {}),
            userMessageId,
            ...(turnId ? { turnId } : {}),
          },
        },
      };
      return withUserProcessMeta(withTurnId, {
        expanded: state.expanded,
        loading: state.loading,
        loaded: state.loaded,
      });
    }

    const finalForUserMessageId = message.metadata?.historyFinalForUserMessageId;
    const finalProcessKey = resolveFinalAssistantProcessKey(message);
    const explicitFinalTurnId = normalizeTurnId(message.metadata?.historyFinalForTurnId)
      || getConversationTurnId(message);
    if (finalForUserMessageId || finalProcessKey) {
      const turnState = resolveState(finalProcessKey, finalForUserMessageId || '');
      return {
        ...message,
        metadata: {
          ...(message.metadata || {}),
          historyProcessExpanded: turnState?.expanded === true,
          ...(explicitFinalTurnId ? { historyFinalForTurnId: explicitFinalTurnId } : {}),
        },
      };
    }

    const turnUserMessageId = typeof message.metadata?.historyProcessUserMessageId === 'string'
      ? message.metadata.historyProcessUserMessageId
      : '';
    const turnProcessKey = resolveProcessMessageKey(message);
    const explicitProcessTurnId = normalizeTurnId(message.metadata?.historyProcessTurnId)
      || getConversationTurnId(message);
    if (!turnUserMessageId && !turnProcessKey) {
      return message;
    }

    const turnState = resolveState(turnProcessKey, turnUserMessageId);
    const expanded = turnState?.expanded === true;
    const loaded = turnState?.loaded === true;
    const visible = expanded && loaded;
    const cachedItems = resolveCache(turnProcessKey, turnUserMessageId) || [];
    const cached = cachedItems.find((item) => item.id === message.id);
    if (!cached) {
      return {
        ...message,
        metadata: {
          ...(message.metadata || {}),
          hidden: !visible,
          ...(turnUserMessageId ? { historyProcessUserMessageId: turnUserMessageId } : {}),
          ...(explicitProcessTurnId ? { historyProcessTurnId: explicitProcessTurnId } : {}),
          historyProcessExpanded: expanded,
        },
      };
    }

    return {
      ...cached,
      metadata: {
        ...(cached.metadata || {}),
        hidden: !visible,
        ...(turnUserMessageId ? { historyProcessUserMessageId: turnUserMessageId } : {}),
        ...(explicitProcessTurnId ? { historyProcessTurnId: explicitProcessTurnId } : {}),
        historyProcessLoaded: true,
        historyProcessPlaceholder: false,
        historyProcessExpanded: expanded,
      },
    };
  });
};
