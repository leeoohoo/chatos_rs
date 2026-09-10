import assert from 'node:assert/strict';
import test from 'node:test';
import { DEFAULT_WORKSPACE_SHELL, parseWorkspaceShellState, workspaceShellGridStyle, workspaceShellReducer, workspaceShellShortcut } from '../dist/workspace-shell-model.test.mjs';

test('workspace shell independently selects areas, resizes panels, and maximizes the canvas', () => {
  let state = workspaceShellReducer(DEFAULT_WORKSPACE_SHELL, { type: 'select-area', area: 'layers' });
  assert.equal(state.activeArea, 'layers');
  state = workspaceShellReducer(state, { type: 'resize-left-panel', width: 1000 });
  state = workspaceShellReducer(state, { type: 'resize-right-panel', width: 100 });
  assert.equal(state.leftPanelWidth, 460);
  assert.equal(state.rightPanelWidth, 260);
  state = workspaceShellReducer(state, { type: 'toggle-canvas-maximized' });
  assert.equal(state.canvasMaximized, true);
  assert.equal(state.leftPanelOpen, false);
  assert.equal(state.rightPanelOpen, false);
  assert.deepEqual(workspaceShellGridStyle(state), { '--workspace-left-width': '0px', '--workspace-right-width': '0px' });
  state = workspaceShellReducer(state, { type: 'restore-panels' });
  assert.equal(state.leftPanelOpen, true);
  assert.equal(state.rightPanelOpen, true);
});

test('workspace shell persistence rejects malformed state and clamps restored panel widths', () => {
  assert.deepEqual(parseWorkspaceShellState('{broken'), DEFAULT_WORKSPACE_SHELL);
  const restored = parseWorkspaceShellState(JSON.stringify({ activeArea: 'variables', activeTool: 'hand', leftPanelWidth: 900, rightPanelWidth: 20 }));
  assert.equal(restored.activeArea, 'variables');
  assert.equal(restored.activeTool, 'hand');
  assert.equal(restored.leftPanelWidth, 460);
  assert.equal(restored.rightPanelWidth, 260);
  assert.deepEqual(workspaceShellShortcut('\\', true), { type: 'toggle-canvas-maximized' });
  assert.deepEqual(workspaceShellShortcut('h', false), { type: 'select-tool', tool: 'hand' });
  assert.deepEqual(workspaceShellShortcut('i', false), { type: 'select-tool', tool: 'insert' });
});
