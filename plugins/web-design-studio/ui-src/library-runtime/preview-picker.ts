import type { LibraryPreviewPointerEvent, LibraryPreviewSelection } from './element-selection';

const SEMANTIC_SELECTOR = [
  'button', 'a[href]', 'input', 'textarea', 'select', 'summary',
  '[role="button"]', '[role="checkbox"]', '[role="radio"]', '[role="switch"]', '[role="tab"]',
  'img', 'video', 'canvas'
].join(',');

const DAISY_CLASS_BY_SLUG: Record<string, string[]> = {
  button: ['btn'],
  checkbox: ['checkbox'],
  'file-input': ['file-input'],
  input: ['input'],
  radio: ['radio'],
  range: ['range'],
  select: ['select'],
  textarea: ['textarea'],
  toggle: ['toggle']
};

function normalized(value: string) {
  return value.trim().toLowerCase().replace(/_/g, '-');
}

function visible(element: HTMLElement) {
  const style = getComputedStyle(element);
  if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0) return false;
  const bounds = element.getBoundingClientRect();
  return bounds.width >= 6 && bounds.height >= 6;
}

function frameworkRoot(element: HTMLElement, slug: string) {
  const normalizedSlug = normalized(slug);
  const classNames = [...element.classList].map(normalized);
  if (classNames.includes(`ant-${normalizedSlug}`)) return true;
  if (classNames.includes(normalizedSlug)) return true;
  if ((DAISY_CLASS_BY_SLUG[normalizedSlug] ?? []).some((name) => classNames.includes(name))) return true;
  const scope = normalized(element.dataset.scope ?? '');
  const part = normalized(element.dataset.part ?? '');
  if (scope === normalizedSlug && (!part || ['root', 'control', 'item'].includes(part))) return true;
  const slot = normalized(element.dataset.slot ?? '');
  return slot === normalizedSlug || slot === `${normalizedSlug}-root`;
}

function previewPath(element: HTMLElement, root: HTMLElement) {
  const indexes: number[] = [];
  let current: HTMLElement | null = element;
  while (current && current !== root) {
    const parent: HTMLElement | null = current.parentElement;
    if (!parent) break;
    indexes.unshift([...parent.children].indexOf(current));
    current = parent;
  }
  return indexes.join('.');
}

function previewLabel(element: HTMLElement, index: number) {
  const text = element.getAttribute('aria-label')
    || element.getAttribute('title')
    || element.getAttribute('placeholder')
    || element.innerText?.replace(/\s+/g, ' ').trim()
    || element.getAttribute('alt')
    || element.getAttribute('value')
    || '';
  const tag = element.tagName.toLowerCase();
  const kind = tag === 'a' ? '链接'
    : tag === 'button' || element.getAttribute('role') === 'button' ? '按钮'
      : ['input', 'textarea', 'select'].includes(tag) ? '输入项'
        : '组件';
  return text ? text.slice(0, 42) : `${kind} ${index + 1}`;
}

function fallbackRoots(root: HTMLElement) {
  let level = [...root.children].filter((element): element is HTMLElement => element instanceof HTMLElement && visible(element));
  for (let depth = 0; depth < 5 && level.length === 1; depth += 1) {
    const children = [...level[0].children].filter((element): element is HTMLElement => element instanceof HTMLElement && visible(element));
    if (children.length === 0) break;
    level = children;
  }
  return level;
}

function collectPreviewItems(root: HTMLElement, slug: string): Array<{ element: HTMLElement; selection: LibraryPreviewSelection }> {
  const all = [...root.querySelectorAll<HTMLElement>('*')].filter(visible);
  const exact = all.filter((element) => frameworkRoot(element, slug));
  const semantic = all.filter((element) => element.matches(SEMANTIC_SELECTOR)
    && !element.parentElement?.closest(SEMANTIC_SELECTOR));
  const exactRoots = exact.filter((element) => !exact.some((candidate) => candidate !== element && candidate.contains(element)));
  const candidates = [...new Set([...exactRoots, ...semantic])];
  const chosen = candidates.length > 0 ? candidates : fallbackRoots(root);
  const viewportWidth = Math.max(1, document.documentElement.clientWidth);
  const viewportHeight = Math.max(1, document.documentElement.clientHeight);
  return chosen.slice(0, 240).map((element, index) => {
    const bounds = element.getBoundingClientRect();
    return {
      element,
      selection: {
        path: previewPath(element, root),
        label: previewLabel(element, index),
        x: Math.max(0, bounds.left),
        y: Math.max(0, bounds.top),
        width: Math.max(6, bounds.width),
        height: Math.max(6, bounds.height),
        viewportWidth,
        viewportHeight
      }
    };
  });
}

