export type QueryValue = string | number | boolean | null | undefined;

export interface NormalizeApiBaseUrlOptions {
  stripApiSuffix?: boolean;
}

export function normalizeApiBaseUrl(
  value: string | null | undefined,
  options?: NormalizeApiBaseUrlOptions,
): string;

export function buildApiUrl(baseUrl: string, path: string): string;

export function withQuery(
  path: string,
  params: Record<string, QueryValue>,
): string;

export interface FormatDateTimeOptions {
  fallback?: string;
  invalid?: string;
}

export function formatDateTime(
  value: Date | string | null | undefined,
  options?: FormatDateTimeOptions,
): string;

export function formatFileSize(bytes: number): string;

export function normalizeUiLocale<T extends string>(
  value: unknown,
  supportedLocales: readonly T[],
  fallbackLocale: T,
): T;

export function interpolateMessage(
  template: string,
  values?: Record<string, string | number>,
): string;

export type MessageDictionary = Record<string, string>;

export interface TranslatorOptions<T extends string> {
  locale: T;
  messages: Record<T, MessageDictionary>;
  fallbackLocale: T;
}

export type TranslateFn = (
  key: string,
  values?: Record<string, string | number>,
) => string;

export function createTranslator<T extends string>(
  options: TranslatorOptions<T>,
): TranslateFn;

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export interface EventTargetLike {
  dispatchEvent(event: Event): boolean;
}

export interface BrowserAuthTokenStoreOptions {
  storageKey: string;
  changeEvent?: string;
  storage?: StorageLike;
  eventTarget?: EventTargetLike;
}

export interface BrowserAuthTokenStore {
  getAuthToken(): string | null;
  setAuthToken(token: string): void;
  clearAuthToken(): void;
}

export function createBrowserAuthTokenStore(
  options: BrowserAuthTokenStoreOptions,
): BrowserAuthTokenStore;

export function readApiErrorMessage(response: Response): Promise<string>;

export function readJsonResponse(response: Response): Promise<unknown>;

export type JsonRequest = <T>(path: string, init?: RequestInit) => Promise<T>;

export interface JsonApiClientOptions {
  baseUrl?: string;
  getAuthToken?: () => string | null;
  onUnauthorized?: () => void;
  fetchImpl?: typeof fetch;
  readErrorMessage?: (response: Response) => Promise<string>;
  createResponseError?: (response: Response) => Promise<Error>;
  readSuccessResponse?: (response: Response) => Promise<unknown>;
  overrideContentType?: boolean;
}

export function createJsonApiClient(options?: JsonApiClientOptions): JsonRequest;
