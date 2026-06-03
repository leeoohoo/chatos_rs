export const CHAT_STORE_PERSIST_PREFIX = 'chat-store-with-backend';
export const LEGACY_CHAT_STORE_PERSIST_KEY = CHAT_STORE_PERSIST_PREFIX;

const ANONYMOUS_CHAT_STORE_KEY = `${CHAT_STORE_PERSIST_PREFIX}:anonymous`;

const canUseLocalStorage = () => typeof localStorage !== 'undefined';

const normalizeUserId = (userId?: string | null) => {
  const normalized = typeof userId === 'string' ? userId.trim() : '';
  return normalized || null;
};

export const resolveChatStorePersistKey = (userId?: string | null) => {
  const normalizedUserId = normalizeUserId(userId);
  if (!normalizedUserId) {
    return ANONYMOUS_CHAT_STORE_KEY;
  }
  return `${CHAT_STORE_PERSIST_PREFIX}:${normalizedUserId}`;
};

export const primeScopedChatStoreStateFromLegacy = (userId?: string | null) => {
  const normalizedUserId = normalizeUserId(userId);
  if (!normalizedUserId || !canUseLocalStorage()) {
    return;
  }

  const targetKey = resolveChatStorePersistKey(normalizedUserId);
  if (localStorage.getItem(targetKey) !== null) {
    return;
  }

  const legacyState = localStorage.getItem(LEGACY_CHAT_STORE_PERSIST_KEY);
  if (legacyState === null) {
    return;
  }

  localStorage.setItem(targetKey, legacyState);
  localStorage.removeItem(LEGACY_CHAT_STORE_PERSIST_KEY);
};

export const clearLegacyChatStoreState = () => {
  if (!canUseLocalStorage()) {
    return;
  }
  localStorage.removeItem(LEGACY_CHAT_STORE_PERSIST_KEY);
};

export const clearAnonymousChatStoreState = () => {
  if (!canUseLocalStorage()) {
    return;
  }
  localStorage.removeItem(ANONYMOUS_CHAT_STORE_KEY);
};
