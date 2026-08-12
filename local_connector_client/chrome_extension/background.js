// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const NATIVE_HOST = 'com.chatos.chrome';
const CLAIMED_TABS_KEY = 'claimed_tabs';
const TARGET_STATES_KEY = 'target_states';
const TARGET_ATTRIBUTE = 'data-chatos-target';
const MAX_CLAIMED_TABS = 64;
const MAX_COMMAND_CHARS = 10_000_000;
const MAX_COMMAND_RESULTS = 500;
const MAX_TEXT_CHARS = 2_000;
const MAX_SCREENSHOT_BYTES = 700 * 1024;
const MAX_UPLOAD_BYTES = 10 * 1024 * 1024;
const MAX_UPLOAD_CHUNKS = 64;
const MAX_UPLOAD_CHUNK_BYTES = 192 * 1024;
const MAX_DOWNLOAD_BYTES = 10 * 1024 * 1024;
const MAX_DOWNLOAD_CHUNKS = 64;
const DOWNLOAD_CHUNK_BYTES = 192 * 1024;
const MAX_DOWNLOAD_DATA_URL_CHARS = 14 * 1024 * 1024 + 4096;

let nativePort = null;
let reconnectTimer = null;
let reconnectAttempt = 0;
const commandAbortControllers = new Map();

const storageArea = chrome.storage.session || chrome.storage.local;

function safeOriginAndPattern(rawUrl) {
  try {
    const url = new URL(rawUrl);
    if (url.protocol !== 'http:' && url.protocol !== 'https:') {
      return null;
    }
    if (url.port) {
      return null;
    }
    const origin = `${url.protocol}//${url.hostname}`;
    return { origin, pattern: `${origin}/*` };
  } catch {
    return null;
  }
}

async function loadClaims() {
  const stored = await storageArea.get(CLAIMED_TABS_KEY);
  const claims = stored?.[CLAIMED_TABS_KEY];
  return claims && typeof claims === 'object' && !Array.isArray(claims) ? claims : {};
}

async function saveClaims(claims) {
  await storageArea.set({ [CLAIMED_TABS_KEY]: claims });
  await publishState();
}

async function loadTargetStates() {
  const stored = await storageArea.get(TARGET_STATES_KEY);
  const states = stored?.[TARGET_STATES_KEY];
  return states && typeof states === 'object' && !Array.isArray(states) ? states : {};
}

async function saveTargetStates(states) {
  await storageArea.set({ [TARGET_STATES_KEY]: states });
}

async function saveTargetState(tabId, origin, snapshotId, targets) {
  const states = await loadTargetStates();
  states[String(tabId)] = {
    origin,
    snapshot_id: snapshotId,
    targets,
  };
  await saveTargetStates(states);
}

async function clearTargetState(tabId) {
  const states = await loadTargetStates();
  const key = String(tabId);
  if (states[key]) {
    delete states[key];
    await saveTargetStates(states);
  }
}

async function sitePermissionGranted(pattern) {
  return chrome.permissions.contains({ origins: [pattern] });
}

async function currentActiveTab() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  return tab || null;
}

async function claimActiveTab() {
  const tab = await currentActiveTab();
  const site = safeOriginAndPattern(tab?.url || '');
  if (!tab?.id || tab.incognito || !site) {
    throw new Error('Only a normal HTTP(S) tab can be connected.');
  }
  if (!(await sitePermissionGranted(site.pattern))) {
    throw new Error('Site permission was not granted for this tab.');
  }
  const claims = await loadClaims();
  if (!claims[String(tab.id)] && Object.keys(claims).length >= MAX_CLAIMED_TABS) {
    throw new Error('The connected tab limit has been reached.');
  }
  claims[String(tab.id)] = {
    origin: site.origin,
    claimed_at: new Date().toISOString(),
  };
  await saveClaims(claims);
  return popupStatus();
}

async function releaseTab(tabId) {
  const claims = await loadClaims();
  const key = String(tabId);
  const released = Boolean(claims[key]);
  delete claims[key];
  await clearTargetState(tabId);
  await saveClaims(claims);
  return released;
}

async function releaseOrigin(origin) {
  const claims = await loadClaims();
  let changed = false;
  for (const [tabId, claim] of Object.entries(claims)) {
    if (claim?.origin === origin) {
      delete claims[tabId];
      await clearTargetState(tabId);
      changed = true;
    }
  }
  if (changed) {
    await saveClaims(claims);
  } else {
    await publishState();
  }
}

async function popupStatus() {
  const tab = await currentActiveTab();
  const site = safeOriginAndPattern(tab?.url || '');
  const claims = await loadClaims();
  const claim = tab?.id ? claims[String(tab.id)] : null;
  const permissionGranted = site ? await sitePermissionGranted(site.pattern) : false;
  return {
    connected_to_native_host: Boolean(nativePort),
    tab_supported: Boolean(tab?.id && !tab.incognito && site),
    tab_id: tab?.id || null,
    title: typeof tab?.title === 'string' ? tab.title.slice(0, 200) : '',
    origin: site?.origin || null,
    pattern: site?.pattern || null,
    permission_granted: permissionGranted,
    claimed: Boolean(claim && site && claim.origin === site.origin && permissionGranted),
  };
}

function connectNativeHost() {
  if (nativePort) {
    return;
  }
  try {
    const port = chrome.runtime.connectNative(NATIVE_HOST);
    nativePort = port;
    port.onMessage.addListener((message) => {
      void handleNativeMessage(message);
    });
    port.onDisconnect.addListener(() => {
      if (nativePort === port) {
        nativePort = null;
      }
      void saveClaims({});
      void saveTargetStates({});
      scheduleReconnect();
    });
    reconnectAttempt = 0;
    sendNative({
      type: 'hello',
      extension_id: chrome.runtime.id,
      extension_version: chrome.runtime.getManifest().version,
    });
    void publishState();
  } catch {
    nativePort = null;
    scheduleReconnect();
  }
}

function scheduleReconnect() {
  if (reconnectTimer) {
    return;
  }
  const delay = Math.min(500 * (2 ** Math.min(reconnectAttempt, 5)), 15_000);
  reconnectAttempt += 1;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connectNativeHost();
  }, delay);
}

function sendNative(message) {
  if (!nativePort) {
    return false;
  }
  try {
    nativePort.postMessage(message);
    return true;
  } catch {
    return false;
  }
}

async function publishState() {
  const claims = await loadClaims();
  const permissions = await chrome.permissions.getAll();
  sendNative({
    type: 'state',
    claimed_tab_count: Math.min(Object.keys(claims).length, MAX_CLAIMED_TABS),
    authorized_origin_count: Math.min(permissions.origins?.length || 0, 10_000),
  });
}

async function handleNativeMessage(message) {
  if (!message || typeof message !== 'object') {
    return;
  }
  if (message.type === 'host_ready') {
    sendNative({
      type: 'hello',
      extension_id: chrome.runtime.id,
      extension_version: chrome.runtime.getManifest().version,
    });
    await publishState();
    return;
  }
  if (message.type === 'cancel' && validRequestId(message.request_id)) {
    commandAbortControllers.get(message.request_id)?.abort();
    return;
  }
  if (message.type !== 'command' || !validRequestId(message.request_id)) {
    return;
  }
  const controller = new AbortController();
  commandAbortControllers.set(message.request_id, controller);
  try {
    const result = await executeCommand(
      message.command,
      message.arguments || {},
      controller.signal,
    );
    throwIfAborted(controller.signal);
    sendNative({
      type: 'command_result',
      request_id: message.request_id,
      ok: true,
      result,
    });
  } catch (error) {
    sendNative({
      type: 'command_result',
      request_id: message.request_id,
      ok: false,
      error: safeError(error),
    });
  } finally {
    commandAbortControllers.delete(message.request_id);
  }
}

