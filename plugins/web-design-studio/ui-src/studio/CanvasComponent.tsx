import { useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react';
import type { ResolvedWebDesignComponent } from '../../src/editor-model';
import type { WebComponentStyle, WebComponentVisualState, WebDesignComponent, WebDesignTokens } from '../../src/schema';
import { LibraryCanvasComponent } from './LibraryCanvasComponent';
import { componentEffectStyleToCss, componentStyleToCss, mergeComponentStyles } from './component-style';

export function CanvasComponentContent({ component, style = component.style, interactive, tokens, slotContent }: { component: WebDesignComponent; style?: WebComponentStyle; interactive: boolean; tokens?: WebDesignTokens; slotContent?: Record<string, ReactNode> }) {
  const renderedComponent = style === component.style ? component : { ...component, style };
  if (component.library) return <div className={`ui-library-canvas-content library-${component.library.name} ${interactive ? 'preview' : ''}`}><LibraryCanvasComponent component={renderedComponent} preview={interactive} tokens={tokens} slotContent={slotContent} /></div>;
  if (component.type === 'image') return component.content ? <img src={component.content} alt={component.name} draggable={false} style={{ objectFit: style.objectFit, objectPosition: style.objectPosition }} /> : <span className="image-placeholder">图片</span>;
  if (component.type === 'video') return component.content ? <video src={component.content} controls={interactive} muted style={{ objectFit: style.objectFit, objectPosition: style.objectPosition }} /> : <span className="media-placeholder">▶<small>视频</small></span>;
  if (component.type === 'input') return <span className="input-placeholder">{component.content}</span>;
  if (component.type === 'textarea') return <span className="input-placeholder textarea-placeholder">{component.content}</span>;
  if (component.type === 'select') return <span className="select-placeholder"><span>{component.content.split('\n')[0]}</span><b>⌄</b></span>;
  if (component.type === 'checkbox') return <span className="choice-control"><i>✓</i>{component.content}</span>;
  if (component.type === 'switch') return <span className="choice-control"><i className="switch-track">●</i>{component.content}</span>;
  if (component.type === 'divider') return null;
  if (component.type === 'list') return <ul className="component-list">{component.content.split('\n').filter(Boolean).map((item, index) => <li key={index}>{item}</li>)}</ul>;
  if (component.type === 'table') return <table className="component-table"><tbody>{component.content.split('\n').filter(Boolean).map((row, rowIndex) => <tr key={rowIndex}>{row.split('|').map((cell, cellIndex) => <td key={cellIndex}>{cell}</td>)}</tr>)}</tbody></table>;
  if (component.type === 'avatar' && /^(data:image\/|https?:\/\/)/.test(component.content)) return <img src={component.content} alt={component.name} draggable={false} />;
  if (component.type === 'section') return null;
  return <span className="component-copy">{component.content}</span>;
}

export function CanvasComponent({ component, resolved, selected, primary, interactive, forcedState, tokens, slotContent, onPointerDown, onResizePointerDown, onPreviewActivate, onEditContents }: {
  component: WebDesignComponent;
  resolved: ResolvedWebDesignComponent;
  selected: boolean;
  primary: boolean;
  interactive: boolean;
  forcedState?: WebComponentVisualState;
  tokens?: WebDesignTokens;
  slotContent?: Record<string, ReactNode>;
  onPointerDown: (event: ReactPointerEvent) => void;
  onResizePointerDown: (event: ReactPointerEvent) => void;
  onPreviewActivate: () => void;
  onEditContents?: () => void;
}) {
  const [hovered, setHovered] = useState(false);
  const [pressed, setPressed] = useState(false);
  const [focused, setFocused] = useState(false);
  const pointerOrigin = useRef<{ x: number; y: number } | null>(null);
  const pointerMoved = useRef(false);
  const runtimeState: WebComponentVisualState | undefined = forcedState ?? (pressed ? 'active' : focused ? 'focus' : hovered ? 'hover' : undefined);
  const effectiveStyle = mergeComponentStyles(resolved.style, runtimeState ? component.states?.[runtimeState] : undefined);
  const style: CSSProperties = {
    left: resolved.x, top: resolved.y, width: resolved.width, height: resolved.height, zIndex: component.zIndex,
    ...(component.library ? componentEffectStyleToCss(effectiveStyle) : componentStyleToCss(effectiveStyle)),
    transition: component.states ? 'background .18s ease, color .18s ease, border-color .18s ease, box-shadow .18s ease, opacity .18s ease, transform .18s ease' : undefined
  };
  return (
    <div data-component-id={component.id} className={`canvas-component type-${component.type} ${component.library ? `library-component library-${component.library.name}` : ''} ${selected ? 'selected' : ''} ${component.locked ? 'locked' : ''} ${interactive ? 'interactive' : ''}`} style={style} tabIndex={interactive && component.states?.focus ? 0 : undefined} onPointerEnter={() => interactive && setHovered(true)} onPointerLeave={() => { setHovered(false); setPressed(false); }} onPointerDown={(event) => { pointerOrigin.current = { x: event.clientX, y: event.clientY }; pointerMoved.current = false; if (interactive) setPressed(true); onPointerDown(event); }} onPointerUp={(event) => { const origin = pointerOrigin.current; pointerMoved.current = Boolean(origin && Math.hypot(event.clientX - origin.x, event.clientY - origin.y) > 4); setPressed(false); }} onPointerCancel={() => { pointerOrigin.current = null; pointerMoved.current = false; setPressed(false); }} onFocus={() => interactive && setFocused(true)} onBlur={() => setFocused(false)} onDoubleClick={(event) => { event.preventDefault(); }} onClick={(event) => { if (!interactive && onEditContents && !pointerMoved.current) { event.stopPropagation(); onEditContents(); } else if (interactive && component.interaction) { event.stopPropagation(); onPreviewActivate(); } pointerOrigin.current = null; pointerMoved.current = false; }}>
      <CanvasComponentContent component={component} style={effectiveStyle} interactive={interactive} tokens={tokens} slotContent={slotContent} />
      {!interactive && component.annotations.some((annotation) => annotation.status === 'open') && <span className="annotation-badge">{component.annotations.filter((annotation) => annotation.status === 'open').length}</span>}
      {primary && !interactive && <><span className="selection-label">{component.locked ? '🔒 ' : ''}{component.name}</span>{onEditContents && <span className="selection-edit-hint">拖动整体 · 点击编辑内部</span>}{!component.locked && <span className="resize-handle" onPointerDown={onResizePointerDown} />}</>}
    </div>
  );
}
