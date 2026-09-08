import assert from 'node:assert/strict';
import test from 'node:test';
import { assertSceneDocument, bindableVariableTypesForScenePath, indexSceneDocument } from '../dist/v2-scene-schema.test.mjs';
import { resolveSceneNodeVariableBindings, resolveSceneVariable } from '../dist/v2-scene-variables.test.mjs';
import { nestedWebsite } from './helpers/v2-scene-fixture.mjs';

function variableDocument() {
  const document = nestedWebsite();
  document.variableCollections.push({
    id: 'variables-semantic',
    name: 'Semantic',
    modes: [{ id: 'semantic-default', name: 'Default' }],
    variables: [{
      id: 'variable-page-background',
      name: 'Page background',
      type: 'color',
      valuesByMode: {},
      aliasByMode: { 'semantic-default': 'variable-surface' }
    }]
  });
  return document;
}

test('variables resolve direct values and cross-collection aliases using selected modes', () => {
  const document = variableDocument();
  assertSceneDocument(document);
  assert.deepEqual(resolveSceneVariable(document, 'variable-page-background', {
    'variables-semantic': 'semantic-default',
    'variables-brand': 'mode-dark'
  }), {
    variableId: 'variable-page-background',
    collectionId: 'variables-brand',
    modeId: 'mode-dark',
    type: 'color',
    value: '#111111',
    chain: [
      { variableId: 'variable-page-background', collectionId: 'variables-semantic', modeId: 'semantic-default' },
      { variableId: 'variable-surface', collectionId: 'variables-brand', modeId: 'mode-dark' }
    ]
  });
});

test('every variable mode must define exactly one correctly typed value or alias', () => {
  const missing = nestedWebsite();
  delete missing.variableCollections[0].variables[0].valuesByMode['mode-dark'];
  assert.throws(() => assertSceneDocument(missing), /exactly one value or alias for mode mode-dark/);

  const duplicate = nestedWebsite();
  duplicate.variableCollections[0].variables[0].aliasByMode = { 'mode-light': 'variable-surface' };
  assert.throws(() => assertSceneDocument(duplicate), /exactly one value or alias for mode mode-light/);

  const wrongType = nestedWebsite();
  wrongType.variableCollections[0].variables[0].valuesByMode['mode-light'] = 42;
  assert.throws(() => assertSceneDocument(wrongType), /must be a string/);
});

test('variable aliases reject missing targets, type mismatches, and potential cycles', () => {
  const missing = variableDocument();
  missing.variableCollections[1].variables[0].aliasByMode['semantic-default'] = 'variable-missing';
  assert.throws(() => assertSceneDocument(missing), /aliases unknown variable/);

  const mismatch = variableDocument();
  mismatch.variableCollections[0].variables.push({
    id: 'variable-spacing', name: 'Spacing', type: 'number', valuesByMode: { 'mode-light': 8, 'mode-dark': 8 }
  });
  mismatch.variableCollections[1].variables[0].aliasByMode['semantic-default'] = 'variable-spacing';
  assert.throws(() => assertSceneDocument(mismatch), /cannot alias number variable/);

  const cycle = variableDocument();
  cycle.variableCollections[0].variables[0].valuesByMode = {};
  cycle.variableCollections[0].variables[0].aliasByMode = {
    'mode-light': 'variable-page-background',
    'mode-dark': 'variable-page-background'
  };
  assert.throws(() => assertSceneDocument(cycle), /Variable alias cycle/);
});

test('scene variable bindings validate property paths and semantic types', () => {
  const document = variableDocument();
  const heading = indexSceneDocument(document).get('text-hero-heading').node;
  heading.variableBindings = {
    'appearance.fills.0.color': 'variable-page-background'
  };
  assertSceneDocument(document);
  assert.deepEqual(bindableVariableTypesForScenePath(heading, 'appearance.fills.0.color'), ['color']);
  assert.deepEqual(resolveSceneNodeVariableBindings(document, heading.id, {
    'variables-brand': 'mode-light'
  })['appearance.fills.0.color'].value, '#FFFFFF');

  heading.variableBindings = { 'frame.width': 'variable-page-background' };
  assert.throws(() => assertSceneDocument(document), /cannot bind color variable .* to frame.width/);
  heading.variableBindings = { 'children.0.content': 'variable-page-background' };
  assert.throws(() => assertSceneDocument(document), /is not bindable/);
  heading.variableBindings = { 'appearance.fills.0.color': 'variable-missing' };
  assert.throws(() => assertSceneDocument(document), /binds unknown variable/);
});

test('library properties and component overrides infer bindable scalar variable types', () => {
  const document = variableDocument();
  document.variableCollections[0].variables.push({
    id: 'variable-delay', name: 'Delay', type: 'duration', valuesByMode: { 'mode-light': 120, 'mode-dark': 120 }
  });
  const frame = indexSceneDocument(document).get('frame-desktop').node;
  frame.children.push({
    ...structuredClone(frame.children[0]),
    id: 'library-motion',
    type: 'library-instance',
    name: 'Motion component',
    library: 'motion',
    component: 'Reveal',
    properties: { delay: 0 },
    slots: {},
    variableBindings: { 'properties.delay': 'variable-delay' }
  });
  assertSceneDocument(document);
  const library = indexSceneDocument(document).get('library-motion').node;
  assert.deepEqual(bindableVariableTypesForScenePath(library, 'properties.delay'), ['number', 'duration']);
});

test('resolving rejects an unknown selected mode and keeps returned chains isolated', () => {
  const document = variableDocument();
  assert.throws(() => resolveSceneVariable(document, 'variable-surface', { 'variables-brand': 'missing' }), /has no mode missing/);
  const resolved = resolveSceneVariable(document, 'variable-surface');
  resolved.chain[0].variableId = 'mutated';
  assert.equal(document.variableCollections[0].variables[0].id, 'variable-surface');
});
