// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { normalizeUiLocale } from './index.js';

export function createStoredUiLocaleHook(reactHooks) {
  const { useCallback, useEffect, useState } = reactHooks;

  return function useStoredUiLocale(options) {
    const {
      storageKey,
      supportedLocales,
      fallbackLocale,
      storage = globalThis.window?.localStorage,
      documentElement = globalThis.document?.documentElement,
      persist = 'effect',
      ignoreStorageErrors = false,
      updateDocumentLanguage = true,
    } = options;

    const readLocale = () => {
      try {
        return normalizeUiLocale(storage?.getItem(storageKey), supportedLocales, fallbackLocale);
      } catch (error) {
        if (!ignoreStorageErrors) {
          throw error;
        }
        return fallbackLocale;
      }
    };

    const writeLocale = (locale) => {
      try {
        storage?.setItem(storageKey, locale);
      } catch (error) {
        if (!ignoreStorageErrors) {
          throw error;
        }
      }
    };

    const [locale, setLocaleState] = useState(readLocale);
    const setLocale = useCallback(
      (nextLocale) => {
        const normalized = normalizeUiLocale(nextLocale, supportedLocales, fallbackLocale);
        if (persist === 'setter') {
          writeLocale(normalized);
        }
        setLocaleState(normalized);
      },
      [fallbackLocale, ignoreStorageErrors, persist, storage, storageKey, supportedLocales],
    );

    useEffect(() => {
      if (persist === 'effect') {
        writeLocale(locale);
      }
      if (updateDocumentLanguage && documentElement) {
        documentElement.lang = locale;
      }
    }, [
      documentElement,
      ignoreStorageErrors,
      locale,
      persist,
      storage,
      storageKey,
      updateDocumentLanguage,
    ]);

    return [locale, setLocale];
  };
}
