// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import {
  createTranslator,
} from '@chatos/frontend-runtime';
import { createStoredUiLocaleHook } from '@chatos/frontend-runtime/react';

import { UI_MESSAGES, type UiLocale } from './messages';

const UI_LOCALE_STORAGE_KEY = 'chat_ui_locale';

type I18nContextValue = {
  locale: UiLocale;
  setLocale: (locale: UiLocale) => void;
  t: (key: string, params?: Record<string, string | number>) => string;
};

export type TranslateFn = I18nContextValue['t'];

const SUPPORTED_LOCALES: readonly UiLocale[] = ['zh-CN', 'en-US'];

const buildTranslator = (locale: UiLocale) =>
  createTranslator({
    locale,
    messages: UI_MESSAGES,
    fallbackLocale: 'zh-CN',
  });

const defaultI18nContext: I18nContextValue = {
  locale: 'zh-CN',
  setLocale: () => undefined,
  t: buildTranslator('zh-CN'),
};

const I18nContext = React.createContext<I18nContextValue>(defaultI18nContext);
const useStoredUiLocale = createStoredUiLocaleHook(React);

export const I18nProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [locale, setLocale] = useStoredUiLocale({
    storageKey: UI_LOCALE_STORAGE_KEY,
    supportedLocales: SUPPORTED_LOCALES,
    fallbackLocale: 'zh-CN',
    persist: 'setter',
    ignoreStorageErrors: true,
  });

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
