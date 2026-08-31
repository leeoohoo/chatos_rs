import {BridgeProtocolError, EXTENSION_PROTOCOL_VERSION} from './bridge.js';

const CDP_PROTOCOL_VERSION = '1.3';
const MAX_SUBSCRIPTION_METHODS = 128;
const MAX_GROUP_TITLE_LENGTH = 80;
const TAB_GROUP_COLORS = ['grey', 'blue', 'red', 'yellow', 'green', 'pink', 'purple', 'cyan', 'orange'];
const ENABLED_EVENT_DOMAINS = new Set([
  'DOM',
  'Log',
  'Network',
  'Page',
  'Performance',
  'Runtime',
  'Security'
]);

export class ExtensionController {
  #chrome;
  #bridge;
  #randomId;
  #targets = new Map();
  #targetByTab = new Map();
  #sessions = new Map();
  #sessionByTab = new Map();
  #subscriptions = new Map();
  #enabledDomains = new Map();
  #ownedTabIds = new Set();
  #groupId = null;
  #sessionName = null;
  #started = false;

  constructor({chromeApi = globalThis.chrome, bridge, randomId = () => crypto.randomUUID()}) {
    this.#chrome = chromeApi;
    this.#bridge = bridge;
    this.#randomId = randomId;
  }

  start() {
    if (this.#started) return;
    this.#started = true;
    this.#chrome.debugger.onEvent.addListener((source, method, params) => {
      this.#onDebuggerEvent(source, method, params ?? {});
    });
    this.#chrome.debugger.onDetach.addListener((source, reason) => {
      this.#onDebuggerDetach(source, reason);
    });
    this.#chrome.tabs.onRemoved.addListener(tabId => {
      void this.#removeTab(tabId, 'tab_closed');
    });
    this.#chrome.tabs.onUpdated.addListener((tabId, _changeInfo, tab) => {
      void this.#updateTab(tabId, tab);
    });
  }

  async handleRequest(method, params = {}) {
    switch (method) {
      case 'extension.getCapabilities':
        const capabilities = [
          'explicit_tab_sharing',
          'page_control',
          'raw_cdp',
          'session_events',
          'tab_create',
          'tab_close'
        ];
        if (this.#supportsNativeTabGroups()) capabilities.push('native_tab_groups');
        return {
          protocol_version: EXTENSION_PROTOCOL_VERSION,
          cdp_protocol_version: CDP_PROTOCOL_VERSION,
          capabilities
        };
      case 'extension.configureSession':
        return this.configureSession(requiredString(params, 'session_name'));
      case 'extension.endSession':
        await this.endSession();
        return {};
      case 'extension.listTargets':
        return {targets: this.targets()};
      case 'extension.createTarget':
        return {target: await this.createTarget(params.url)};
      case 'extension.closeTarget':
        await this.closeTarget(requiredString(params, 'target_id'));
        return {};
      case 'extension.attachTarget':
        return {session_id: await this.attachTarget(requiredString(params, 'target_id'))};
      case 'extension.detachTarget':
        await this.detachSession(requiredString(params, 'session_id'));
        return {};
      case 'extension.cdpSend':
        return {
          result: await this.sendCommand(
            params.session_id ?? null,
            requiredString(params, 'method'),
            params.params ?? {}
          )
        };
      case 'extension.subscribe':
        await this.subscribe(params);
        return {};
      case 'extension.unsubscribe':
        this.unsubscribe(requiredString(params, 'subscription_id'));
        return {};
      default:
        throw new BridgeProtocolError('unsupported_by_backend', `Unsupported extension method: ${method}`);
    }
  }

  targets() {
    return [...this.#targets.values()].map(({descriptor}) => ({...descriptor}));
  }

  status() {
    return {
      targets: this.targets(),
      attachedTargetIds: [...this.#sessions.values()].map(session => session.targetId),
      sessionName: this.#sessionName,
      groupId: this.#groupId,
      ownedTabIds: [...this.#ownedTabIds]
    };
  }

  async configureSession(sessionName) {
    if (!this.#supportsNativeTabGroups()) {
      throw new BridgeProtocolError(
        'unsupported_by_backend',
        'This Chrome version or extension installation does not support native tab groups'
      );
    }
    await this.endSession();
    this.#sessionName = normalizeGroupTitle(sessionName);
    return {session_name: this.#sessionName};
  }

  async shareTab(tabId, {owned = false} = {}) {
    if (!Number.isSafeInteger(tabId)) {
      throw new BridgeProtocolError('invalid_request', 'A valid Chrome tab ID is required');
    }
    const existingTargetId = this.#targetByTab.get(tabId);
    if (existingTargetId) return this.#targets.get(existingTargetId).descriptor;
    const tab = await this.#chrome.tabs.get(tabId);
    ensureControllableUrl(tab.url);
    const targetId = `target_${this.#randomId()}`;
    const descriptor = tabDescriptor(targetId, tab);
    this.#targets.set(targetId, {tabId, descriptor, owned});
    this.#targetByTab.set(tabId, targetId);
    if (owned) this.#ownedTabIds.add(tabId);
    this.#publishTargets();
    return {...descriptor};
  }

  async revokeTarget(targetId) {
    const target = this.#requiredTarget(targetId);
    const sessionId = this.#sessionByTab.get(target.tabId);
    if (sessionId) await this.detachSession(sessionId);
    this.#targets.delete(targetId);
    this.#targetByTab.delete(target.tabId);
    this.#publishTargets();
  }

  async revokeAll() {
    await this.endSession();
    const sessionIds = [...this.#sessions.keys()];
    await Promise.allSettled(sessionIds.map(sessionId => this.detachSession(sessionId)));
    this.#targets.clear();
    this.#targetByTab.clear();
    this.#subscriptions.clear();
    this.#enabledDomains.clear();
  }

  async endSession() {
    const ownedTabIds = [...this.#ownedTabIds];
    for (const tabId of ownedTabIds) {
      const sessionId = this.#sessionByTab.get(tabId);
      if (sessionId) await this.detachSession(sessionId);
      const targetId = this.#targetByTab.get(tabId);
      if (targetId) {
        this.#targets.delete(targetId);
        this.#targetByTab.delete(tabId);
      }
    }
    if (Number.isSafeInteger(this.#groupId)) {
      try {
        await this.#chrome.tabGroups.update(this.#groupId, {collapsed: true});
      } catch {
        // The user may have ungrouped or closed the task tabs already.
      }
    }
    const changed = ownedTabIds.length > 0;
    this.#ownedTabIds.clear();
    this.#groupId = null;
    this.#sessionName = null;
    if (changed) this.#publishTargets('session_ended');
  }

  async createTarget(url = 'about:blank') {
    ensureControllableUrl(url);
    const tab = await this.#chrome.tabs.create({url, active: true});
    if (!Number.isSafeInteger(tab.id)) {
      throw new BridgeProtocolError('backend_error', 'Chrome did not return a tab ID');
    }
    try {
      if (this.#sessionName) await this.#addTabToTaskGroup(tab.id);
      return await this.shareTab(tab.id, {owned: true});
    } catch (error) {
      this.#ownedTabIds.delete(tab.id);
      if (this.#ownedTabIds.size === 0) this.#groupId = null;
      try {
        await this.#chrome.tabs.remove(tab.id);
      } catch {
        // Avoid replacing the original structured grouping error with cleanup noise.
      }
      throw error;
    }
  }

  async closeTarget(targetId) {
    const target = this.#requiredTarget(targetId);
    await this.#chrome.tabs.remove(target.tabId);
    await this.#removeTab(target.tabId, 'target_closed');
  }

  async attachTarget(targetId) {
    const target = this.#requiredTarget(targetId);
    if (this.#sessionByTab.has(target.tabId)) {
      throw new BridgeProtocolError('invalid_request', 'Target is already attached');
    }
    try {
      await this.#chrome.debugger.attach({tabId: target.tabId}, CDP_PROTOCOL_VERSION);
    } catch (error) {
      throw chromeError(error, 'Could not attach to the selected tab');
    }
    const sessionId = `session_${this.#randomId()}`;
    this.#sessions.set(sessionId, {targetId, tabId: target.tabId});
    this.#sessionByTab.set(target.tabId, sessionId);
    this.#enabledDomains.set(sessionId, new Set());
    return sessionId;
  }

  async detachSession(sessionId) {
    const session = this.#requiredSession(sessionId);
    this.#removeSession(sessionId);
    try {
      await this.#chrome.debugger.detach({tabId: session.tabId});
    } catch {
      // Chrome may have already detached or closed the tab.
    }
  }

  async sendCommand(sessionId, method, params) {
    if (sessionId === null) {
      throw new BridgeProtocolError(
        'unsupported_by_backend',
        'Browser-scoped CDP commands are unavailable through chrome.debugger'
      );
    }
    validateCdpMethod(method);
    if (!params || typeof params !== 'object' || Array.isArray(params)) {
      throw new BridgeProtocolError('invalid_request', 'CDP params must be an object');
    }
    if (method.startsWith('Browser.')) {
      throw new BridgeProtocolError(
        'unsupported_by_backend',
        'Browser domain commands are unavailable through this backend'
      );
    }
    const session = this.#requiredSession(sessionId);
    try {
      return (await this.#chrome.debugger.sendCommand({tabId: session.tabId}, method, params)) ?? {};
    } catch (error) {
      throw chromeError(error, `CDP command ${method} failed`);
    }
  }

  async subscribe(params) {
    const subscriptionId = requiredString(params, 'subscription_id');
    if (this.#subscriptions.has(subscriptionId)) {
      throw new BridgeProtocolError('invalid_request', 'Subscription already exists');
    }
    const sessionId = requiredString(params, 'session_id');
    const session = this.#requiredSession(sessionId);
    if (
      !Array.isArray(params.methods) ||
      params.methods.length === 0 ||
      params.methods.length > MAX_SUBSCRIPTION_METHODS
    ) {
      throw new BridgeProtocolError('invalid_request', 'Subscription methods are invalid');
    }
    const methods = new Set();
    for (const method of params.methods) {
      validateCdpMethod(method);
      if (method.startsWith('Browser.')) {
        throw new BridgeProtocolError(
          'unsupported_by_backend',
          'Browser events are unavailable through chrome.debugger'
        );
      }
      methods.add(method);
    }
    for (const domain of new Set([...methods].map(method => method.split('.')[0]))) {
      await this.#enableDomain(sessionId, session, domain);
    }
    this.#subscriptions.set(subscriptionId, {sessionId, methods});
  }

  unsubscribe(subscriptionId) {
    if (!this.#subscriptions.delete(subscriptionId)) {
      throw new BridgeProtocolError('not_found', `Unknown subscription: ${subscriptionId}`);
    }
  }

  async #enableDomain(sessionId, session, domain) {
    if (!ENABLED_EVENT_DOMAINS.has(domain)) return;
    const enabled = this.#enabledDomains.get(sessionId);
    if (enabled.has(domain)) return;
    await this.sendCommand(sessionId, `${domain}.enable`, {});
    enabled.add(domain);
  }

  #onDebuggerEvent(source, method, params) {
    if (!Number.isSafeInteger(source.tabId)) return;
    const sessionId = this.#sessionByTab.get(source.tabId);
    if (!sessionId) return;
    for (const [subscriptionId, subscription] of this.#subscriptions) {
      if (subscription.sessionId !== sessionId || !subscription.methods.has(method)) continue;
      try {
        this.#bridge.notify('extension.cdpEvent', {
          subscription_id: subscriptionId,
          session_id: sessionId,
          method,
          params
        });
      } catch {
        this.#bridge.notify('extension.eventDropped', {
          subscription_id: subscriptionId,
          method,
          reason: 'event_too_large'
        });
      }
    }
  }

  #onDebuggerDetach(source, reason) {
    if (!Number.isSafeInteger(source.tabId)) return;
    const sessionId = this.#sessionByTab.get(source.tabId);
    if (!sessionId) return;
    this.#removeSession(sessionId);
    this.#bridge.notify('extension.detached', {
      session_id: sessionId,
      reason: safeReason(reason)
    });
  }

  async #removeTab(tabId, reason) {
    const targetId = this.#targetByTab.get(tabId);
    if (!targetId) return;
    const sessionId = this.#sessionByTab.get(tabId);
    if (sessionId) this.#removeSession(sessionId);
    this.#targetByTab.delete(tabId);
    this.#targets.delete(targetId);
    this.#ownedTabIds.delete(tabId);
    if (this.#ownedTabIds.size === 0) this.#groupId = null;
    this.#publishTargets(reason);
  }

  #supportsNativeTabGroups() {
    return (
      typeof this.#chrome.tabs.group === 'function' &&
      typeof this.#chrome.tabGroups?.update === 'function'
    );
  }

  async #addTabToTaskGroup(tabId) {
    if (!this.#supportsNativeTabGroups()) {
      throw new BridgeProtocolError(
        'unsupported_by_backend',
        'Native Chrome tab groups are unavailable; update Chrome and the Browser Bridge extension'
      );
    }
    let groupId = this.#groupId;
    if (Number.isSafeInteger(groupId)) {
      try {
        groupId = await this.#chrome.tabs.group({tabIds: tabId, groupId});
      } catch {
        groupId = null;
      }
    }
    if (!Number.isSafeInteger(groupId)) {
      try {
        groupId = await this.#chrome.tabs.group({tabIds: tabId});
      } catch (error) {
        throw chromeError(error, 'Could not create a native Chrome tab group');
      }
    }
    try {
      await this.#chrome.tabGroups.update(groupId, {
        title: this.#sessionName,
        color: groupColor(this.#sessionName),
        collapsed: false
      });
    } catch (error) {
      throw chromeError(error, 'Could not name the native Chrome tab group');
    }
    this.#groupId = groupId;
    this.#ownedTabIds.add(tabId);
  }

  async #updateTab(tabId, tab) {
    const targetId = this.#targetByTab.get(tabId);
    if (!targetId) return;
    try {
      ensureControllableUrl(tab.url);
    } catch {
      await this.revokeTarget(targetId);
      return;
    }
    this.#targets.get(targetId).descriptor = tabDescriptor(targetId, tab);
    this.#publishTargets('target_updated');
  }

  #removeSession(sessionId) {
    const session = this.#sessions.get(sessionId);
    if (!session) return;
    this.#sessions.delete(sessionId);
    this.#sessionByTab.delete(session.tabId);
    this.#enabledDomains.delete(sessionId);
    for (const [subscriptionId, subscription] of this.#subscriptions) {
      if (subscription.sessionId === sessionId) this.#subscriptions.delete(subscriptionId);
    }
  }

  #publishTargets(reason = 'targets_changed') {
    this.#bridge.notify('extension.targetsChanged', {
      reason,
      targets: this.targets()
    });
  }

  #requiredTarget(targetId) {
    const target = this.#targets.get(targetId);
    if (!target) throw new BridgeProtocolError('not_found', `Unknown target: ${targetId}`);
    return target;
  }

  #requiredSession(sessionId) {
    const session = this.#sessions.get(sessionId);
    if (!session) throw new BridgeProtocolError('not_found', `Unknown session: ${sessionId}`);
    return session;
  }
}

