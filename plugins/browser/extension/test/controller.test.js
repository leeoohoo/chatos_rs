import assert from 'node:assert/strict';
import test from 'node:test';

import {BridgeProtocolError} from '../src/bridge.js';
import {ExtensionController, ensureControllableUrl} from '../src/controller.js';

test('only explicitly shared web tabs enter the target catalog', async () => {
  const harness = createHarness();
  const controller = new ExtensionController({
    chromeApi: harness.chrome,
    bridge: harness.bridge,
    randomId: sequence('one', 'two')
  });
  controller.start();

  assert.deepEqual(controller.targets(), []);
  const target = await controller.shareTab(7);
  assert.equal(target.id, 'target_one');
  assert.equal(target.url, 'https://example.test/');
  assert.equal(controller.targets().length, 1);
  assert.equal(harness.notifications.at(-1).method, 'extension.targetsChanged');

  await assert.rejects(() => controller.shareTab(8), error => {
    assert.equal(error.code, 'permission_denied');
    return true;
  });
  assert.equal(controller.targets().length, 1);
});

test('authorized targets route CDP commands and exact event subscriptions', async () => {
  const harness = createHarness();
  const controller = new ExtensionController({
    chromeApi: harness.chrome,
    bridge: harness.bridge,
    randomId: sequence('target', 'session')
  });
  controller.start();
  const target = await controller.shareTab(7);
  const attached = await controller.handleRequest('extension.attachTarget', {target_id: target.id});
  assert.equal(attached.session_id, 'session_session');
  assert.deepEqual(harness.attachments, [{source: {tabId: 7}, version: '1.3'}]);

  const evaluated = await controller.handleRequest('extension.cdpSend', {
    session_id: attached.session_id,
    method: 'Runtime.evaluate',
    params: {expression: '1 + 1'}
  });
  assert.equal(evaluated.result.result.value, 2);

  await assert.rejects(
    () =>
      controller.handleRequest('extension.cdpSend', {
        session_id: null,
        method: 'Browser.getVersion',
        params: {}
      }),
    error => error instanceof BridgeProtocolError && error.code === 'unsupported_by_backend'
  );

  await controller.handleRequest('extension.subscribe', {
    subscription_id: 'sub_console',
    session_id: attached.session_id,
    methods: ['Runtime.consoleAPICalled']
  });
  assert.equal(
    harness.commands.some(command => command.method === 'Runtime.enable'),
    true
  );
  harness.debuggerEvents.emit(
    {tabId: 7},
    'Runtime.consoleAPICalled',
    {type: 'log', args: []}
  );
  const event = harness.notifications.at(-1);
  assert.equal(event.method, 'extension.cdpEvent');
  assert.equal(event.params.subscription_id, 'sub_console');

  await controller.handleRequest('extension.detachTarget', {session_id: attached.session_id});
  assert.deepEqual(harness.detachments, [{tabId: 7}]);
});

test('closing or revoking a tab removes its opaque target and sessions', async () => {
  const harness = createHarness();
  const controller = new ExtensionController({
    chromeApi: harness.chrome,
    bridge: harness.bridge,
    randomId: sequence('target', 'session')
  });
  controller.start();
  const target = await controller.shareTab(7);
  const {session_id: sessionId} = await controller.handleRequest('extension.attachTarget', {
    target_id: target.id
  });
  await controller.revokeTarget(target.id);
  assert.deepEqual(controller.targets(), []);
  await assert.rejects(
    () => controller.sendCommand(sessionId, 'Runtime.evaluate', {}),
    error => error.code === 'not_found'
  );
});

test('agent-created tabs enter one named native Chrome tab group while shared tabs stay put', async () => {
  const harness = createHarness();
  const controller = new ExtensionController({
    chromeApi: harness.chrome,
    bridge: harness.bridge,
    randomId: sequence('shared', 'owned-one', 'owned-two')
  });
  controller.start();
  await controller.shareTab(7);
  await controller.handleRequest('extension.configureSession', {session_name: '  WMS   发布验证  '});

  await controller.handleRequest('extension.createTarget', {url: 'https://one.test/'});
  await controller.handleRequest('extension.createTarget', {url: 'https://two.test/'});

  assert.deepEqual(harness.groupCalls, [
    {tabIds: 9},
    {tabIds: 10, groupId: 101}
  ]);
  assert.equal(harness.groupUpdates[0].groupId, 101);
  assert.equal(harness.groupUpdates[0].update.title, 'WMS 发布验证');
  assert.equal(harness.groupUpdates[0].update.collapsed, false);
  assert.equal(harness.groupUpdates[1].groupId, 101);
  assert.deepEqual(controller.status().ownedTabIds, [9, 10]);
  assert.equal(controller.targets().some(target => target.id === 'target_shared'), true);
});

