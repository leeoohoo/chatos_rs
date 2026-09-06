import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { DAISYUI_REGISTRY_ENTRIES } from '../dist/daisyui-registry.test.mjs';

test('daisyUI runtime is generated from every official v5.7.28 HTML example', () => {
  assert.equal(DAISYUI_REGISTRY_ENTRIES.length, 68);
  assert.equal(DAISYUI_REGISTRY_ENTRIES.flatMap((entry) => entry.demos).length, 587);
  assert.equal(DAISYUI_REGISTRY_ENTRIES.every((entry) => entry.demos.length >= 1), true);
  assert.equal(DAISYUI_REGISTRY_ENTRIES.every((entry) => entry.docsUrl === `https://daisyui.com/components/${entry.slug}/`), true);
  assert.equal(DAISYUI_REGISTRY_ENTRIES.flatMap((entry) => entry.demos).every((demo) => demo.id && demo.label), true);
});

test('daisyUI runtime preserves official structures for interactive and content-rich components', () => {
  const bySlug = Object.fromEntries(DAISYUI_REGISTRY_ENTRIES.map((entry) => [entry.slug, entry]));
  assert.equal(bySlug.button.demos.length, 18);
  assert.equal(bySlug.dropdown.demos.length, 24);
  assert.equal(bySlug.list.demos.length, 3);
  assert.deepEqual(bySlug.list.demos.map((demo) => demo.id), ['list-1', 'list-2', 'list-3']);
});

test('daisyUI official HTML payloads remain separate, interactive runtime assets', async () => {
  const read = async (slug) => JSON.parse(await readFile(new URL(`../ui-src/public/daisyui/${slug}.json`, import.meta.url), 'utf8'));
  const [list, drawer, modal, select] = await Promise.all(['list', 'drawer', 'modal', 'select'].map(read));
  assert.match(list.demos[0].html, /class="list-row"/);
  assert.match(drawer.demos[0].html, /class="drawer-toggle"/);
  assert.match(modal.demos[0].html, /<dialog[^>]+class="modal"/);
  assert.match(select.demos[0].html, /<select[^>]+class="select"/);
  assert.equal([list, drawer, modal, select].flatMap((entry) => entry.demos).every((demo) => !demo.html.includes('$$')), true);
});
