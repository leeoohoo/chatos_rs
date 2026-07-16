// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import assert from 'node:assert/strict';
import test from 'node:test';

import {
  buildApiUrl,
  createBrowserAuthTokenStore,
  createJsonApiClient,
  createTranslator,
  formatDateTime,
  formatFileSize,
  interpolateMessage,
  normalizeApiBaseUrl,
  normalizeUiLocale,
  withQuery,
} from './index.js';
import { createStoredUiLocaleHook } from './react.js';
import { createStandardAdminAppShell } from './antd.js';

function createFakeReactHooks() {
  let initialized = false;
  let state;
  let effects = [];
  return {
    hooks: {
      useState(initialState) {
        if (!initialized) {
          state = typeof initialState === 'function' ? initialState() : initialState;
          initialized = true;
        }
        return [state, (nextState) => {
          state = typeof nextState === 'function' ? nextState(state) : nextState;
        }];
      },
      useCallback(callback) {
        return callback;
      },
      useEffect(effect) {
        effects.push(effect);
      },
    },
    flushEffects() {
      const pending = effects;
      effects = [];
      pending.forEach((effect) => effect());
    },
  };
}

function findElement(node, type) {
  if (!node || typeof node !== 'object') {
    return undefined;
  }
  if (node.type === type) {
    return node;
  }
  for (const child of node.props?.children || []) {
    const found = findElement(child, type);
    if (found) {
      return found;
    }
  }
  return undefined;
}

test('normalizes API bases and paths', () => {
  assert.equal(normalizeApiBaseUrl(' https://example.test/api/// '), 'https://example.test');
  assert.equal(normalizeApiBaseUrl('/service/', { stripApiSuffix: false }), '/service');
  assert.equal(buildApiUrl('/service', 'api/health'), '/service/api/health');
});

test('query builder omits empty values and preserves false', () => {
  assert.equal(
    withQuery('/api/items', { query: ' value ', empty: '', missing: undefined, enabled: false }),
    '/api/items?query=value&enabled=false',
  );
});

test('display formatters preserve local date-time and binary file-size conventions', () => {
  assert.equal(formatDateTime(undefined), '-');
  assert.equal(formatDateTime('invalid'), 'Invalid Date');
  assert.equal(formatDateTime('2026-01-02 03:04:05'), '2026-01-02 03:04:05');
  assert.equal(formatDateTime(null, { fallback: 'none' }), 'none');
  assert.equal(formatFileSize(512), '512 B');
  assert.equal(formatFileSize(1536), '1.5 KB');
  assert.equal(formatFileSize(2 * 1024 * 1024), '2.0 MB');
});

test('locale and translation helpers preserve fallbacks and placeholders', () => {
  assert.equal(normalizeUiLocale('invalid', ['zh-CN', 'en-US'], 'zh-CN'), 'zh-CN');
  assert.equal(interpolateMessage('Hello {name} {missing}', { name: 'Codex' }), 'Hello Codex {missing}');
  const translate = createTranslator({
    locale: 'en-US',
    fallbackLocale: 'zh-CN',
    messages: {
      'zh-CN': { fallback: '回退 {count}' },
      'en-US': { greeting: 'Hello {name}' },
    },
  });
  assert.equal(translate('greeting', { name: 'Codex' }), 'Hello Codex');
  assert.equal(translate('fallback', { count: 2 }), '回退 2');
});

test('browser auth store persists tokens and emits change events', () => {
  const values = new Map();
  const events = [];
  const store = createBrowserAuthTokenStore({
    storageKey: 'token',
    changeEvent: 'auth-changed',
    storage: {
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => values.set(key, value),
      removeItem: (key) => values.delete(key),
    },
    eventTarget: {
      dispatchEvent: (event) => {
        events.push(event.type);
        return true;
      },
    },
  });

  store.setAuthToken('secret');
  assert.equal(store.getAuthToken(), 'secret');
  store.clearAuthToken();
  assert.equal(store.getAuthToken(), null);
  assert.deepEqual(events, ['auth-changed', 'auth-changed']);
});

test('stored UI locale hook preserves persistence timing and document language updates', () => {
  const values = new Map([['locale', 'en-US']]);
  const documentElement = { lang: '' };
  const fakeReact = createFakeReactHooks();
  const useStoredUiLocale = createStoredUiLocaleHook(fakeReact.hooks);
  const options = {
    storageKey: 'locale',
    supportedLocales: ['zh-CN', 'en-US'],
    fallbackLocale: 'zh-CN',
    storage: {
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => values.set(key, value),
      removeItem: (key) => values.delete(key),
    },
    documentElement,
  };

  let [locale, setLocale] = useStoredUiLocale(options);
  fakeReact.flushEffects();
  assert.equal(locale, 'en-US');
  assert.equal(documentElement.lang, 'en-US');

  setLocale('zh-CN');
  [locale, setLocale] = useStoredUiLocale(options);
  assert.equal(values.get('locale'), 'en-US');
  fakeReact.flushEffects();
  assert.equal(locale, 'zh-CN');
  assert.equal(values.get('locale'), 'zh-CN');
  assert.equal(documentElement.lang, 'zh-CN');
});