function validRequestId(value) {
  return typeof value === 'string' && /^[0-9a-f-]{36}$/i.test(value);
}

async function executeCommand(command, argumentsValue, signal) {
  throwIfAborted(signal);
  switch (command) {
    case 'tabs':
      return listConnectedTabs(argumentsValue);
    case 'snapshot':
      return snapshotConnectedTab(argumentsValue);
    case 'release_tab':
      return releaseConnectedTab(argumentsValue);
    case 'navigate':
      return navigateConnectedTab(argumentsValue, signal);
    case 'click':
      return clickConnectedTab(argumentsValue, signal);
    case 'type_text':
      return typeIntoConnectedTab(argumentsValue, signal);
    case 'select_option':
      return selectInConnectedTab(argumentsValue, signal);
    case 'scroll':
      return scrollConnectedTab(argumentsValue, signal);
    case 'history':
      return historyConnectedTab(argumentsValue, signal);
    case 'activate':
      return activateConnectedTab(argumentsValue, signal);
    case 'screenshot':
      return screenshotConnectedTab(argumentsValue, signal);
    case 'upload_begin':
      return uploadBegin(argumentsValue, signal);
    case 'upload_chunk':
      return uploadChunk(argumentsValue, signal);
    case 'upload_finish':
      return uploadFinish(argumentsValue, signal);
    case 'upload_abort':
      return uploadAbort(argumentsValue);
    case 'download_begin':
      return downloadBegin(argumentsValue, signal);
    case 'download_chunk':
      return downloadChunk(argumentsValue, signal);
    case 'download_finish':
      return downloadFinish(argumentsValue);
    case 'download_abort':
      return downloadAbort(argumentsValue);
    default:
      throw new Error('The requested Chrome command is not supported.');
  }
}

function throwIfAborted(signal) {
  if (signal?.aborted) {
    throw new Error('The Chrome action was cancelled.');
  }
}

async function authorizedClaimedTab(tabId) {
  const claims = await loadClaims();
  const claim = claims[String(tabId)];
  if (!claim || typeof claim.origin !== 'string') {
    throw new Error('The Chrome tab is not connected to ChatOS.');
  }
  let tab;
  try {
    tab = await chrome.tabs.get(tabId);
  } catch {
    await releaseTab(tabId);
    throw new Error('The connected Chrome tab no longer exists.');
  }
  const site = safeOriginAndPattern(tab.url || '');
  if (!site || site.origin !== claim.origin || !(await sitePermissionGranted(site.pattern))) {
    await releaseTab(tabId);
    throw new Error('The Chrome tab navigation or site permission changed.');
  }
  return tab;
}

function publicTab(tab) {
  return {
    tab_id: `ct${tab.id}`,
    window_id: `cw${tab.windowId}`,
    active: Boolean(tab.active),
    pinned: Boolean(tab.pinned),
    incognito: Boolean(tab.incognito),
    title: typeof tab.title === 'string' ? tab.title.slice(0, 512) : '',
    url: typeof tab.url === 'string' ? tab.url.slice(0, 8192) : '',
  };
}

async function listConnectedTabs(argumentsValue) {
  const requestedLimit = Number(argumentsValue?.limit || 20);
  const limit = Number.isInteger(requestedLimit) ? Math.min(Math.max(requestedLimit, 1), 50) : 20;
  const claims = await loadClaims();
  const tabs = [];
  for (const tabIdText of Object.keys(claims).slice(0, MAX_CLAIMED_TABS)) {
    const tabId = Number(tabIdText);
    if (!Number.isSafeInteger(tabId) || tabId <= 0) {
      await releaseTab(tabIdText);
      continue;
    }
    try {
      const tab = await authorizedClaimedTab(tabId);
      tabs.push(publicTab(tab));
    } catch {
      // Stale or unauthorized claims are removed by authorizedClaimedTab.
    }
    if (tabs.length >= limit) {
      break;
    }
  }
  return { tabs };
}

function parseStableTabId(value) {
  if (typeof value !== 'string' || !/^ct[1-9][0-9]{0,9}$/.test(value)) {
    throw new Error('A valid stable Chrome tab ID is required.');
  }
  const tabId = Number(value.slice(2));
  if (!Number.isSafeInteger(tabId) || tabId <= 0) {
    throw new Error('The Chrome tab ID is out of range.');
  }
  return tabId;
}

async function snapshotConnectedTab(argumentsValue) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const requestedMax = Number(argumentsValue?.max_chars || 20_000);
  const maxChars = Number.isInteger(requestedMax)
    ? Math.min(Math.max(requestedMax, 1), MAX_COMMAND_CHARS)
    : 20_000;
  const tab = await authorizedClaimedTab(tabId);
  const site = safeOriginAndPattern(tab.url || '');
  const snapshotId = randomHex(8);
  const results = await chrome.scripting.executeScript({
    target: { tabId },
    world: 'ISOLATED',
    func: buildBoundedPageSnapshot,
    args: [
      maxChars,
      MAX_COMMAND_RESULTS,
      snapshotId,
      TARGET_ATTRIBUTE,
      MAX_DOWNLOAD_DATA_URL_CHARS,
    ],
  });
  const result = results?.[0]?.result;
  if (!result || typeof result.snapshot !== 'string' || !Array.isArray(result.targets)) {
    throw new Error('The Chrome tab did not return a page snapshot.');
  }
  const targets = result.targets.slice(0, MAX_COMMAND_RESULTS).map((target) => {
    if (!target
      || typeof target.target_id !== 'string'
      || !/^cr[0-9a-f]{16}-[1-9][0-9]{0,3}$/.test(target.target_id)
      || typeof target.fingerprint !== 'string'
      || !/^[0-9a-f]{8}$/.test(target.fingerprint)
      || typeof target.kind !== 'string'
      || target.kind.length > 40) {
      throw new Error('The Chrome tab returned malformed action targets.');
    }
    return {
      target_id: target.target_id,
      fingerprint: target.fingerprint,
      kind: target.kind,
    };
  });
  await saveTargetState(tabId, site.origin, snapshotId, targets);
  return {
    tab: publicTab(tab),
    snapshot: result.snapshot.slice(0, maxChars),
    truncated: Boolean(result.truncated || result.snapshot.length > maxChars),
    target_count: targets.length,
    captured_at: new Date().toISOString(),
  };
}

async function releaseConnectedTab(argumentsValue) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  return { released: await releaseTab(tabId) };
}

function parseTargetId(value) {
  if (typeof value !== 'string' || !/^cr[0-9a-f]{16}-[1-9][0-9]{0,3}$/.test(value)) {
    throw new Error('A valid target from the latest Chrome snapshot is required.');
  }
  return value;
}

function parseUploadId(value) {
  if (typeof value !== 'string' || !/^[0-9a-f-]{36}$/i.test(value)) {
    throw new Error('A valid Chrome upload ID is required.');
  }
  return value;
}

async function boundTarget(tabId, targetId) {
  const tab = await authorizedClaimedTab(tabId);
  const site = safeOriginAndPattern(tab.url || '');
  const states = await loadTargetStates();
  const state = states[String(tabId)];
  const target = state?.targets?.find((candidate) => candidate?.target_id === targetId);
  if (!site
    || !state
    || state.origin !== site.origin
    || typeof state.snapshot_id !== 'string'
    || !target
    || !targetId.startsWith(`cr${state.snapshot_id}-`)) {
    throw new Error('The target is stale. Capture a fresh Chrome tab snapshot.');
  }
  return { tab, target };
}

