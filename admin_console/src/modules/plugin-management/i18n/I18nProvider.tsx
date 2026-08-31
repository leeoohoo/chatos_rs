// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  createContext,
  useContext,
  useMemo,
  type ReactNode,
} from 'react';

import { createTranslator } from '@chatos/frontend-runtime';

import { useAdminI18n } from '../../../app/i18n/AdminI18nProvider';
import { enUSMessages, zhCNMessages } from './messages';

export type AppLocale = 'zh-CN' | 'en-US';

interface I18nContextValue {
  locale: AppLocale;
  setLocale: (locale: AppLocale) => void;
  t: (key: string, values?: Record<string, string | number>) => string;
}

const MESSAGE_CATALOG = {
  'zh-CN': zhCNMessages,
  'en-US': enUSMessages,
};

const I18nContext = createContext<I18nContextValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const { locale, setLocale } = useAdminI18n();

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

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nContextValue {
  const value = useContext(I18nContext);
  if (!value) {
    throw new Error('useI18n must be used inside I18nProvider');
  }
  return value;
}