test('stored UI locale hook can ignore unavailable browser storage', () => {
  const fakeReact = createFakeReactHooks();
  const useStoredUiLocale = createStoredUiLocaleHook(fakeReact.hooks);
  const options = {
    storageKey: 'locale',
    supportedLocales: ['zh-CN', 'en-US'],
    fallbackLocale: 'zh-CN',
    persist: 'setter',
    ignoreStorageErrors: true,
    updateDocumentLanguage: false,
    storage: {
      getItem: () => {
        throw new Error('storage unavailable');
      },
      setItem: () => {
        throw new Error('storage unavailable');
      },
      removeItem: () => undefined,
    },
  };

  let [locale, setLocale] = useStoredUiLocale(options);
  assert.equal(locale, 'zh-CN');
  assert.doesNotThrow(() => setLocale('en-US'));
  [locale, setLocale] = useStoredUiLocale(options);
  assert.equal(locale, 'en-US');
});

test('standard admin shell keeps shared navigation and user actions configurable', () => {
  const navigations = [];
  const Layout = { Header: 'Header', Sider: 'Sider', Content: 'Content' };
  const createElement = (type, props, ...children) => ({
    type,
    props: { ...props, children },
  });
  const StandardAdminAppShell = createStandardAdminAppShell({
    createElement,
    Layout,
    Menu: 'Menu',
    Space: 'Space',
    Typography: { Title: 'Title', Text: 'Text' },
    Button: 'Button',
    Outlet: 'Outlet',
    useLocation: () => ({ pathname: '/users' }),
    useNavigate: () => (path) => navigations.push(path),
    UserIcon: 'UserIcon',
    LogoutIcon: 'LogoutIcon',
  });
  const onLogout = () => undefined;
  const tree = StandardAdminAppShell({
    brandTitle: 'Admin',
    brandSubtitle: 'Control plane',
    headerSummary: 'Summary',
    navItems: [{ key: '/users', label: 'Users' }],
    currentUser: { username: 'alice', display_name: 'Alice' },
    logoutLabel: 'Logout',
    onLogout,
    headerBeforeUser: 'locale-control',
  });

  const menu = findElement(tree, 'Menu');
  assert.deepEqual(menu.props.selectedKeys, ['/users']);
  menu.props.onClick({ key: '/models' });
  assert.deepEqual(navigations, ['/models']);
  const button = findElement(tree, 'Button');
  assert.equal(button.props.onClick, onLogout);
  assert.match(JSON.stringify(tree), /Alice/);
  assert.match(JSON.stringify(tree), /locale-control/);
});

test('JSON client adds bearer auth and parses empty success responses', async () => {
  const calls = [];
  const request = createJsonApiClient({
    baseUrl: '/service',
    getAuthToken: () => 'secret',
    fetchImpl: async (url, init) => {
      calls.push({ url, init });
      return new Response(null, { status: 204 });
    },
  });

  assert.equal(await request('/api/items'), undefined);
  assert.equal(calls[0].url, '/service/api/items');
  assert.equal(calls[0].init.headers.get('Authorization'), 'Bearer secret');
  assert.equal(calls[0].init.headers.get('Content-Type'), 'application/json');
});

test('JSON client clears unauthorized auth and exposes API errors', async () => {
  let unauthorized = 0;
  const request = createJsonApiClient({
    onUnauthorized: () => {
      unauthorized += 1;
    },
    fetchImpl: async () =>
      new Response(JSON.stringify({ error: 'token expired' }), {
        status: 401,
        statusText: 'Unauthorized',
        headers: { 'Content-Type': 'application/json' },
      }),
  });

  await assert.rejects(() => request('/api/me'), /token expired/);
  assert.equal(unauthorized, 1);
});

test('JSON client preserves service-specific structured errors', async () => {
  let unauthorized = 0;
  class StructuredError extends Error {
    constructor(message, status, code) {
      super(message);
      this.status = status;
      this.code = code;
    }
  }

  const request = createJsonApiClient({
    onUnauthorized: () => {
      unauthorized += 1;
    },
    fetchImpl: async () =>
      new Response(JSON.stringify({ error: { code: 'TOKEN_EXPIRED', message: 'sign in again' } }), {
        status: 401,
        statusText: 'Unauthorized',
        headers: { 'Content-Type': 'application/json' },
      }),
    createResponseError: async (response) => {
      const body = await response.json();
      return new StructuredError(body.error.message, response.status, body.error.code);
    },
  });

  await assert.rejects(() => request('/api/me'), (error) => {
    assert.equal(error.message, 'sign in again');
    assert.equal(error.status, 401);
    assert.equal(error.code, 'TOKEN_EXPIRED');
    return true;
  });
  assert.equal(unauthorized, 1);
});

test('JSON client supports service-specific success readers', async () => {
  const request = createJsonApiClient({
    fetchImpl: async () => new Response(JSON.stringify({ ok: true }), { status: 200 }),
    readSuccessResponse: async (response) => ({ wrapped: await response.json() }),
  });
  assert.deepEqual(await request('/api/value'), { wrapped: { ok: true } });
});