async function runTargetAction(tabId, targetId, action, payload = {}) {
  const bound = await boundTarget(tabId, targetId);
  const results = await chrome.scripting.executeScript({
    target: { tabId },
    world: 'ISOLATED',
    func: runBoundTargetAction,
    args: [
      targetId,
      bound.target.fingerprint,
      TARGET_ATTRIBUTE,
      action,
      payload,
      MAX_DOWNLOAD_DATA_URL_CHARS,
    ],
  });
  const result = results?.[0]?.result;
  if (!result || result.ok !== true) {
    throw new Error(result?.error || 'The Chrome target action failed.');
  }
  return { tab: bound.tab, result };
}

async function navigateConnectedTab(argumentsValue, signal) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const tab = await authorizedClaimedTab(tabId);
  const currentSite = safeOriginAndPattern(tab.url || '');
  let targetUrl;
  try {
    targetUrl = new URL(String(argumentsValue?.url || ''));
  } catch {
    throw new Error('A valid HTTP(S) navigation URL is required.');
  }
  if (!currentSite
    || !['http:', 'https:'].includes(targetUrl.protocol)
    || targetUrl.username
    || targetUrl.password
    || targetUrl.origin !== currentSite.origin) {
    throw new Error('Chrome navigation is limited to the currently authorized exact origin.');
  }
  throwIfAborted(signal);
  await clearTargetState(tabId);
  const navigating = await chrome.tabs.update(tabId, { url: targetUrl.toString() });
  if (navigating.status !== 'complete' || navigating.url !== targetUrl.toString()) {
    await waitForTabComplete(tabId, signal, 10_000);
  }
  const updated = await authorizedClaimedTab(tabId);
  return {
    navigated: true,
    tab: publicTab(updated),
    target_scope: 'same_origin_only',
  };
}

async function waitForTabComplete(tabId, signal, timeoutMs) {
  throwIfAborted(signal);
  try {
    const current = await chrome.tabs.get(tabId);
    if (current.status === 'complete') return;
  } catch {
    throw new Error('The connected Chrome tab no longer exists.');
  }
  await new Promise((resolve, reject) => {
    let settled = false;
    const cleanup = () => {
      chrome.tabs.onUpdated.removeListener(onUpdated);
      signal?.removeEventListener('abort', onAbort);
      clearTimeout(timer);
    };
    const finish = (callback) => {
      if (settled) return;
      settled = true;
      cleanup();
      callback();
    };
    const onUpdated = (updatedTabId, changeInfo) => {
      if (updatedTabId === tabId && changeInfo.status === 'complete') {
        finish(resolve);
      }
    };
    const onAbort = () => finish(() => reject(new Error('The Chrome action was cancelled.')));
    const timer = setTimeout(
      () => finish(() => reject(new Error('Chrome navigation did not finish within 10 seconds.'))),
      timeoutMs,
    );
    chrome.tabs.onUpdated.addListener(onUpdated);
    signal?.addEventListener('abort', onAbort, { once: true });
  });
}

async function clickConnectedTab(argumentsValue, signal) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const targetId = parseTargetId(argumentsValue?.target_id);
  throwIfAborted(signal);
  const { result } = await runTargetAction(tabId, targetId, 'click');
  await clearTargetState(tabId);
  throwIfAborted(signal);
  return {
    clicked: true,
    tab_id: `ct${tabId}`,
    target_id: targetId,
    target_kind: result.target_kind,
    snapshot_required: true,
  };
}

async function typeIntoConnectedTab(argumentsValue, signal) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const targetId = parseTargetId(argumentsValue?.target_id);
  const text = typeof argumentsValue?.text === 'string' ? argumentsValue.text : '';
  if (!text
    || [...text].length > MAX_TEXT_CHARS
    || /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f\u200b-\u200f\u202a-\u202e\u2060-\u206f]/u.test(text)) {
    throw new Error(`Chrome text must contain 1-${MAX_TEXT_CHARS} visible characters without control or direction-format characters.`);
  }
  const replace = argumentsValue?.replace !== false;
  throwIfAborted(signal);
  const { result } = await runTargetAction(tabId, targetId, 'type_text', { text, replace });
  await clearTargetState(tabId);
  throwIfAborted(signal);
  return {
    typed: true,
    tab_id: `ct${tabId}`,
    target_id: targetId,
    target_kind: result.target_kind,
    character_count: [...text].length,
    replace,
    snapshot_required: true,
  };
}

async function selectInConnectedTab(argumentsValue, signal) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const targetId = parseTargetId(argumentsValue?.target_id);
  const optionLabel = String(argumentsValue?.option_label || '').replace(/\s+/g, ' ').trim();
  if (!optionLabel
    || optionLabel.length > 240
    || /[\u0000-\u001f\u007f]/.test(optionLabel)) {
    throw new Error('A visible option label of at most 240 characters is required.');
  }
  throwIfAborted(signal);
  const { result } = await runTargetAction(tabId, targetId, 'select_option', {
    option_label: optionLabel,
  });
  await clearTargetState(tabId);
  throwIfAborted(signal);
  return {
    selected: true,
    tab_id: `ct${tabId}`,
    target_id: targetId,
    target_kind: result.target_kind,
    option_label: result.option_label,
    option_index: result.option_index,
    snapshot_required: true,
  };
}

async function scrollConnectedTab(argumentsValue, signal) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const deltaX = Number(argumentsValue?.delta_x || 0);
  const deltaY = Number(argumentsValue?.delta_y || 0);
  if (!Number.isInteger(deltaX)
    || !Number.isInteger(deltaY)
    || Math.abs(deltaX) > 2_000
    || Math.abs(deltaY) > 2_000
    || (deltaX === 0 && deltaY === 0)) {
    throw new Error('Chrome scroll deltas must be non-zero integers between -2000 and 2000.');
  }
  await authorizedClaimedTab(tabId);
  throwIfAborted(signal);
  const results = await chrome.scripting.executeScript({
    target: { tabId },
    world: 'ISOLATED',
    func: scrollPageBy,
    args: [deltaX, deltaY],
  });
  const result = results?.[0]?.result;
  if (!result || result.ok !== true) {
    throw new Error('The connected Chrome tab could not be scrolled.');
  }
  await clearTargetState(tabId);
  throwIfAborted(signal);
  return {
    scrolled: true,
    tab_id: `ct${tabId}`,
    delta_x: deltaX,
    delta_y: deltaY,
    scroll_x: result.scroll_x,
    scroll_y: result.scroll_y,
    viewport_width: result.viewport_width,
    viewport_height: result.viewport_height,
    snapshot_required: true,
  };
}

function scrollPageBy(deltaX, deltaY) {
  window.scrollBy({ left: deltaX, top: deltaY, behavior: 'auto' });
  return {
    ok: true,
    scroll_x: Math.max(0, Math.round(window.scrollX)),
    scroll_y: Math.max(0, Math.round(window.scrollY)),
    viewport_width: Math.max(0, Math.round(window.innerWidth)),
    viewport_height: Math.max(0, Math.round(window.innerHeight)),
  };
}

async function historyConnectedTab(argumentsValue, signal) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const direction = argumentsValue?.direction;
  if (!['back', 'forward'].includes(direction)) {
    throw new Error('Chrome history direction must be back or forward.');
  }
  await authorizedClaimedTab(tabId);
  throwIfAborted(signal);
  await clearTargetState(tabId);
  await runHistoryNavigation(tabId, direction, signal, 10_000);
  const updated = await authorizedClaimedTab(tabId);
  return {
    moved: true,
    direction,
    tab: publicTab(updated),
    snapshot_required: true,
  };
}

