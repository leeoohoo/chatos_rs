import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  ADMIN_AUTH_CHANGED_EVENT,
  ADMIN_AUTH_TOKEN_KEY,
  clearAuthToken,
  getAuthToken,
  migrateLegacyAuthToken,
  setAuthToken,
} from './tokenStore';

describe('administrator token storage', () => {
  beforeEach(() => {
    clearAuthToken();
    localStorage.clear();
  });

  it('keeps bearer tokens only in page memory', () => {
    const listener = vi.fn();
    window.addEventListener(ADMIN_AUTH_CHANGED_EVENT, listener);

    setAuthToken('admin-secret');

    expect(getAuthToken()).toBe('admin-secret');
    expect(localStorage.getItem(ADMIN_AUTH_TOKEN_KEY)).toBeNull();
    expect(listener).toHaveBeenCalledOnce();
    window.removeEventListener(ADMIN_AUTH_CHANGED_EVENT, listener);
  });

  it('purges legacy persisted tokens instead of migrating them into memory', () => {
    localStorage.setItem(ADMIN_AUTH_TOKEN_KEY, 'persisted-admin-secret');
    localStorage.setItem('user_service_auth_token', 'legacy-secret');

    expect(migrateLegacyAuthToken()).toBeNull();
    expect(localStorage.getItem(ADMIN_AUTH_TOKEN_KEY)).toBeNull();
    expect(localStorage.getItem('user_service_auth_token')).toBeNull();
  });
});
