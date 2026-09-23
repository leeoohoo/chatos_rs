import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import type { WebComponentVisualState, WebDesignComponent, WebDesignDevice, WebDesignDocument, WebDesignJsonValue, WebDesignTokens } from '../../src/schema';
import { resolveComponent, type ResolvedWebDesignComponent } from '../../src/editor-model';
import { componentsInSlot, editableSlotsForUiComponent, visibleComponentsInSlot } from '../../src/library-slots';
import type { UiEditableSlot } from '../../src/ui-library';
import { CanvasComponentContent as WorkspaceCanvasComponentContent } from './CanvasComponent';
import { componentEffectStyleToCss, componentStyleToCss, mergeComponentStyles } from './component-style';

export function NumberField({ label, value, onChange, disabled = false, step, min, max }: { label: string; value: number; onChange: (value: number) => void; disabled?: boolean; step?: number; min?: number; max?: number }) {
  return <label className="field-label">{label}<input type="number" disabled={disabled} step={step} min={min} max={max} value={Number.isFinite(value) ? value : 0} onChange={(event) => onChange(Number(event.target.value))} /></label>;
}

export function SceneNumberField({ label, value, onCommit, disabled = false, min, max }: {
  label: string;
  value: number;
  onCommit: (value: number) => void;
  disabled?: boolean;
  min?: number;
  max?: number;
}) {
  return <label className="field-label">{label}<input
    key={`${value}`}
    type="number"
    defaultValue={Number.isFinite(value) ? value : 0}
    disabled={disabled}
    min={min}
    max={max}
    onBlur={(event) => {
      const next = Number(event.currentTarget.value);
      if (!Number.isFinite(next) || next === value) return;
      onCommit(Math.min(max ?? Number.POSITIVE_INFINITY, Math.max(min ?? Number.NEGATIVE_INFINITY, next)));
    }}
    onKeyDown={(event) => {
      if (event.key === 'Enter') event.currentTarget.blur();
    }}
  /></label>;
}

export function ColorValueField({ label, value, onChange, allowComplex = false }: { label: string; value: string; onChange: (value: string) => void; allowComplex?: boolean }) {
  const colorValue = /^#[0-9a-f]{6}$/i.test(value) ? value : '#000000';
  return <label className="field-label color-value-field">{label}<span><input type="color" value={colorValue} onChange={(event) => onChange(event.target.value.toUpperCase())} /><input value={value} onChange={(event) => onChange(event.target.value)} placeholder={allowComplex ? '#FFFFFF 或 linear-gradient(...)' : '#1D1D1F'} /></span></label>;
}