async function runHistoryNavigation(tabId, direction, signal, timeoutMs) {
  await new Promise((resolve, reject) => {
    let settled = false;
    let triggered = false;
    let triggerCompleted = false;
    let navigationObserved = false;
    const cleanup = () => {
      chrome.tabs.onUpdated.removeListener(onUpdated);
      signal?.removeEventListener('abort', onAbort);
      clearTimeout(timer);
    };
    const finish = (callback) => {
      if (settled) return;
      settled = true;
      cleanup();
      callback();
    };
    const maybeResolve = () => {
      if (triggerCompleted && navigationObserved) {
        finish(resolve);
      }
    };
    const onUpdated = (updatedTabId, changeInfo) => {
      if (triggered
        && updatedTabId === tabId
        && (changeInfo.status === 'complete' || typeof changeInfo.url === 'string')) {
        navigationObserved = true;
        maybeResolve();
      }
    };
    const onAbort = () => finish(() => reject(new Error('The Chrome action was cancelled.')));
    const timer = setTimeout(
      () => finish(() => reject(new Error('Chrome history navigation did not finish within 10 seconds.'))),
      timeoutMs,
    );
    chrome.tabs.onUpdated.addListener(onUpdated);
    signal?.addEventListener('abort', onAbort, { once: true });
    void (async () => {
      try {
        triggered = true;
        if (direction === 'back') {
          await chrome.tabs.goBack(tabId);
        } else {
          await chrome.tabs.goForward(tabId);
        }
        triggerCompleted = true;
        maybeResolve();
      } catch (error) {
        finish(() => reject(error));
      }
    })();
  });
}

async function activateConnectedTab(argumentsValue, signal) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const tab = await authorizedClaimedTab(tabId);
  throwIfAborted(signal);
  const activated = await chrome.tabs.update(tabId, { active: true });
  throwIfAborted(signal);
  const [activeTab] = await chrome.tabs.query({ active: true, windowId: tab.windowId });
  if (activeTab?.id !== tabId || activated.id !== tabId) {
    throw new Error('Chrome did not activate the requested connected tab.');
  }
  return {
    activated: true,
    tab: publicTab(activeTab),
    window_focused: false,
  };
}

async function screenshotConnectedTab(argumentsValue, signal) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const requestedQuality = Number(argumentsValue?.quality ?? 65);
  const quality = Number.isInteger(requestedQuality)
    ? Math.min(Math.max(requestedQuality, 40), 85)
    : 65;
  const tab = await authorizedClaimedTab(tabId);
  if (!tab.active) {
    throw new Error('The connected Chrome tab must be active before it can be captured.');
  }
  throwIfAborted(signal);
  const dataUrl = await chrome.tabs.captureVisibleTab(tab.windowId, {
    format: 'jpeg',
    quality,
  });
  throwIfAborted(signal);
  const [activeTab] = await chrome.tabs.query({ active: true, windowId: tab.windowId });
  if (activeTab?.id !== tabId) {
    throw new Error('The active Chrome tab changed during screenshot capture.');
  }
  if (typeof dataUrl !== 'string' || !dataUrl.startsWith('data:image/jpeg;base64,')) {
    throw new Error('Chrome did not return a JPEG screenshot.');
  }
  const sizeBytes = base64ByteLength(dataUrl.slice('data:image/jpeg;base64,'.length));
  if (sizeBytes <= 0 || sizeBytes > MAX_SCREENSHOT_BYTES) {
    throw new Error(`Chrome screenshot exceeds the ${MAX_SCREENSHOT_BYTES}-byte safety limit.`);
  }
  return {
    tab: publicTab(activeTab),
    mime_type: 'image/jpeg',
    quality,
    size_bytes: sizeBytes,
    data_url: dataUrl,
    captured_at: new Date().toISOString(),
  };
}

function base64ByteLength(value) {
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(value)) return -1;
  const padding = value.endsWith('==') ? 2 : value.endsWith('=') ? 1 : 0;
  return Math.floor((value.length * 3) / 4) - padding;
}

async function uploadBegin(argumentsValue, signal) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const targetId = parseTargetId(argumentsValue?.target_id);
  const uploadId = parseUploadId(argumentsValue?.upload_id);
  const filename = String(argumentsValue?.filename || '');
  const sizeBytes = Number(argumentsValue?.size_bytes);
  const chunkCount = Number(argumentsValue?.chunk_count);
  const sha256 = String(argumentsValue?.sha256 || '');
  const mimeType = String(argumentsValue?.mime_type || 'application/octet-stream');
  if (!filename
    || filename.length > 255
    || /[\/\\\u0000-\u001f\u007f]/.test(filename)
    || !Number.isInteger(sizeBytes)
    || sizeBytes <= 0
    || sizeBytes > MAX_UPLOAD_BYTES
    || !Number.isInteger(chunkCount)
    || chunkCount <= 0
    || chunkCount > MAX_UPLOAD_CHUNKS
    || !/^[0-9a-f]{64}$/.test(sha256)
    || !/^[\x20-\x7e]{1,120}$/.test(mimeType)) {
    throw new Error('Chrome upload metadata is invalid or exceeds the safety limits.');
  }
  throwIfAborted(signal);
  await runTargetAction(tabId, targetId, 'upload_begin', {
    upload_id: uploadId,
    filename,
    size_bytes: sizeBytes,
    chunk_count: chunkCount,
    sha256,
    mime_type: mimeType,
  });
  return { upload_id: uploadId, ready: true };
}

async function uploadChunk(argumentsValue, signal) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const targetId = parseTargetId(argumentsValue?.target_id);
  const uploadId = parseUploadId(argumentsValue?.upload_id);
  const chunkIndex = Number(argumentsValue?.chunk_index);
  const dataBase64 = String(argumentsValue?.data_base64 || '');
  const chunkBytes = base64ByteLength(dataBase64);
  if (!Number.isInteger(chunkIndex)
    || chunkIndex < 0
    || chunkIndex >= MAX_UPLOAD_CHUNKS
    || chunkBytes <= 0
    || chunkBytes > MAX_UPLOAD_CHUNK_BYTES) {
    throw new Error('Chrome upload chunk is invalid or exceeds the safety limits.');
  }
  throwIfAborted(signal);
  const { result } = await runTargetAction(tabId, targetId, 'upload_chunk', {
    upload_id: uploadId,
    chunk_index: chunkIndex,
    data_base64: dataBase64,
  });
  return {
    upload_id: uploadId,
    accepted_chunk_index: result.accepted_chunk_index,
    received_bytes: result.received_bytes,
  };
}

async function uploadFinish(argumentsValue, signal) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const targetId = parseTargetId(argumentsValue?.target_id);
  const uploadId = parseUploadId(argumentsValue?.upload_id);
  throwIfAborted(signal);
  const { result } = await runTargetAction(tabId, targetId, 'upload_finish', {
    upload_id: uploadId,
  });
  await clearTargetState(tabId);
  throwIfAborted(signal);
  return {
    uploaded: true,
    upload_id: uploadId,
    tab_id: `ct${tabId}`,
    target_id: targetId,
    filename: result.filename,
    size_bytes: result.size_bytes,
    sha256: result.sha256,
    snapshot_required: true,
  };
}

async function uploadAbort(argumentsValue) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const uploadId = parseUploadId(argumentsValue?.upload_id);
  await authorizedClaimedTab(tabId);
  try {
    const results = await chrome.scripting.executeScript({
      target: { tabId },
      world: 'ISOLATED',
      func: runBoundTargetAction,
      args: [
        '',
        '',
        TARGET_ATTRIBUTE,
        'upload_abort',
        { upload_id: uploadId },
        MAX_DOWNLOAD_DATA_URL_CHARS,
      ],
    });
    return { upload_id: uploadId, aborted: Boolean(results?.[0]?.result?.aborted) };
  } catch {
    return { upload_id: uploadId, aborted: false };
  }
}

