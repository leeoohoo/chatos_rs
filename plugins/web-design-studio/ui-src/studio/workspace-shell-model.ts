export type WorkspaceArea = 'layers' | 'assets' | 'tools' | 'my' | 'ai' | 'variables';
export type WorkspaceTool = 'select' | 'hand' | 'insert' | 'comment' | 'ai';

export interface WorkspaceShellState {
  activeArea: WorkspaceArea;
  activeTool: WorkspaceTool;
  leftPanelOpen: boolean;
  rightPanelOpen: boolean;
  leftPanelWidth: number;
  rightPanelWidth: number;
  canvasMaximized: boolean;
}

export type WorkspaceShellAction =
  | { type: 'select-area'; area: WorkspaceArea }
  | { type: 'select-tool'; tool: WorkspaceTool }
  | { type: 'toggle-left-panel' }
  | { type: 'toggle-right-panel' }
  | { type: 'resize-left-panel'; width: number }
  | { type: 'resize-right-panel'; width: number }
  | { type: 'toggle-canvas-maximized' }
  | { type: 'restore-panels' };

export const DEFAULT_WORKSPACE_SHELL: WorkspaceShellState = {
  activeArea: 'assets',
  activeTool: 'select',
  leftPanelOpen: true,
  rightPanelOpen: true,
  leftPanelWidth: 272,
  rightPanelWidth: 320,
  canvasMaximized: false
};

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, Math.round(value)));
}

export function workspaceShellReducer(state: WorkspaceShellState, action: WorkspaceShellAction): WorkspaceShellState {
  if (action.type === 'select-area') {
    return { ...state, activeArea: action.area, leftPanelOpen: true, canvasMaximized: false };
  }
  if (action.type === 'select-tool') return { ...state, activeTool: action.tool };
  if (action.type === 'toggle-left-panel') return { ...state, leftPanelOpen: !state.leftPanelOpen, canvasMaximized: false };
  if (action.type === 'toggle-right-panel') return { ...state, rightPanelOpen: !state.rightPanelOpen, canvasMaximized: false };
  if (action.type === 'resize-left-panel') return { ...state, leftPanelWidth: clamp(action.width, 220, 460) };
  if (action.type === 'resize-right-panel') return { ...state, rightPanelWidth: clamp(action.width, 260, 520) };
  if (action.type === 'toggle-canvas-maximized') {
    const canvasMaximized = !state.canvasMaximized;
    return { ...state, canvasMaximized, leftPanelOpen: !canvasMaximized, rightPanelOpen: !canvasMaximized };
  }
  return { ...state, leftPanelOpen: true, rightPanelOpen: true, canvasMaximized: false };
}

export function workspaceShellGridStyle(state: WorkspaceShellState): Record<string, string> {
  return {
    '--workspace-left-width': state.leftPanelOpen ? `${state.leftPanelWidth}px` : '0px',
    '--workspace-right-width': state.rightPanelOpen ? `${state.rightPanelWidth}px` : '0px'
  };
}

export function parseWorkspaceShellState(source: string | null | undefined): WorkspaceShellState {
  if (!source) return DEFAULT_WORKSPACE_SHELL;
  try {
    const value = JSON.parse(source) as Partial<WorkspaceShellState>;
    const activeAreas: WorkspaceArea[] = ['layers', 'assets', 'tools', 'my', 'ai', 'variables'];
    const activeTools: WorkspaceTool[] = ['select', 'hand', 'insert', 'comment', 'ai'];
    let state = { ...DEFAULT_WORKSPACE_SHELL };
    if (value.activeArea && activeAreas.includes(value.activeArea)) state.activeArea = value.activeArea;
    if (value.activeTool && activeTools.includes(value.activeTool)) state.activeTool = value.activeTool;
    if (typeof value.leftPanelOpen === 'boolean') state.leftPanelOpen = value.leftPanelOpen;
    if (typeof value.rightPanelOpen === 'boolean') state.rightPanelOpen = value.rightPanelOpen;
    if (typeof value.canvasMaximized === 'boolean') state.canvasMaximized = value.canvasMaximized;
    if (typeof value.leftPanelWidth === 'number') state = workspaceShellReducer(state, { type: 'resize-left-panel', width: value.leftPanelWidth });
    if (typeof value.rightPanelWidth === 'number') state = workspaceShellReducer(state, { type: 'resize-right-panel', width: value.rightPanelWidth });
    if (state.canvasMaximized) return { ...state, leftPanelOpen: false, rightPanelOpen: false };
    return state;
  } catch {
    return DEFAULT_WORKSPACE_SHELL;
  }
}

export function workspaceShellShortcut(key: string, command: boolean): WorkspaceShellAction | undefined {
  const normalized = key.toLowerCase();
  if (command && key === '\\') return { type: 'toggle-canvas-maximized' };
  if (command && key === '[') return { type: 'toggle-left-panel' };
  if (command && key === ']') return { type: 'toggle-right-panel' };
  if (command) return undefined;
  if (normalized === 'v') return { type: 'select-tool', tool: 'select' };
  if (normalized === 'h') return { type: 'select-tool', tool: 'hand' };
  if (normalized === 'i') return { type: 'select-tool', tool: 'insert' };
  if (normalized === 'c') return { type: 'select-tool', tool: 'comment' };
  return undefined;
}

export function workspaceToolInstruction(tool: WorkspaceTool): string {
  if (tool === 'hand') return '按住并拖动画布，查看页面四周的自由工作区';
  if (tool === 'insert') return '从左侧选择形状、文字或容器，再拖到画布';
  if (tool === 'comment') return '点击一个组件，然后在右侧写批注或让 AI 修改';
  if (tool === 'ai') return '在左侧描述整页目标，或安排 AI 设计任务';
  return '点击组件进行选择；拖动组件改变位置，拖动控制点改变大小';
}
