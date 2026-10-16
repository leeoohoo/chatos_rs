'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');
const contract = import('../../scripts/chrome-verification-contract.mjs');

test('Chrome verification uses public arguments and always closes its session', async () => {
  const {withChromeSession, createVerificationTabs} = await contract;
  const calls = [];
  const callTool = async (name, args) => {
    calls.push({name, args});
    if (name === 'browser_session_open') return {mode: 'chrome_extension'};
    return {tab_id: `tab-${calls.length}`};
  };
  const tabs = await withChromeSession(callTool, 'Contract fixture', () => createVerificationTabs(callTool));
  assert.equal(tabs.length, 2);
  assert.deepEqual(calls, [
    {name: 'browser_session_open', args: {session_name: 'Contract fixture'}},
    {name: 'browser_tab_new', args: {url: 'https://example.com/?browser-mcp-verification=one'}},
    {name: 'browser_tab_new', args: {url: 'https://example.com/?browser-mcp-verification=two'}},
    {name: 'browser_session_close', args: {}}
  ]);
});

test('Chrome verification closes fallback without performing browser work', async () => {
  const {withChromeSession} = await contract;
  const calls = [];
  await assert.rejects(withChromeSession(async (name, args) => {
    calls.push({name, args});
    return {mode: 'managed'};
  }, 'Fallback fixture', () => assert.fail('must not run')), /Paired Chrome is unavailable/);
  assert.deepEqual(calls.map(call => call.name), ['browser_session_open', 'browser_session_close']);
});

test('Chrome verification closes after work failure and never loops on open failure', async () => {
  const {withChromeSession} = await contract;
  const calls = [];
  await assert.rejects(withChromeSession(async name => {
    calls.push(name);
    return {mode: 'chrome_extension'};
  }, 'Failure fixture', () => { throw new Error('work failed'); }), /work failed/);
  assert.deepEqual(calls, ['browser_session_open', 'browser_session_close']);
  calls.length = 0;
  await assert.rejects(withChromeSession(async name => {
    calls.push(name);
    throw new Error('open failed');
  }, 'Open failure', () => assert.fail('must not run')), /open failed/);
  assert.deepEqual(calls, ['browser_session_open']);
});