async function downloadBegin(argumentsValue, signal) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const targetId = parseTargetId(argumentsValue?.target_id);
  const downloadId = parseUploadId(argumentsValue?.download_id);
  const maxBytes = Number(argumentsValue?.max_bytes);
  if (!Number.isInteger(maxBytes) || maxBytes <= 0 || maxBytes > MAX_DOWNLOAD_BYTES) {
    throw new Error(`Chrome download max_bytes must be between 1 and ${MAX_DOWNLOAD_BYTES}.`);
  }
  const onAbort = () => {
    void abortDownloadInPage(tabId, downloadId);
  };
  signal?.addEventListener('abort', onAbort, { once: true });
  try {
    throwIfAborted(signal);
    const { result } = await runTargetAction(tabId, targetId, 'download_begin', {
      download_id: downloadId,
      max_bytes: maxBytes,
      chunk_bytes: DOWNLOAD_CHUNK_BYTES,
      max_chunks: MAX_DOWNLOAD_CHUNKS,
    });
    throwIfAborted(signal);
    return {
      download_id: downloadId,
      ready: true,
      size_bytes: result.size_bytes,
      sha256: result.sha256,
      chunk_count: result.chunk_count,
      source_kind: result.source_kind,
      source_url: result.source_url,
      mime_type: result.mime_type,
    };
  } finally {
    signal?.removeEventListener('abort', onAbort);
  }
}

async function downloadChunk(argumentsValue, signal) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const targetId = parseTargetId(argumentsValue?.target_id);
  const downloadId = parseUploadId(argumentsValue?.download_id);
  const chunkIndex = Number(argumentsValue?.chunk_index);
  if (!Number.isInteger(chunkIndex) || chunkIndex < 0 || chunkIndex >= MAX_DOWNLOAD_CHUNKS) {
    throw new Error('Chrome download chunk index is invalid.');
  }
  throwIfAborted(signal);
  const { result } = await runTargetAction(tabId, targetId, 'download_chunk', {
    download_id: downloadId,
    chunk_index: chunkIndex,
    chunk_bytes: DOWNLOAD_CHUNK_BYTES,
  });
  throwIfAborted(signal);
  const dataBase64 = String(result.data_base64 || '');
  const sizeBytes = base64ByteLength(dataBase64);
  if (sizeBytes <= 0 || sizeBytes > DOWNLOAD_CHUNK_BYTES) {
    throw new Error('Chrome download chunk exceeded the safety limit.');
  }
  return {
    download_id: downloadId,
    chunk_index: chunkIndex,
    size_bytes: sizeBytes,
    data_base64: dataBase64,
  };
}

async function downloadFinish(argumentsValue) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const targetId = parseTargetId(argumentsValue?.target_id);
  const downloadId = parseUploadId(argumentsValue?.download_id);
  const { result } = await runTargetAction(tabId, targetId, 'download_finish', {
    download_id: downloadId,
  });
  return { download_id: downloadId, released: Boolean(result.released) };
}

async function abortDownloadInPage(tabId, downloadId) {
  try {
    const results = await chrome.scripting.executeScript({
      target: { tabId },
      world: 'ISOLATED',
      func: runBoundTargetAction,
      args: [
        '',
        '',
        TARGET_ATTRIBUTE,
        'download_abort',
        { download_id: downloadId },
        MAX_DOWNLOAD_DATA_URL_CHARS,
      ],
    });
    return Boolean(results?.[0]?.result?.aborted);
  } catch {
    return false;
  }
}

async function downloadAbort(argumentsValue) {
  const tabId = parseStableTabId(argumentsValue?.tab_id);
  const downloadId = parseUploadId(argumentsValue?.download_id);
  await authorizedClaimedTab(tabId);
  return { download_id: downloadId, aborted: await abortDownloadInPage(tabId, downloadId) };
}

