// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { UI_MESSAGES, type UiLocale } from './messages';

const UI_LOCALE_STORAGE_KEY = 'chat_ui_locale';

type I18nContextValue = {
  locale: UiLocale;
  setLocale: (locale: UiLocale) => void;
  t: (key: string, params?: Record<string, string | number>) => string;
};

export type TranslateFn = I18nContextValue['t'];

const normalizeLocale = (value: unknown): UiLocale => (
  value === 'en-US' ? 'en-US' : 'zh-CN'
);

const formatMessage = (
  template: string,
  params?: Record<string, string | number>,
): string => {
  if (!params) {
    return template;
  }

  return template.replace(/\{(\w+)\}/g, (_match, key: string) => (
    Object.prototype.hasOwnProperty.call(params, key)
      ? String(params[key])
      : `{${key}}`
  ));
};

const readStoredLocale = (): UiLocale => {
  if (typeof window === 'undefined') {
    return 'zh-CN';
  }

  try {
    return normalizeLocale(window.localStorage.getItem(UI_LOCALE_STORAGE_KEY));
  } catch {
    return 'zh-CN';
  }
};

const writeStoredLocale = (locale: UiLocale) => {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(UI_LOCALE_STORAGE_KEY, locale);
  } catch {
    // Ignore storage failures; the in-memory locale still updates.
  }
};

const buildTranslator = (locale: UiLocale) => (
  (key: string, params?: Record<string, string | number>) => {
    const currentDictionary = UI_MESSAGES[locale] || UI_MESSAGES['zh-CN'];
    const fallbackDictionary = UI_MESSAGES['zh-CN'];
    const template = currentDictionary[key] || fallbackDictionary[key] || key;
    return formatMessage(template, params);
  }
);

const defaultI18nContext: I18nContextValue = {
  locale: 'zh-CN',
  setLocale: () => undefined,
  t: buildTranslator('zh-CN'),
};

const I18nContext = React.createContext<I18nContextValue>(defaultI18nContext);

export const I18nProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [locale, setLocaleState] = React.useState<UiLocale>(() => readStoredLocale());

  const setLocale = React.useCallback((nextLocale: UiLocale) => {
    const normalized = normalizeLocale(nextLocale);
    setLocaleState(normalized);
    writeStoredLocale(normalized);
  }, []);

  React.useEffect(() => {
    if (typeof document !== 'undefined') {
      document.documentElement.lang = locale;
    }
  }, [locale]);

  const value = React.useMemo<I18nContextValue>(() => ({
    locale,
    setLocale,
    t: buildTranslator(locale),
  }), [locale, setLocale]);

  return (
    <I18nContext.Provider value={value}>
      {children}
    </I18nContext.Provider>
  );
};

export const useI18n = (): I18nContextValue => React.useContext(I18nContext);
