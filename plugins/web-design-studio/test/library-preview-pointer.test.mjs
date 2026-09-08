import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { translateLibraryPreviewPointerEvent } from '../dist/library-preview-pointer.test.mjs';

const selection = {
  path: '1.0.0.0',
  label: '动画文字',
  x: 0,
  y: 0,
  width: 468,
  height: 32,
  viewportWidth: 468,
  viewportHeight: 150
};

test('iframe preview pointer messages translate into parent viewport coordinates', () => {
  assert.deepEqual(translateLibraryPreviewPointerEvent({
    selection,
    pointerId: 7,
    clientX: 234,
    clientY: 16,
    phase: 'end'
  }, 456, 197), {
    selection,
    pointerId: 7,
    clientX: 690,
    clientY: 213,
    phase: 'end'
  });
});

test('malformed preview pointer messages are rejected', () => {
  assert.equal(translateLibraryPreviewPointerEvent({ selection, pointerId: 1, clientX: 2, clientY: 3, phase: 'drop' }, 0, 0), undefined);
  assert.equal(translateLibraryPreviewPointerEvent({ selection: { ...selection, width: 0 }, pointerId: 1, clientX: 2, clientY: 3, phase: 'start' }, 0, 0), undefined);
});

test('preview picker keeps clicks local and captures only a real drag', () => {
  const picker = readFileSync('ui-src/library-runtime/preview-picker.ts', 'utf8');
  const studio = readFileSync('ui-src/studio/WebDesignStudioApp.tsx', 'utf8');
  assert.match(picker, /const pickableAtPoint = \(clientX: number, clientY: number\)/);
  assert.match(picker, /return pickableAtPoint\(pointer\.clientX, pointer\.clientY\);/);
  assert.match(picker, /Math\.hypot\(event\.clientX - activePointer\.startClientX, event\.clientY - activePointer\.startClientY\)/);
  assert.match(picker, /if \(distance < 5\) return;/);
  assert.match(picker, /else if \(!cancelled\) \{\s*emitSelect\(active\.selection\);/);
  assert.match(studio, /\{variantPickerDrag\?\.dragging && <div\s*className="variant-picker-pointer-capture"/);
  assert.doesNotMatch(studio, /\{variantPickerDrag && <div\s*className="variant-picker-pointer-capture"/);
});
