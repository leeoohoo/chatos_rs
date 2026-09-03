import {BridgeProtocolError, LocalConnectorBridge} from './bridge.js';
import {ExtensionController} from './controller.js';

const bridge = new LocalConnectorBridge();
const controller = new ExtensionController({bridge});
const AUTO_RECONNECT_DELAY_MS = 2000;
const ONBOARDING_PATH = 'onboarding/onboarding.html';

let pairingEnabled = false;
let reconnectTimer = null;

controller.start();
bridge.setRequestHandler((method, params) => controller.handleRequest(method, params));
bridge.onStateChange(({connected}) => {
  void updateActionBadge(connected);
  if (connected) {
    cancelReconnect();
  } else {
    void controller.revokeAll();
    scheduleReconnect();
  }
});

chrome.runtime.onInstalled.addListener(({reason}) => {
  if (reason === 'install' || reason === 'update') {
    void openOnboardingIfNeeded();
  }
});

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (sender.id !== chrome.runtime.id) {
    sendResponse({ok: false, error: {code: 'permission_denied', message: 'Untrusted sender'}});
    return false;
  }
  void handlePopupMessage(message)
    .then(result => sendResponse({ok: true, result}))
    .catch(error => {
      const protocolError = normalizeError(error);
      sendResponse({
        ok: false,
        error: {code: protocolError.code, message: protocolError.message.slice(0, 1024)}
      });
    });
  return true;
});

void restorePairing();

async function restorePairing() {
  const {paired = false} = await chrome.storage.local.get('paired');
  pairingEnabled = paired;
  await updateActionBadge(bridge.connected);
  if (!pairingEnabled) return;
  await reconnectPairedBridge();
}

async function reconnectPairedBridge() {
  if (!pairingEnabled || bridge.connected) return;
  try {
    await bridge.connect({pairingRequested: false});
  } catch {
    // The native host or MCP process may still be starting. Keep the pairing and retry.
    scheduleReconnect();
  }
}

function scheduleReconnect() {
  if (!pairingEnabled || bridge.connected || reconnectTimer !== null) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    void reconnectPairedBridge();
  }, AUTO_RECONNECT_DELAY_MS);
}

function cancelReconnect() {
  if (reconnectTimer === null) return;
  clearTimeout(reconnectTimer);
  reconnectTimer = null;
}

async function handlePopupMessage(message) {
  switch (message?.action) {
    case 'status':
      return {connected: bridge.connected, paired: pairingEnabled, ...controller.status()};
    case 'connect':
      await bridge.connect({pairingRequested: true});
      pairingEnabled = true;
      await chrome.storage.local.set({paired: true, onboarding_completed: true});
      await updateActionBadge(true);
      return {connected: true, paired: true, ...controller.status()};
    case 'open_onboarding':
      await openOnboarding();
      return {connected: bridge.connected, paired: pairingEnabled, ...controller.status()};
    case 'share_current_tab': {
      if (!bridge.connected) {
        throw new BridgeProtocolError('extension_unavailable', 'Connect to Browser MCP first');
      }
      const [tab] = await chrome.tabs.query({active: true, currentWindow: true});
      if (!Number.isSafeInteger(tab?.id)) {
        throw new BridgeProtocolError('not_found', 'No active tab is available');
      }
      const target = await controller.shareTab(tab.id);
      return {connected: bridge.connected, paired: pairingEnabled, target, ...controller.status()};
    }
    case 'revoke_target':
      await controller.revokeTarget(message.target_id);
      return {connected: bridge.connected, paired: pairingEnabled, ...controller.status()};
    case 'disconnect':
      pairingEnabled = false;
      cancelReconnect();
      await chrome.storage.local.set({paired: false});
      await updateActionBadge(false);
      bridge.notify('extension.unpair', {});
      await bridge.disconnect('user_disconnected');
      await controller.revokeAll();
      return {connected: false, paired: false, ...controller.status()};
    default:
      throw new BridgeProtocolError('invalid_request', 'Unknown popup action');
  }
}

async function openOnboardingIfNeeded() {
  const {paired = false, onboarding_completed: onboardingCompleted = false} =
    await chrome.storage.local.get(['paired', 'onboarding_completed']);
  if (paired) {
    if (!onboardingCompleted) await chrome.storage.local.set({onboarding_completed: true});
    return;
  }
  if (onboardingCompleted) return;
  await openOnboarding();
}

async function openOnboarding() {
  const url = chrome.runtime.getURL(ONBOARDING_PATH);
  const tabs = await chrome.tabs.query({});
  const existing = tabs.find(tab => tab.url === url);
  if (Number.isSafeInteger(existing?.id)) {
    await chrome.tabs.update(existing.id, {active: true});
    if (Number.isSafeInteger(existing.windowId)) {
      await chrome.windows.update(existing.windowId, {focused: true});
    }
    return;
  }
  await chrome.tabs.create({url, active: true});
}

async function updateActionBadge(connected = bridge.connected) {
  const text = connected ? 'ON' : pairingEnabled ? '…' : '!';
  const color = connected ? '#16A34A' : pairingEnabled ? '#D97706' : '#DC2626';
  await chrome.action.setBadgeText({text});
  await chrome.action.setBadgeBackgroundColor({color});
}

function normalizeError(error) {
  if (error instanceof BridgeProtocolError) return error;
  return new BridgeProtocolError(
    'backend_error',
    error instanceof Error ? error.message : 'Unknown extension error'
  );
}
