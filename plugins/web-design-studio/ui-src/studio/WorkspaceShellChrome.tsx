import { memo, useRef, type PointerEvent as ReactPointerEvent } from 'react';
import { workspaceToolInstruction, type WorkspaceArea, type WorkspaceTool } from './workspace-shell-model';

const navigationItems: Array<{ area: WorkspaceArea; icon: string; label: string }> = [
  { area: 'layers', icon: '▤', label: '页面与图层' },
  { area: 'assets', icon: '◫', label: '组件与资产' },
  { area: 'tools', icon: '◇', label: '视觉工具' },
  { area: 'my', icon: '☆', label: '我的组件' },
  { area: 'variables', icon: '◉', label: '变量与样式' },
  { area: 'ai', icon: '✦', label: 'AI 任务' }
];

export const WorkspaceNavigationBar = memo(function WorkspaceNavigationBar({ activeArea, leftPanelOpen, onSelect, onToggleLeft }: {
  activeArea: WorkspaceArea;
  leftPanelOpen: boolean;
  onSelect: (area: WorkspaceArea) => void;
  onToggleLeft: () => void;
}) {
  return <nav className="workspace-navigation" aria-label="工作区导航">
    <div className="workspace-navigation-main">
      {navigationItems.map((item) => <button key={item.area} className={activeArea === item.area ? 'active' : ''} title={item.label} aria-label={item.label} onClick={() => onSelect(item.area)}><span>{item.icon}</span><small>{item.label}</small></button>)}
    </div>
    <button className="workspace-navigation-collapse" title={leftPanelOpen ? '收起左侧栏' : '展开左侧栏'} aria-label={leftPanelOpen ? '收起左侧栏' : '展开左侧栏'} onClick={onToggleLeft}>{leftPanelOpen ? '‹' : '›'}</button>
  </nav>;
});

const tools: Array<{ tool: WorkspaceTool; icon: string; label: string; shortcut?: string }> = [
  { tool: 'select', icon: '↖', label: '选择', shortcut: 'V' },
  { tool: 'hand', icon: '✋', label: '移动画布', shortcut: 'H' },
  { tool: 'insert', icon: '＋', label: '添加元素', shortcut: 'I' },
  { tool: 'comment', icon: '●', label: '批注', shortcut: 'C' },
  { tool: 'ai', icon: '✦', label: 'AI 修改' }
];

export const WorkspaceBottomToolbar = memo(function WorkspaceBottomToolbar({ activeTool, leftPanelOpen, rightPanelOpen, canvasMaximized, onSelectTool, onToggleLeft, onToggleRight, onToggleMaximize }: {
  activeTool: WorkspaceTool;
  leftPanelOpen: boolean;
  rightPanelOpen: boolean;
  canvasMaximized: boolean;
  onSelectTool: (tool: WorkspaceTool) => void;
  onToggleLeft: () => void;
  onToggleRight: () => void;
  onToggleMaximize: () => void;
}) {
  const active = tools.find((item) => item.tool === activeTool) ?? tools[0];
  return <div className="workspace-bottom-toolbar-shell">
    <div className="workspace-tool-instruction" role="status"><strong>{active.label}</strong><span>{workspaceToolInstruction(activeTool)}</span></div>
    <div className="workspace-bottom-toolbar" role="toolbar" aria-label="画布工具">
      <button className="panel-toggle" title={leftPanelOpen ? '收起左侧栏' : '展开左侧栏'} aria-label={leftPanelOpen ? '收起左侧栏' : '展开左侧栏'} onClick={onToggleLeft}><b>{leftPanelOpen ? '◧' : '▯'}</b><em>左栏</em></button>
      <span />
      {tools.map((item) => <button key={item.tool} className={`workspace-tool-button ${activeTool === item.tool ? 'active' : ''}`} title={`${item.label}${item.shortcut ? ` · ${item.shortcut}` : ''}`} aria-label={`${item.label}${item.shortcut ? `，快捷键 ${item.shortcut}` : ''}`} onClick={() => onSelectTool(item.tool)}><b>{item.icon}</b><em>{item.label}</em>{item.shortcut && <kbd>{item.shortcut}</kbd>}</button>)}
      <span />
      <button className="panel-toggle" title={rightPanelOpen ? '收起属性栏' : '展开属性栏'} aria-label={rightPanelOpen ? '收起属性栏' : '展开属性栏'} onClick={onToggleRight}><b>{rightPanelOpen ? '◨' : '▯'}</b><em>属性</em></button>
      <button className={canvasMaximized ? 'active panel-toggle' : 'panel-toggle'} title={canvasMaximized ? '恢复左右面板' : '隐藏面板，专注画布'} aria-label={canvasMaximized ? '恢复左右面板' : '隐藏面板，专注画布'} onClick={onToggleMaximize}><b>{canvasMaximized ? '⊡' : '□'}</b><em>{canvasMaximized ? '恢复' : '专注'}</em></button>
    </div>
  </div>;
});

export const WorkspacePanelResizeHandle = memo(function WorkspacePanelResizeHandle({ side, width, onResize }: {
  side: 'left' | 'right';
  width: number;
  onResize: (width: number) => void;
}) {
  const start = useRef<{ x: number; width: number } | undefined>(undefined);
  function onPointerDown(event: ReactPointerEvent<HTMLDivElement>) {
    event.preventDefault();
    start.current = { x: event.clientX, width };
    event.currentTarget.setPointerCapture(event.pointerId);
  }
  function onPointerMove(event: ReactPointerEvent<HTMLDivElement>) {
    if (!start.current || !event.currentTarget.hasPointerCapture(event.pointerId)) return;
    const delta = event.clientX - start.current.x;
    onResize(start.current.width + (side === 'left' ? delta : -delta));
  }
  function finish(event: ReactPointerEvent<HTMLDivElement>) {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    start.current = undefined;
  }
  return <div className={`workspace-panel-resize workspace-panel-resize-${side}`} role="separator" aria-orientation="vertical" aria-label={`调整${side === 'left' ? '左侧栏' : '属性栏'}宽度`} onPointerDown={onPointerDown} onPointerMove={onPointerMove} onPointerUp={finish} onPointerCancel={finish} />;
});