async function runBoundTargetAction(
  targetId,
  expectedFingerprint,
  targetAttribute,
  action,
  payload,
  maxDownloadDataUrlChars,
) {
  const clean = (value, limit = 240) => String(value || '')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, limit);
  const accessibleName = (element) => {
    const aria = clean(element.getAttribute('aria-label'));
    if (aria) return aria;
    const labelledBy = element.getAttribute('aria-labelledby');
    if (labelledBy) {
      const label = labelledBy
        .split(/\s+/)
        .map((id) => clean(document.getElementById(id)?.textContent))
        .filter(Boolean)
        .join(' ');
      if (label) return clean(label);
    }
    if (element.id) {
      const explicit = document.querySelector(`label[for="${CSS.escape(element.id)}"]`);
      const label = clean(explicit?.textContent);
      if (label) return label;
    }
    return clean(element.getAttribute('alt') || element.getAttribute('title') || element.textContent);
  };
  const fnv1a = (value) => {
    let hash = 0x811c9dc5;
    for (let index = 0; index < value.length; index += 1) {
      hash ^= value.charCodeAt(index);
      hash = Math.imul(hash, 0x01000193) >>> 0;
    }
    return hash.toString(16).padStart(8, '0');
  };
  const domPath = (element) => {
    const parts = [];
    let current = element;
    while (current && current.nodeType === Node.ELEMENT_NODE && parts.length < 24) {
      let index = 1;
      let sibling = current.previousElementSibling;
      while (sibling) {
        if (sibling.tagName === current.tagName) index += 1;
        sibling = sibling.previousElementSibling;
      }
      parts.push(`${current.tagName.toLowerCase()}:${index}`);
      current = current.parentElement;
    }
    return parts.reverse().join('/');
  };
  const boundedAnchorHref = (element) => {
    if (element.tagName.toLowerCase() !== 'a') return '';
    const href = String(element.href || '');
    const limit = href.startsWith('data:') ? maxDownloadDataUrlChars : 8192;
    return Number.isInteger(limit) && href.length <= limit ? href : null;
  };
  const behaviorSignature = (element) => [
    boundedAnchorHref(element) || '',
    clean(element.getAttribute('formaction'), 2_048),
    clean(element.getAttribute('name'), 240),
    element.hasAttribute('multiple') ? 'multiple' : '',
  ].join('|');
  const fingerprint = (element) => {
    const inputType = clean(element.getAttribute('type'), 40).toLowerCase();
    const secure = inputType === 'password'
      || /password|secret/i.test(clean(element.getAttribute('autocomplete'), 80));
    const name = secure ? '[secure field]' : accessibleName(element);
    return fnv1a([
      domPath(element),
      element.tagName.toLowerCase(),
      inputType,
      clean(element.getAttribute('role'), 40),
      clean(name, 240),
      behaviorSignature(element),
    ].join('|'));
  };
  const visible = (element) => {
    const style = window.getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.visibility !== 'hidden'
      && style.display !== 'none'
      && Number(style.opacity || 1) !== 0
      && rect.width > 0
      && rect.height > 0;
  };
  const uploadStore = () => {
    if (!globalThis.__chatosChromeUploads) {
      globalThis.__chatosChromeUploads = new Map();
    }
    return globalThis.__chatosChromeUploads;
  };
  const downloadStore = () => {
    if (!globalThis.__chatosChromeDownloads) {
      globalThis.__chatosChromeDownloads = new Map();
    }
    return globalThis.__chatosChromeDownloads;
  };
  const bytesToBase64 = (bytes) => {
    let binary = '';
    for (let offset = 0; offset < bytes.length; offset += 0x8000) {
      binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
    }
    return btoa(binary);
  };
  const uploadId = payload?.upload_id;
  const downloadId = payload?.download_id;
  if (action === 'upload_abort') {
    const aborted = uploadStore().delete(uploadId);
    return { ok: true, aborted };
  }
  if (action === 'download_abort') {
    const download = downloadStore().get(downloadId);
    download?.controller?.abort();
    const aborted = downloadStore().delete(downloadId);
    return { ok: true, aborted };
  }
  const matches = [...document.querySelectorAll(`[${targetAttribute}="${targetId}"]`)];
  if (matches.length !== 1) {
    return { ok: false, error: 'The Chrome target no longer exists uniquely.' };
  }
  const element = matches[0];
  if (!visible(element) || fingerprint(element) !== expectedFingerprint) {
    return { ok: false, error: 'The Chrome target changed. Capture a fresh snapshot.' };
  }
  const tag = element.tagName.toLowerCase();
  const type = clean(element.getAttribute('type'), 40).toLowerCase();
  const role = clean(element.getAttribute('role') || tag, 40);
  const targetKind = type ? `${tag}:${type}` : role;
  if (action === 'click') {
    if (element.matches(':disabled') || element.getAttribute('aria-disabled') === 'true') {
      return { ok: false, error: 'The Chrome target is disabled.' };
    }
    element.scrollIntoView({ block: 'center', inline: 'center', behavior: 'auto' });
    element.focus({ preventScroll: true });
    element.click();
    return { ok: true, target_kind: targetKind };
  }
  if (action === 'type_text') {
    const secure = type === 'password'
      || /password|secret/i.test(clean(element.getAttribute('autocomplete'), 80));
    const editable = tag === 'textarea'
      || (tag === 'input' && !['button', 'checkbox', 'color', 'file', 'hidden', 'image', 'radio', 'range', 'reset', 'submit'].includes(type))
      || element.isContentEditable
      || ['textbox', 'combobox'].includes(element.getAttribute('role'));
    if (secure || !editable || element.matches(':disabled') || element.getAttribute('aria-readonly') === 'true') {
      return { ok: false, error: 'The Chrome target is not a safe editable text control.' };
    }
    const text = String(payload?.text || '');
    element.focus({ preventScroll: false });
    if (tag === 'input' || tag === 'textarea') {
      const prototype = tag === 'input' ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype;
      const setter = Object.getOwnPropertyDescriptor(prototype, 'value')?.set;
      const nextValue = payload?.replace === false ? `${element.value || ''}${text}` : text;
      if (!setter) return { ok: false, error: 'The Chrome text control cannot be updated safely.' };
      setter.call(element, nextValue);
    } else {
      if (payload?.replace !== false) element.textContent = '';
      const selection = window.getSelection();
      const range = document.createRange();
      range.selectNodeContents(element);
      range.collapse(false);
      selection.removeAllRanges();
      selection.addRange(range);
      if (!document.execCommand('insertText', false, text)) {
        element.append(document.createTextNode(text));
      }
    }
    element.dispatchEvent(new InputEvent('input', {
      bubbles: true,
      inputType: 'insertText',
      data: text,
    }));
    element.dispatchEvent(new Event('change', { bubbles: true }));
    return { ok: true, target_kind: targetKind };
  }
  if (action === 'select_option') {
    if (tag !== 'select'
      || element.multiple
      || element.matches(':disabled')
      || element.getAttribute('aria-readonly') === 'true') {
      return { ok: false, error: 'The Chrome target is not a safe single-select control.' };
    }
    const optionLabel = clean(payload?.option_label, 240);
    const matches = [...element.options].filter((option) => clean(option.textContent, 240) === optionLabel);
    if (matches.length !== 1 || matches[0].disabled) {
      return { ok: false, error: 'The Chrome option label is missing, duplicated, or disabled.' };
    }
    const option = matches[0];
    element.selectedIndex = option.index;
    element.dispatchEvent(new Event('input', { bubbles: true }));
    element.dispatchEvent(new Event('change', { bubbles: true }));
    return {
      ok: true,
      target_kind: targetKind,
      option_label: optionLabel,
      option_index: option.index,
    };
  }
  if (action === 'download_begin') {
    if (tag !== 'a' || !element.hasAttribute('href')) {
      return { ok: false, error: 'The Chrome target is not a direct download link.' };
    }
    const rawSourceUrl = String(element.href || '');
    const sourceUrlLimit = rawSourceUrl.startsWith('data:') ? maxDownloadDataUrlChars : 8192;
    if (!Number.isInteger(sourceUrlLimit)
      || sourceUrlLimit <= 0
      || rawSourceUrl.length === 0
      || rawSourceUrl.length > sourceUrlLimit) {
      return { ok: false, error: 'The Chrome download link URL exceeds the safety limit.' };
    }
    let sourceUrl;
    try {
      sourceUrl = new URL(rawSourceUrl, location.href);
    } catch {
      return { ok: false, error: 'The Chrome download link URL is invalid.' };
    }
    const sourceKind = sourceUrl.protocol.replace(/:$/, '');
    if (!['http', 'https', 'blob', 'data'].includes(sourceKind)
      || sourceUrl.username
      || sourceUrl.password
      || (['http', 'https', 'blob'].includes(sourceKind) && sourceUrl.origin !== location.origin)) {
      return { ok: false, error: 'Chrome downloads are limited to same-origin HTTP(S)/blob links or bounded data links.' };
    }
    const maxBytes = payload?.max_bytes;
    const chunkBytes = payload?.chunk_bytes;
    const maxChunks = payload?.max_chunks;
    if (!Number.isInteger(maxBytes)
      || maxBytes <= 0
      || maxBytes > 10 * 1024 * 1024
      || chunkBytes !== 192 * 1024
      || maxChunks !== 64) {
      return { ok: false, error: 'The Chrome download bounds are invalid.' };
    }
    const downloads = downloadStore();
    if (downloads.size >= 4 || downloads.has(downloadId)) {
      return { ok: false, error: 'Chrome download capacity is exhausted.' };
    }
    const controller = new AbortController();
    downloads.set(downloadId, { controller, pending: true });
    try {
      const response = await fetch(sourceUrl.toString(), {
        method: 'GET',
        credentials: 'include',
        redirect: 'follow',
        cache: 'no-store',
        signal: controller.signal,
      });
      if (!response.ok) {
        throw new Error(`The download link returned HTTP ${response.status}.`);
      }
      const finalUrl = new URL(response.url || sourceUrl.toString(), location.href);
      const finalKind = finalUrl.protocol.replace(/:$/, '');
      if (['http', 'https', 'blob'].includes(finalKind) && finalUrl.origin !== location.origin) {
        throw new Error('The download redirected outside the authorized origin.');
      }
      const declaredLength = Number(response.headers.get('content-length'));
      if (Number.isFinite(declaredLength) && declaredLength > maxBytes) {
        throw new Error('The download exceeds the approved byte limit.');
      }
      const reader = response.body?.getReader();
      if (!reader) {
        throw new Error('The download response body is unavailable.');
      }
      const chunks = [];
      let totalBytes = 0;
      while (true) {
        const next = await reader.read();
        if (next.done) break;
        if (!(next.value instanceof Uint8Array)) {
          throw new Error('The download returned an unsupported body chunk.');
        }
        totalBytes += next.value.length;
        if (totalBytes > maxBytes) {
          controller.abort();
          throw new Error('The download exceeds the approved byte limit.');
        }
        chunks.push(next.value);
      }
      if (totalBytes <= 0) {
        throw new Error('The download is empty.');
      }
      const bytes = new Uint8Array(totalBytes);
      let offset = 0;
      for (const chunk of chunks) {
        bytes.set(chunk, offset);
        offset += chunk.length;
      }
      const chunkCount = Math.ceil(bytes.length / chunkBytes);
      if (chunkCount <= 0 || chunkCount > maxChunks) {
        throw new Error('The download requires too many Native Messaging chunks.');
      }
      const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
      const sha256 = [...digest].map((value) => value.toString(16).padStart(2, '0')).join('');
      const rawMime = String(response.headers.get('content-type') || 'application/octet-stream')
        .split(';', 1)[0]
        .trim();
      const mimeType = /^[\x20-\x7e]{1,120}$/.test(rawMime)
        ? rawMime
        : 'application/octet-stream';
      downloads.set(downloadId, {
        controller,
        bytes,
        sha256,
        chunk_count: chunkCount,
        source_kind: sourceKind,
        source_url: ['http', 'https'].includes(finalKind) ? finalUrl.toString().slice(0, 8192) : null,
        mime_type: mimeType,
      });
      return {
        ok: true,
        size_bytes: bytes.length,
        sha256,
        chunk_count: chunkCount,
        source_kind: sourceKind,
        source_url: ['http', 'https'].includes(finalKind) ? finalUrl.toString().slice(0, 8192) : null,
        mime_type: mimeType,
        target_kind: targetKind,
      };
    } catch (error) {
      downloads.delete(downloadId);
      const message = error instanceof Error ? error.message : 'The Chrome download failed.';
      return { ok: false, error: clean(message, 300) || 'The Chrome download failed.' };
    }
  }
  if (action === 'download_chunk') {
    const download = downloadStore().get(downloadId);
    const index = payload?.chunk_index;
    const chunkBytes = payload?.chunk_bytes;
    if (!download
      || !(download.bytes instanceof Uint8Array)
      || !Number.isInteger(index)
      || index < 0
      || index >= download.chunk_count
      || chunkBytes !== 192 * 1024) {
      return { ok: false, error: 'The Chrome download chunk request is invalid or stale.' };
    }
    const start = index * chunkBytes;
    const chunk = download.bytes.subarray(start, Math.min(download.bytes.length, start + chunkBytes));
    return {
      ok: true,
      chunk_index: index,
      size_bytes: chunk.length,
      data_base64: bytesToBase64(chunk),
    };
  }
  if (action === 'download_finish') {
    const released = downloadStore().delete(downloadId);
    return { ok: true, released };
  }
  if (action === 'upload_begin') {
    if (tag !== 'input' || type !== 'file' || element.matches(':disabled')) {
      return { ok: false, error: 'The Chrome target is not an enabled file input.' };
    }
    const uploads = uploadStore();
    if (uploads.size >= 8 || uploads.has(uploadId)) {
      return { ok: false, error: 'Chrome upload capacity is exhausted.' };
    }
    uploads.set(uploadId, {
      target_id: targetId,
      fingerprint: expectedFingerprint,
      filename: payload.filename,
      size_bytes: payload.size_bytes,
      chunk_count: payload.chunk_count,
      sha256: payload.sha256,
      mime_type: payload.mime_type,
      chunks: new Array(payload.chunk_count),
      received_bytes: 0,
    });
    return { ok: true, target_kind: targetKind };
  }
  const upload = uploadStore().get(uploadId);
  if (!upload || upload.target_id !== targetId || upload.fingerprint !== expectedFingerprint) {
    return { ok: false, error: 'The Chrome upload session is missing or stale.' };
  }
  if (action === 'upload_chunk') {
    const index = payload.chunk_index;
    if (!Number.isInteger(index) || index < 0 || index >= upload.chunk_count || upload.chunks[index]) {
      return { ok: false, error: 'The Chrome upload chunk index is invalid or duplicated.' };
    }
    let binary;
    try {
      binary = atob(payload.data_base64);
    } catch {
      return { ok: false, error: 'The Chrome upload chunk is not valid base64.' };
    }
    const bytes = new Uint8Array(binary.length);
    for (let indexByte = 0; indexByte < binary.length; indexByte += 1) {
      bytes[indexByte] = binary.charCodeAt(indexByte);
    }
    if (bytes.length <= 0
      || bytes.length > 192 * 1024
      || upload.received_bytes + bytes.length > upload.size_bytes) {
      return { ok: false, error: 'The Chrome upload chunk exceeds the declared bounds.' };
    }
    upload.chunks[index] = bytes;
    upload.received_bytes += bytes.length;
    return {
      ok: true,
      accepted_chunk_index: index,
      received_bytes: upload.received_bytes,
    };
  }
  if (action === 'upload_finish') {
    if (tag !== 'input' || type !== 'file' || upload.chunks.some((chunk) => !chunk)) {
      return { ok: false, error: 'The Chrome upload is incomplete or the target changed.' };
    }
    const bytes = new Uint8Array(upload.size_bytes);
    let offset = 0;
    for (const chunk of upload.chunks) {
      bytes.set(chunk, offset);
      offset += chunk.length;
    }
    if (offset !== upload.size_bytes) {
      return { ok: false, error: 'The Chrome upload size does not match the declared file.' };
    }
    const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
    const sha256 = [...digest].map((value) => value.toString(16).padStart(2, '0')).join('');
    if (sha256 !== upload.sha256) {
      uploadStore().delete(uploadId);
      return { ok: false, error: 'The Chrome upload hash verification failed.' };
    }
    const file = new File([bytes], upload.filename, { type: upload.mime_type });
    const transfer = new DataTransfer();
    transfer.items.add(file);
    element.files = transfer.files;
    element.dispatchEvent(new Event('input', { bubbles: true }));
    element.dispatchEvent(new Event('change', { bubbles: true }));
    uploadStore().delete(uploadId);
    return {
      ok: true,
      filename: upload.filename,
      size_bytes: upload.size_bytes,
      sha256,
      target_kind: targetKind,
    };
  }
  return { ok: false, error: 'The Chrome target action is unsupported.' };
}