export function AdvancedCssEditor({ value, onChange }: { value: Record<string, string | number>; onChange: (value: Record<string, string | number>) => void }) {
  const serialized = Object.entries(value).map(([property, propertyValue]) => `${property}: ${propertyValue}`).join('\n');
  const [draft, setDraft] = useState(serialized);
  const [error, setError] = useState('');
  useEffect(() => { setDraft(serialized); setError(''); }, [serialized]);
  function apply() {
    try {
      const next: Record<string, string | number> = {};
      for (const rawLine of draft.split('\n')) {
        const line = rawLine.trim().replace(/;$/, '');
        if (!line || line.startsWith('/*') || line.startsWith('//')) continue;
        const separator = line.indexOf(':');
        if (separator <= 0) throw new Error(`缺少冒号：${line}`);
        const property = line.slice(0, separator).trim();
        const propertyValue = line.slice(separator + 1).trim();
        if (!/^(?:--[a-zA-Z0-9_-]{1,80}|[a-zA-Z][a-zA-Z0-9-]{0,80})$/.test(property)) throw new Error(`属性名不正确：${property}`);
        if (!propertyValue) throw new Error(`缺少属性值：${property}`);
        next[property] = /^-?(?:\d+|\d*\.\d+)$/.test(propertyValue) ? Number(propertyValue) : propertyValue;
      }
      onChange(next);
      setError('');
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'CSS 格式不正确');
    }
  }
  const suggestions = [
    ['异形裁切', 'clip-path: polygon(0 0, 100% 0, 92% 100%, 8% 100%)'],
    ['玻璃高光', 'background-image: linear-gradient(135deg, rgba(255,255,255,.28), rgba(255,255,255,0))'],
    ['渐隐遮罩', 'mask-image: linear-gradient(to bottom, black 72%, transparent)'],
    ['立体视角', 'perspective: 1000px']
  ] as const;
  function appendSuggestion(declaration: string) {
    setDraft((current) => current.trim() ? `${current.trim()}\n${declaration}` : declaration);
  }
  return <div className="advanced-css-editor"><div className="advanced-css-suggestions">{suggestions.map(([label, declaration]) => <button key={label} onClick={() => appendSuggestion(declaration)}>{label}</button>)}</div><textarea rows={Math.min(10, Math.max(4, draft.split('\n').length))} value={draft} onChange={(event) => setDraft(event.target.value)} spellCheck={false} placeholder={'clip-path: circle(48%)\ntransform-origin: 50% 100%\n--brand-glow: rgba(99,102,241,.45)'} /><div className="advanced-css-actions"><small>{error || '每行一个 CSS 属性；AI 也可以直接写入这些结构化样式。'}</small><button onClick={apply}>应用样式</button></div></div>;
}

export function JsonPropertyEditor({ label, value, onChange }: { label: string; value: WebDesignJsonValue; onChange: (value: WebDesignJsonValue) => void }) {
  const serialized = JSON.stringify(value, null, 2);
  const [draft, setDraft] = useState(serialized);
  const [error, setError] = useState('');
  useEffect(() => { setDraft(serialized); setError(''); }, [serialized]);
  function apply() {
    try {
      const parsed = JSON.parse(draft) as WebDesignJsonValue;
      onChange(parsed);
      setError('');
    } catch {
      setError('JSON 格式不正确');
    }
  }
  return <div className="json-prop-editor"><div><strong>{label}</strong><button onClick={apply}>应用数据</button></div><textarea rows={Math.min(10, Math.max(4, draft.split('\n').length))} value={draft} onChange={(event) => setDraft(event.target.value)} spellCheck={false} />{error && <small>{error}</small>}</div>;
}

export function JsonObjectEditor({ label, value, onChange, disabled = false }: { label: string; value: Record<string, unknown>; onChange: (value: Record<string, unknown>) => void; disabled?: boolean }) {
  const serialized = JSON.stringify(value, null, 2);
  const [draft, setDraft] = useState(serialized);
  const [error, setError] = useState('');
  useEffect(() => { setDraft(serialized); setError(''); }, [serialized]);
  function apply() {
    try {
      const parsed = JSON.parse(draft) as unknown;
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error('属性必须是 JSON 对象');
      onChange(parsed as Record<string, unknown>);
      setError('');
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'JSON 格式不正确');
    }
  }
  return <div className="json-prop-editor scene-json-object-editor"><div><strong>{label}</strong><button disabled={disabled} onClick={apply}>应用数据</button></div><textarea disabled={disabled} rows={Math.min(14, Math.max(5, draft.split('\n').length))} value={draft} onChange={(event) => setDraft(event.target.value)} spellCheck={false} />{error && <small>{error}</small>}</div>;
}

export function runtimeSlotContentMap(
  document: WebDesignDocument,
  component: WebDesignComponent,
  device: WebDesignDevice,
  interactive: boolean,
  tokens: WebDesignTokens | undefined,
  onPreviewActivate: (component: WebDesignComponent) => void
): Record<string, ReactNode> {
  return Object.fromEntries(editableSlotsForUiComponent(component)
    .filter((slot) => componentsInSlot(document, component.id, slot.id).length > 0)
    .map((slot) => [slot.id,
      <RuntimeSlotContent key={slot.id} document={document} container={component} slot={slot} device={device} interactive={interactive} tokens={tokens} onPreviewActivate={onPreviewActivate} />
    ]));
}

