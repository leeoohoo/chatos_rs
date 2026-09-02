import { describe, expect, it } from 'vitest';

import {
  ADMIN_LOCALE_STORAGE_KEY,
  normalizeAdminLocale,
  readStoredAdminLocale,
} from './AdminI18nProvider';

function memoryStorage(seed: Record<string, string> = {}) {
  const values = new Map(Object.entries(seed));
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
    values,
  };
}

describe('admin locale storage', () => {
  it('defaults to Chinese for missing or unsupported values', () => {
    expect(normalizeAdminLocale('fr-FR')).toBeNull();
    expect(readStoredAdminLocale(memoryStorage())).toBe('zh-CN');
  });

  it('migrates the Task Runner locale into the unified storage key', () => {
    const storage = memoryStorage({ chat_ui_locale: 'en-US' });
    expect(readStoredAdminLocale(storage)).toBe('en-US');
    expect(storage.values.get(ADMIN_LOCALE_STORAGE_KEY)).toBe('en-US');
  });

  it('prefers the unified locale over legacy module values', () => {
    const storage = memoryStorage({
      [ADMIN_LOCALE_STORAGE_KEY]: 'zh-CN',
      plugin_management_service_locale: 'en-US',
    });
    expect(readStoredAdminLocale(storage)).toBe('zh-CN');
  });
});