function randomHex(bytes) {
  const values = new Uint8Array(bytes);
  crypto.getRandomValues(values);
  return [...values].map((value) => value.toString(16).padStart(2, '0')).join('');
}

function buildBoundedPageSnapshot(
  maxChars,
  maxItems,
  snapshotId,
  targetAttribute,
  maxDownloadDataUrlChars,
) {
  const output = [];
  const targets = [];
  let length = 0;
  let itemCount = 0;
  let truncated = false;

  const append = (line) => {
    if (truncated || itemCount >= maxItems) {
      truncated = true;
      return;
    }
    const normalized = String(line || '').replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g, ' ').trim();
    if (!normalized) {
      return;
    }
    const next = `${normalized}\n`;
    if (length + next.length > maxChars) {
      const remaining = Math.max(0, maxChars - length);
      if (remaining > 0) {
        output.push(next.slice(0, remaining));
      }
      truncated = true;
      length = maxChars;
      return;
    }
    output.push(next);
    length += next.length;
    itemCount += 1;
  };

  const visible = (element) => {
    const style = window.getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.visibility !== 'hidden'
      && style.display !== 'none'
      && Number(style.opacity || 1) !== 0
      && rect.width > 0
      && rect.height > 0;
  };

  const clean = (value, limit = 240) => String(value || '')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, limit);

  const accessibleName = (element) => {
    const aria = clean(element.getAttribute('aria-label'));
    if (aria) return aria;
    const labelledBy = element.getAttribute('aria-labelledby');
    if (labelledBy) {
      const label = labelledBy
        .split(/\s+/)
        .map((id) => clean(document.getElementById(id)?.textContent))
        .filter(Boolean)
        .join(' ');
      if (label) return clean(label);
    }
    if (element.id) {
      const explicit = document.querySelector(`label[for="${CSS.escape(element.id)}"]`);
      const label = clean(explicit?.textContent);
      if (label) return label;
    }
    return clean(
      element.getAttribute('alt')
      || element.getAttribute('title')
      || element.textContent,
    );
  };

  const fnv1a = (value) => {
    let hash = 0x811c9dc5;
    for (let index = 0; index < value.length; index += 1) {
      hash ^= value.charCodeAt(index);
      hash = Math.imul(hash, 0x01000193) >>> 0;
    }
    return hash.toString(16).padStart(8, '0');
  };

  const domPath = (element) => {
    const parts = [];
    let current = element;
    while (current && current.nodeType === Node.ELEMENT_NODE && parts.length < 24) {
      let index = 1;
      let sibling = current.previousElementSibling;
      while (sibling) {
        if (sibling.tagName === current.tagName) index += 1;
        sibling = sibling.previousElementSibling;
      }
      parts.push(`${current.tagName.toLowerCase()}:${index}`);
      current = current.parentElement;
    }
    return parts.reverse().join('/');
  };

  const boundedAnchorHref = (element) => {
    if (element.tagName.toLowerCase() !== 'a') return '';
    const href = String(element.href || '');
    const limit = href.startsWith('data:') ? maxDownloadDataUrlChars : 8192;
    return Number.isInteger(limit) && href.length <= limit ? href : null;
  };

  const behaviorSignature = (element) => [
    boundedAnchorHref(element) || '',
    clean(element.getAttribute('formaction'), 2_048),
    clean(element.getAttribute('name'), 240),
    element.hasAttribute('multiple') ? 'multiple' : '',
  ].join('|');

  const fingerprint = (element, name) => fnv1a([
    domPath(element),
    element.tagName.toLowerCase(),
    clean(element.getAttribute('type'), 40),
    clean(element.getAttribute('role'), 40),
    clean(name, 240),
    behaviorSignature(element),
  ].join('|'));

  document.querySelectorAll(`[${targetAttribute}]`).forEach((element) => {
    element.removeAttribute(targetAttribute);
  });

  append(`TITLE ${clean(document.title, 512)}`);
  append(`URL ${location.origin}${location.pathname}`);

  const selector = [
    'a[href]', 'button', 'input:not([type="hidden"])', 'select', 'textarea',
    '[role]', '[contenteditable="true"]', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
  ].join(',');
  const actionableSelector = [
    'a[href]', 'button', 'input:not([type="hidden"])', 'select', 'textarea',
    '[contenteditable="true"]', '[role="button"]', '[role="link"]',
    '[role="checkbox"]', '[role="radio"]', '[role="tab"]', '[role="menuitem"]',
    '[role="option"]', '[role="switch"]', '[role="combobox"]', '[role="textbox"]',
  ].join(',');
  let scannedElements = 0;
  for (const element of document.querySelectorAll(selector)) {
    scannedElements += 1;
    if (scannedElements > 2_000) {
      truncated = true;
      break;
    }
    if (truncated || !visible(element)) continue;
    const tag = element.tagName.toLowerCase();
    const role = clean(element.getAttribute('role') || tag, 80);
    const inputType = tag === 'input' ? clean(element.getAttribute('type') || 'text', 40) : '';
    const secure = inputType === 'password'
      || /password|secret/i.test(clean(element.getAttribute('autocomplete'), 80));
    const name = secure ? '[secure field]' : accessibleName(element);
    const state = [
      element.hasAttribute('disabled') ? 'disabled' : '',
      element.getAttribute('aria-expanded') === 'true' ? 'expanded' : '',
      element.getAttribute('aria-checked') === 'true' ? 'checked' : '',
    ].filter(Boolean).join(' ');
    const selectOptions = tag === 'select'
      ? [...element.options]
        .filter((option) => !option.disabled)
        .slice(0, 20)
        .map((option) => clean(option.textContent, 120))
        .filter(Boolean)
      : [];
    const selectedOption = tag === 'select'
      ? clean(element.selectedOptions?.[0]?.textContent, 120)
      : '';
    let targetId = '';
    if (element.matches(actionableSelector)
      && boundedAnchorHref(element) !== null
      && targets.length < maxItems) {
      targetId = `cr${snapshotId}-${targets.length + 1}`;
      element.setAttribute(targetAttribute, targetId);
      targets.push({
        target_id: targetId,
        fingerprint: fingerprint(element, name),
        kind: inputType ? `${tag}:${inputType}` : role,
      });
    }
    append(`${targetId ? `@${targetId} ` : ''}${role}${inputType ? ` type=${inputType}` : ''}${name ? ` name="${name}"` : ''}${state ? ` ${state}` : ''}${selectedOption ? ` selected=${JSON.stringify(selectedOption)}` : ''}${selectOptions.length ? ` options=${JSON.stringify(selectOptions)}` : ''}`);
  }

  const walker = document.createTreeWalker(document.body || document.documentElement, NodeFilter.SHOW_TEXT);
  let scannedTextNodes = 0;
  while (!truncated) {
    const node = walker.nextNode();
    if (!node) break;
    scannedTextNodes += 1;
    if (scannedTextNodes > 5_000) {
      truncated = true;
      break;
    }
    const parent = node.parentElement;
    if (!parent || ['SCRIPT', 'STYLE', 'NOSCRIPT', 'TEMPLATE'].includes(parent.tagName) || !visible(parent)) {
      continue;
    }
    if (parent.closest('input[type="password"], [aria-hidden="true"]')) {
      continue;
    }
    const text = clean(node.nodeValue, 500);
    if (text.length >= 2) {
      append(`TEXT ${text}`);
    }
  }

  return { snapshot: output.join('').slice(0, maxChars), truncated, targets };
}