test('ending a session preserves and collapses owned tabs but removes them from the next target catalog', async () => {
  const harness = createHarness();
  const controller = new ExtensionController({
    chromeApi: harness.chrome,
    bridge: harness.bridge,
    randomId: sequence('shared', 'owned')
  });
  controller.start();
  const shared = await controller.shareTab(7);
  await controller.handleRequest('extension.configureSession', {session_name: 'Task A'});
  await controller.handleRequest('extension.createTarget', {url: 'https://owned.test/'});

  await controller.handleRequest('extension.endSession');

  assert.deepEqual(controller.targets().map(target => target.id), [shared.id]);
  assert.equal(harness.tabs.has(9), true);
  assert.deepEqual(harness.groupUpdates.at(-1), {
    groupId: 101,
    update: {collapsed: true}
  });
  assert.deepEqual(controller.status().ownedTabIds, []);
});

test('missing native tab-group APIs return a structured capability error', async () => {
  const harness = createHarness({nativeTabGroups: false});
  const controller = new ExtensionController({chromeApi: harness.chrome, bridge: harness.bridge});
  controller.start();
  const capabilities = await controller.handleRequest('extension.getCapabilities');
  assert.equal(capabilities.capabilities.includes('native_tab_groups'), false);
  await assert.rejects(
    () => controller.handleRequest('extension.configureSession', {session_name: 'Task A'}),
    error => error.code === 'unsupported_by_backend'
  );
});

test('URL policy rejects privileged and local schemes', () => {
  assert.equal(ensureControllableUrl('about:blank'), 'about:blank');
  assert.equal(ensureControllableUrl('https://example.test/a'), 'https://example.test/a');
  for (const url of [
    'chrome://settings/',
    'chrome-extension://abc/page.html',
    'devtools://devtools/bundled/',
    'file:///tmp/secret.txt'
  ]) {
    assert.throws(() => ensureControllableUrl(url), error => error.code === 'permission_denied');
  }
});

function createHarness({nativeTabGroups = true} = {}) {
  const debuggerEvents = hook();
  const debuggerDetach = hook();
  const tabRemoved = hook();
  const tabUpdated = hook();
  const notifications = [];
  const attachments = [];
  const detachments = [];
  const commands = [];
  const groupCalls = [];
  const groupUpdates = [];
  const tabs = new Map([
    [7, {id: 7, title: 'Example', url: 'https://example.test/'}],
    [8, {id: 8, title: 'Settings', url: 'chrome://settings/'}]
  ]);
  let nextTabId = 9;
  const chrome = {
    debugger: {
      onEvent: debuggerEvents,
      onDetach: debuggerDetach,
      async attach(source, version) {
        attachments.push({source, version});
      },
      async detach(source) {
        detachments.push(source);
      },
      async sendCommand(source, method, params) {
        commands.push({source, method, params});
        if (method === 'Runtime.evaluate') {
          return {result: {type: 'number', value: 2}};
        }
        return {};
      }
    },
    tabs: {
      onRemoved: tabRemoved,
      onUpdated: tabUpdated,
      async get(tabId) {
        return {...tabs.get(tabId)};
      },
      async create({url}) {
        const tab = {id: nextTabId++, title: 'New tab', url};
        tabs.set(tab.id, tab);
        return {...tab};
      },
      async remove(tabId) {
        tabs.delete(tabId);
      }
    }
  };
  if (nativeTabGroups) {
    chrome.tabs.group = async options => {
      groupCalls.push({...options});
      return options.groupId ?? 101;
    };
    chrome.tabGroups = {
      async update(groupId, update) {
        groupUpdates.push({groupId, update: {...update}});
        return {id: groupId, ...update};
      }
    };
  }
  return {
    debuggerEvents,
    notifications,
    attachments,
    detachments,
    commands,
    groupCalls,
    groupUpdates,
    tabs,
    bridge: {
      notify(method, params) {
        notifications.push({method, params});
        return true;
      }
    },
    chrome
  };
}

function hook() {
  const listeners = new Set();
  return {
    addListener(listener) {
      listeners.add(listener);
    },
    removeListener(listener) {
      listeners.delete(listener);
    },
    emit(...args) {
      for (const listener of listeners) listener(...args);
    }
  };
}

function sequence(...values) {
  let index = 0;
  return () => values[index++];
}