export function ensureControllableUrl(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new BridgeProtocolError('invalid_request', 'Tab URL is invalid');
  }
  if (url.href === 'about:blank') return url.href;
  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    throw new BridgeProtocolError('permission_denied', 'Privileged or local browser URLs cannot be shared');
  }
  return url.href;
}

function tabDescriptor(targetId, tab) {
  return {
    id: targetId,
    title: typeof tab.title === 'string' ? tab.title.slice(0, 1024) : null,
    url: typeof tab.url === 'string' ? tab.url.slice(0, 16 * 1024) : null,
    kind: 'page'
  };
}

function requiredString(value, field) {
  const result = value?.[field];
  if (typeof result !== 'string' || result.length === 0 || result.length > 1024) {
    throw new BridgeProtocolError('invalid_request', `${field} is required`);
  }
  return result;
}

function normalizeGroupTitle(value) {
  const title = String(value)
    .replace(/\s+/g, ' ')
    .trim();
  if (!title) {
    throw new BridgeProtocolError('invalid_request', 'session_name is required');
  }
  return [...title].slice(0, MAX_GROUP_TITLE_LENGTH).join('');
}

function groupColor(title) {
  let hash = 2166136261;
  for (const character of title) {
    hash ^= character.codePointAt(0);
    hash = Math.imul(hash, 16777619);
  }
  return TAB_GROUP_COLORS[(hash >>> 0) % TAB_GROUP_COLORS.length];
}

function validateCdpMethod(method) {
  if (typeof method !== 'string' || !/^[A-Z][A-Za-z0-9_]*\.[A-Za-z][A-Za-z0-9_]*$/.test(method)) {
    throw new BridgeProtocolError('invalid_request', 'Invalid CDP method');
  }
}

function chromeError(error, fallback) {
  const message = error instanceof Error && error.message ? error.message.slice(0, 1024) : fallback;
  const code = /not allowed|not supported|cannot access|restricted/i.test(message)
    ? 'unsupported_by_backend'
    : 'backend_error';
  return new BridgeProtocolError(code, message);
}

function safeReason(reason) {
  return String(reason ?? 'debugger_detached')
    .replace(/[^A-Za-z0-9_-]/g, '')
    .slice(0, 128);
}