function safeError(error) {
  const message = error instanceof Error ? error.message : String(error || 'Chrome command failed.');
  return message.replace(/[\u0000-\u001f\u007f]/g, ' ').slice(0, 512) || 'Chrome command failed.';
}

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  const run = async () => {
    switch (message?.type) {
      case 'popup_status':
        return popupStatus();
      case 'claim_active_tab':
        return claimActiveTab();
      case 'release_active_tab': {
        const tab = await currentActiveTab();
        if (tab?.id) await releaseTab(tab.id);
        return popupStatus();
      }
      case 'origin_permission_removed':
        if (typeof message.origin === 'string') await releaseOrigin(message.origin);
        return popupStatus();
      default:
        throw new Error('Unsupported extension popup request.');
    }
  };
  void run().then(
    (result) => sendResponse({ ok: true, result }),
    (error) => sendResponse({ ok: false, error: safeError(error) }),
  );
  return true;
});

chrome.tabs.onRemoved.addListener((tabId) => {
  void releaseTab(tabId);
});

chrome.tabs.onUpdated.addListener((tabId, changeInfo) => {
  if (typeof changeInfo.url !== 'string') {
    return;
  }
  void (async () => {
    const claims = await loadClaims();
    const claim = claims[String(tabId)];
    const site = safeOriginAndPattern(changeInfo.url);
    if (claim && (!site || site.origin !== claim.origin || !(await sitePermissionGranted(site.pattern)))) {
      await releaseTab(tabId);
    }
  })();
});

chrome.permissions.onRemoved.addListener((permissions) => {
  for (const pattern of permissions.origins || []) {
    const origin = safeOriginAndPattern(pattern.replace(/\/\*$/, '/'))?.origin;
    if (origin) void releaseOrigin(origin);
  }
});

connectNativeHost();