export function RuntimeSlotContent({ document, container, slot, device, interactive, tokens, onPreviewActivate }: {
  document: WebDesignDocument;
  container: WebDesignComponent;
  slot: UiEditableSlot;
  device: WebDesignDevice;
  interactive: boolean;
  tokens?: WebDesignTokens;
  onPreviewActivate: (component: WebDesignComponent) => void;
}) {
  const containerFrame = resolveComponent(container, device);
  const children = visibleComponentsInSlot(document, container.id, slot.id).sort((left, right) => left.zIndex - right.zIndex);
  if (children.length === 0) return null;
  const requiredHeight = Math.max(slot.height, ...children.map((child) => {
    const frame = resolveComponent(child, device);
    return frame.y - containerFrame.y + frame.height + 12;
  }));
  return <div className="runtime-slot-canvas" style={{ minHeight: requiredHeight }}>
    {children.map((child) => {
      const frame = resolveComponent(child, device);
      if (frame.hidden) return null;
      const localFrame = { ...frame, x: frame.x - containerFrame.x, y: frame.y - containerFrame.y };
      return <RuntimeSlotCanvasComponent key={child.id} component={child} frame={localFrame} interactive={interactive} tokens={tokens} slotContent={runtimeSlotContentMap(document, child, device, interactive, tokens, onPreviewActivate)} onPreviewActivate={() => onPreviewActivate(child)} />;
    })}
  </div>;
}

export function RuntimeSlotCanvasComponent({ component, frame, interactive, tokens, slotContent, onPreviewActivate }: {
  component: WebDesignComponent;
  frame: ResolvedWebDesignComponent;
  interactive: boolean;
  tokens?: WebDesignTokens;
  slotContent?: Record<string, ReactNode>;
  onPreviewActivate: () => void;
}) {
  const [hovered, setHovered] = useState(false);
  const [pressed, setPressed] = useState(false);
  const [focused, setFocused] = useState(false);
  const runtimeState: WebComponentVisualState | undefined = pressed ? 'active' : focused ? 'focus' : hovered ? 'hover' : undefined;
  const effectiveStyle = mergeComponentStyles(frame.style, runtimeState ? component.states?.[runtimeState] : undefined);
  const style: CSSProperties = {
    position: 'absolute', left: frame.x, top: frame.y, width: frame.width, height: frame.height, zIndex: component.zIndex,
    ...(component.library ? componentEffectStyleToCss(effectiveStyle) : componentStyleToCss(effectiveStyle)),
    transition: component.states ? 'background .18s ease, color .18s ease, border-color .18s ease, box-shadow .18s ease, opacity .18s ease, transform .18s ease' : undefined
  };
  return <div className={`runtime-slot-component type-${component.type}`} style={style} tabIndex={interactive && component.states?.focus ? 0 : undefined} onPointerEnter={() => interactive && setHovered(true)} onPointerLeave={() => { setHovered(false); setPressed(false); }} onPointerDown={() => interactive && setPressed(true)} onPointerUp={() => setPressed(false)} onPointerCancel={() => setPressed(false)} onFocus={() => interactive && setFocused(true)} onBlur={() => setFocused(false)} onClick={(event) => {
    if (interactive && component.interaction) {
      event.stopPropagation();
      onPreviewActivate();
    }
  }}><WorkspaceCanvasComponentContent component={component} style={effectiveStyle} interactive={interactive} tokens={tokens} slotContent={slotContent} /></div>;
}

export function formatProjectDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '最近更新';
  const today = new Date();
  const sameDay = date.getFullYear() === today.getFullYear()
    && date.getMonth() === today.getMonth()
    && date.getDate() === today.getDate();
  return sameDay
    ? `今天 ${date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}`
    : date.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
}
