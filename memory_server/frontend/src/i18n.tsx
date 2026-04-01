import React, { createContext, useContext, useMemo, useState } from 'react';
import { enUS } from './locales/en-US';
import { zhCN } from './locales/zh-CN';

export type Lang = 'zh-CN' | 'en-US';

type Dict = Record<string, string>;
const DICTS: Record<Lang, Dict> = {
  'zh-CN': zhCN,
  'en-US': enUS,
};

type I18nValue = {
  lang: Lang;
  setLang: (lang: Lang) => void;
  t: (key: string) => string;
};

const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({ children }: { children: React.ReactNode }) {
  const [lang, setLang] = useState<Lang>(() => {
    const saved = localStorage.getItem('memory_frontend_lang');
    return saved === 'en-US' ? 'en-US' : 'zh-CN';
  });

  const value = useMemo<I18nValue>(() => {
    const dict = DICTS[lang];
    return {
      lang,
      setLang: (next) => {
        localStorage.setItem('memory_frontend_lang', next);
        setLang(next);
      },
      t: (key: string) => dict[key] || key,
    };
  }, [lang]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n() {
  const ctx = useContext(I18nContext);
  if (!ctx) {
    throw new Error('useI18n must be used inside I18nProvider');
  }
  return ctx;
}
