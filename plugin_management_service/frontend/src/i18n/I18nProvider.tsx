// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { ConfigProvider } from 'antd';
import enUS from 'antd/locale/en_US';
import zhCN from 'antd/locale/zh_CN';

import { createTranslator } from '@chatos/frontend-runtime';
import { createStoredUiLocaleHook } from '@chatos/frontend-runtime/react';

import { enUSMessages, zhCNMessages } from './messages';

export type AppLocale = 'zh-CN' | 'en-US';

interface I18nContextValue {
  locale: AppLocale;
  setLocale: (locale: AppLocale) => void;
  t: (key: string, values?: Record<string, string | number>) => string;
}

const STORAGE_KEY = 'plugin_management_service_locale';
const SUPPORTED_LOCALES: readonly AppLocale[] = ['zh-CN', 'en-US'];
const MESSAGE_CATALOG = {
  'zh-CN': zhCNMessages,
  'en-US': enUSMessages,
};

const I18nContext = createContext<I18nContextValue | null>(null);
const useStoredUiLocale = createStoredUiLocaleHook({ useCallback, useEffect, useState });

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocale] = useStoredUiLocale({
    storageKey: STORAGE_KEY,
    supportedLocales: SUPPORTED_LOCALES,
    fallbackLocale: 'zh-CN',
    persist: 'effect',
  });

  const value = useMemo<I18nContextValue>(() => {
    return {
      locale,
      setLocale,
      t: createTranslator({
        locale,
        messages: MESSAGE_CATALOG,
        fallbackLocale: 'en-US',
      }),
    };
  }, [locale]);

  return (
    <I18nContext.Provider value={value}>
      <ConfigProvider locale={locale === 'zh-CN' ? zhCN : enUS}>{children}</ConfigProvider>
    </I18nContext.Provider>
  );
}

export function useI18n(): I18nContextValue {
  const value = useContext(I18nContext);
  if (!value) {
    throw new Error('useI18n must be used inside I18nProvider');
  }
  return value;
}