export function installRuntimePreviewPicker(
  root: HTMLElement,
  slug: string,
  emitPointerEvent: (state: LibraryPreviewPointerEvent) => void,
  emitSelect: (selection: LibraryPreviewSelection) => void
) {
  const style = document.createElement('style');
  style.dataset.studioPreviewPicker = 'true';
  style.textContent = `
    [data-studio-pickable] { cursor: grab !important; outline: 1px solid transparent; outline-offset: 3px; transition: outline-color .12s ease, box-shadow .12s ease; touch-action: none; -webkit-user-drag: none; }
    [data-studio-pickable][data-studio-pick-active] { outline: 2px solid #007aff !important; box-shadow: 0 0 0 5px rgba(0,122,255,.12) !important; }
    [data-studio-pickable]:active { cursor: grabbing !important; }
  `;
  document.head.append(style);
  const marked = new Map<HTMLElement, string | null>();
  let activeElement: HTMLElement | undefined;
  let refreshTimer = 0;
  let suppressPointerClickUntil = 0;
  let activePointer: { source: HTMLElement; selection: LibraryPreviewSelection; pointerId: number } | undefined;

  const refresh = () => {
    window.clearTimeout(refreshTimer);
    refreshTimer = window.setTimeout(() => {
      for (const [element, draggable] of marked) {
        element.removeAttribute('data-studio-pickable');
        if (draggable === null) element.removeAttribute('draggable');
        else element.setAttribute('draggable', draggable);
      }
      marked.clear();
      for (const { element, selection } of collectPreviewItems(root, slug)) {
        marked.set(element, element.getAttribute('draggable'));
        element.dataset.studioPickable = JSON.stringify(selection);
        element.draggable = false;
      }
    }, 40);
  };
  const sourceForEvent = (event: Event) => {
    const target = event.target as (EventTarget & { closest?: (selector: string) => HTMLElement | null }) | null;
    return typeof target?.closest === 'function' ? target.closest('[data-studio-pickable]') ?? undefined : undefined;
  };
  const selectionForSource = (source?: HTMLElement) => {
    if (!source?.dataset.studioPickable) return;
    try { return JSON.parse(source.dataset.studioPickable) as LibraryPreviewSelection; } catch { return undefined; }
  };
  const pointerdown = (event: PointerEvent) => {
    if (event.button !== 0) return;
    const selection = selectionForSource(sourceForEvent(event));
    if (!selection) return;
    suppressPointerClickUntil = performance.now() + 1000;
    event.preventDefault();
    event.stopImmediatePropagation();
    const source = sourceForEvent(event)!;
    activePointer = { source, selection, pointerId: event.pointerId };
    source.setPointerCapture?.(event.pointerId);
    emitPointerEvent({ selection, pointerId: event.pointerId, clientX: event.clientX, clientY: event.clientY, phase: 'start' });
  };
  const pointermove = (event: PointerEvent) => {
    if (!activePointer || event.pointerId !== activePointer.pointerId) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    emitPointerEvent({ selection: activePointer.selection, pointerId: event.pointerId, clientX: event.clientX, clientY: event.clientY, phase: 'move' });
  };
  const finishPointer = (event: PointerEvent, cancelled: boolean) => {
    if (!activePointer || event.pointerId !== activePointer.pointerId) return;
    const active = activePointer;
    activePointer = undefined;
    if (active.source.hasPointerCapture?.(event.pointerId)) active.source.releasePointerCapture(event.pointerId);
    event.preventDefault();
    event.stopImmediatePropagation();
    emitPointerEvent({ selection: active.selection, pointerId: event.pointerId, clientX: event.clientX, clientY: event.clientY, phase: cancelled ? 'cancel' : 'end' });
  };
  const pointerup = (event: PointerEvent) => finishPointer(event, false);
  const pointercancel = (event: PointerEvent) => finishPointer(event, true);
  const click = (event: MouseEvent) => {
    const selection = selectionForSource(sourceForEvent(event));
    if (!selection) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    if (event.detail === 0 || performance.now() >= suppressPointerClickUntil) emitSelect(selection);
  };
  const hover = (event: MouseEvent) => {
    const source = sourceForEvent(event);
    if (source === activeElement) return;
    activeElement?.removeAttribute('data-studio-pick-active');
    activeElement = source;
    activeElement?.setAttribute('data-studio-pick-active', 'true');
  };
  const leave = (event: MouseEvent) => {
    const related = event.relatedTarget as (EventTarget & { closest?: (selector: string) => HTMLElement | null }) | null;
    if (typeof related?.closest === 'function' && related.closest('[data-studio-pickable]')) return;
    activeElement?.removeAttribute('data-studio-pick-active');
    activeElement = undefined;
  };
  const dragstart = (event: DragEvent) => {
    event.preventDefault();
    event.stopImmediatePropagation();
  };

  document.addEventListener('pointerdown', pointerdown, true);
  document.addEventListener('pointermove', pointermove, true);
  document.addEventListener('pointerup', pointerup, true);
  document.addEventListener('pointercancel', pointercancel, true);
  document.addEventListener('click', click, true);
  document.addEventListener('mouseover', hover, true);
  document.addEventListener('mouseout', leave, true);
  document.addEventListener('dragstart', dragstart, true);
  const observer = new MutationObserver(refresh);
  observer.observe(root, { childList: true, subtree: true });
  refresh();

  return () => {
    window.clearTimeout(refreshTimer);
    observer.disconnect();
    document.removeEventListener('pointerdown', pointerdown, true);
    document.removeEventListener('pointermove', pointermove, true);
    document.removeEventListener('pointerup', pointerup, true);
    document.removeEventListener('pointercancel', pointercancel, true);
    document.removeEventListener('click', click, true);
    document.removeEventListener('mouseover', hover, true);
    document.removeEventListener('mouseout', leave, true);
    document.removeEventListener('dragstart', dragstart, true);
    activeElement?.removeAttribute('data-studio-pick-active');
    for (const [element, draggable] of marked) {
      element.removeAttribute('data-studio-pickable');
      if (draggable === null) element.removeAttribute('draggable');
      else element.setAttribute('draggable', draggable);
    }
    style.remove();
  };
}
