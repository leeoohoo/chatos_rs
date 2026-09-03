import { useLayoutEffect, useState, type CSSProperties, type ReactNode, type RefObject } from 'react';
import { createPortal } from 'react-dom';
import type { WebDesignJsonValue } from '../../src/schema';

export type RuntimeRecord = Record<string, WebDesignJsonValue>;

export function runtimeRecords(value: WebDesignJsonValue | undefined): RuntimeRecord[] {
  return Array.isArray(value)
    ? value.filter((item): item is RuntimeRecord => Boolean(item) && typeof item === 'object' && !Array.isArray(item))
    : [];
}

export function runtimeStrings(value: WebDesignJsonValue | undefined): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

export function runtimeRows(value: WebDesignJsonValue | undefined): string[][] {
  return Array.isArray(value)
    ? value.filter((row): row is WebDesignJsonValue[] => Array.isArray(row)).map((row) => row.map(String))
    : [];
}

function portalHostFor(anchor: HTMLElement | null): HTMLElement | null {
  return anchor?.closest('[data-library-portal-host], .design-canvas') as HTMLElement | null;
}

function portalThemeFor(anchor: HTMLElement | null): CSSProperties {
  const runtime = anchor?.closest('.shadcn-runtime, .chakra-runtime') as HTMLElement | null;
  if (!runtime) return {};
  const computed = window.getComputedStyle(runtime);
  const style: Record<string, string> = {
    fontFamily: computed.fontFamily,
    fontSize: computed.fontSize
  };
  for (let index = 0; index < computed.length; index += 1) {
    const property = computed.item(index);
    if (!property.startsWith('--')) continue;
    const value = computed.getPropertyValue(property).trim();
    if (value) style[property] = value;
  }
  return style as CSSProperties;
}

export function DesignOverlay({ anchorRef, open, side = 'center', title, className, children, onClose, footer }: {
  anchorRef: RefObject<HTMLElement | null>;
  open: boolean;
  side?: string;
  title: string;
  className: string;
  children: ReactNode;
  onClose: () => void;
  footer?: ReactNode;
}) {
  const target = portalHostFor(anchorRef.current);
  if (!open || !target) return null;
  const normalizedSide = side === 'end' ? 'right' : side === 'start' ? 'left' : side;
  return createPortal(<div className={`library-overlay-host ${className}`} style={portalThemeFor(anchorRef.current)} onPointerDown={(event) => event.stopPropagation()}>
    <button className="library-overlay-scrim" aria-label="关闭" onClick={onClose} />
    <section className={`library-overlay-panel side-${normalizedSide}`}>
      <header><strong>{title}</strong><button aria-label="关闭" onClick={onClose}>×</button></header>
      <div className="library-overlay-body">{children}</div>
      {footer && <footer>{footer}</footer>}
    </section>
  </div>, target);
}

export function FloatingSurface({ anchorRef, open, className, children, placement = 'bottom' }: {
  anchorRef: RefObject<HTMLElement | null>;
  open: boolean;
  className: string;
  children: ReactNode;
  placement?: string;
}) {
  const [style, setStyle] = useState<CSSProperties>({});
  const target = portalHostFor(anchorRef.current);
  useLayoutEffect(() => {
    if (!open || !target || !anchorRef.current) return;
    const anchor = anchorRef.current.getBoundingClientRect();
    const canvas = target.getBoundingClientRect();
    const anchorLeft = anchor.left - canvas.left;
    const anchorTop = anchor.top - canvas.top;
    if (placement === 'top') {
      setStyle({ left: Math.max(12, Math.min(anchorLeft, canvas.width - 340)), top: Math.max(12, anchorTop - 8), transform: 'translateY(-100%)' });
    } else if (placement === 'left') {
      setStyle({ left: Math.max(12, anchorLeft - 8), top: Math.max(12, anchorTop), transform: 'translateX(-100%)' });
    } else if (placement === 'right') {
      setStyle({ left: Math.min(canvas.width - 340, anchor.right - canvas.left + 8), top: Math.max(12, anchorTop) });
    } else {
      setStyle({ left: Math.max(12, Math.min(anchorLeft, canvas.width - 340)), top: Math.max(12, anchor.bottom - canvas.top + 8) });
    }
  }, [open, target, anchorRef, placement]);
  if (!open || !target) return null;
  return createPortal(<div className={`library-floating-surface ${className}`} style={{ ...portalThemeFor(anchorRef.current), ...style }} onPointerDown={(event) => event.stopPropagation()}>{children}</div>, target);
}

export function MiniCalendar({ month = '2026 年 9 月', selectedDay = 3, className = '' }: { month?: string; selectedDay?: number; className?: string }) {
  const days = ['一', '二', '三', '四', '五', '六', '日'];
  return <div className={`mini-calendar ${className}`}><header><button>‹</button><strong>{month}</strong><button>›</button></header><div className="mini-calendar-grid">{days.map((day) => <small key={day}>{day}</small>)}{Array.from({ length: 2 }, (_, index) => <i key={`blank-${index}`} />)}{Array.from({ length: 30 }, (_, index) => index + 1).map((day) => <button key={day} className={day === selectedDay ? 'selected' : ''}>{day}</button>)}</div></div>;
}

export function SimpleDataTable({ columns, rows, className = '', striped = false }: { columns: string[]; rows: string[][]; className?: string; striped?: boolean }) {
  return <div className={`library-data-table ${className} ${striped ? 'striped' : ''}`}><table><thead><tr>{columns.map((column) => <th key={column}>{column}</th>)}</tr></thead><tbody>{rows.map((row, rowIndex) => <tr key={rowIndex}>{row.map((cell, cellIndex) => <td key={cellIndex}>{cell}</td>)}</tr>)}</tbody></table></div>;
}

export function SkeletonComposition({ kind = 'text', lines = 3, className = '' }: { kind?: string; lines?: number; className?: string }) {
  return <div className={`library-skeleton ${className} kind-${kind}`}>{(kind === 'avatar' || kind === 'profile') && <i className="skeleton-avatar" />}{kind === 'card' && <i className="skeleton-media" />}<div>{Array.from({ length: lines }, (_, index) => <i key={index} style={{ width: index === lines - 1 ? '68%' : '100%' }} />)}</div></div>;
}
