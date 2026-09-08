import assert from 'node:assert/strict';
import test from 'node:test';
import { expandSceneGridTracks, parseSceneGridTrack, resolveSceneGridTracks } from '../dist/v2-grid-tracks.test.mjs';

test('grid tracks parse fixed, percent, fraction, auto, minmax, and repeat rules', () => {
  assert.deepEqual(parseSceneGridTrack('240px'), { kind: 'fixed', value: 240 });
  assert.deepEqual(parseSceneGridTrack('25%'), { kind: 'percent', value: 0.25 });
  assert.deepEqual(parseSceneGridTrack('2fr'), { kind: 'fraction', value: 2 });
  assert.deepEqual(parseSceneGridTrack('auto'), { kind: 'auto' });
  assert.deepEqual(parseSceneGridTrack('minmax(180px, 1fr)'), {
    kind: 'minmax', min: { kind: 'fixed', value: 180 }, max: { kind: 'fraction', value: 1 }
  });
  assert.deepEqual(parseSceneGridTrack('repeat(auto-fit, minmax(200px, 1fr))').count, 'auto-fit');
});

test('automatic repeat expands continuously from the available width', () => {
  assert.equal(expandSceneGridTracks(['repeat(auto-fit, minmax(200px, 1fr))'], 680, 20, 10).length, 3);
  assert.equal(expandSceneGridTracks(['repeat(auto-fit, minmax(200px, 1fr))'], 430, 20, 10).length, 2);
  assert.equal(expandSceneGridTracks(['repeat(auto-fill, 100px)'], 460, 20, 1).length, 4);
  assert.equal(expandSceneGridTracks(['repeat(auto-fit, 100px)'], 460, 20, 2).length, 2);
});

test('fraction and minmax tracks divide remaining width after fixed tracks and gaps', () => {
  const expanded = expandSceneGridTracks(['200px', '1fr', '2fr'], 800, 20, 3);
  assert.deepEqual(resolveSceneGridTracks(expanded, 800, 20).map(Math.round), [200, 187, 373]);
  const cards = expandSceneGridTracks(['repeat(3, minmax(180px, 1fr))'], 900, 24, 3);
  assert.deepEqual(resolveSceneGridTracks(cards, 900, 24).map(Math.round), [284, 284, 284]);
});

test('invalid or unbounded automatic track rules are rejected', () => {
  assert.throws(() => parseSceneGridTrack('minmax(1fr, 2fr)'), /cannot use fr as a minmax minimum/);
  assert.throws(() => parseSceneGridTrack('repeat(auto-fit, 1fr)'), /measurable minimum/);
  assert.throws(() => parseSceneGridTrack('repeat(0, 100px)'), /invalid repeat count/);
  assert.throws(() => parseSceneGridTrack('calc(100% - 20px)'), /unsupported/);
});
