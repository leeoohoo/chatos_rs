// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { StorageLike } from './index.js';

export type StoredUiLocalePersistMode = 'effect' | 'setter';

export interface DocumentElementLike {
  lang: string;
}

export interface StoredUiLocaleOptions<T extends string> {
  storageKey: string;
  supportedLocales: readonly T[];
  fallbackLocale: T;
  storage?: StorageLike;
  documentElement?: DocumentElementLike | null;
  persist?: StoredUiLocalePersistMode;
  ignoreStorageErrors?: boolean;
  updateDocumentLanguage?: boolean;
}

export interface ReactStateHooks {
  useState<T>(initialState: T | (() => T)): [
    T,
    (value: T | ((previousValue: T) => T)) => void,
  ];
  useCallback<T extends (...args: never[]) => unknown>(
    callback: T,
    dependencies: readonly unknown[],
  ): T;
  useEffect(
    effect: () => void | (() => void),
    dependencies?: readonly unknown[],
  ): void;
}

export type StoredUiLocaleHook = <T extends string>(
  options: StoredUiLocaleOptions<T>,
) => readonly [T, (locale: T) => void];

export function createStoredUiLocaleHook(reactHooks: ReactStateHooks): StoredUiLocaleHook;
