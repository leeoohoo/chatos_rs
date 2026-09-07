export interface LibraryPreviewSelection {
  path: string;
  label: string;
  x: number;
  y: number;
  width: number;
  height: number;
  viewportWidth: number;
  viewportHeight: number;
}

export interface LibraryPreviewPointerEvent {
  selection: LibraryPreviewSelection;
  clientX: number;
  clientY: number;
  pointerId: number;
  phase: 'start' | 'move' | 'end' | 'cancel';
}

export const LIBRARY_PREVIEW_POINTER_EVENT = 'web-design-library-preview-pointer';

export function libraryPreviewSelection(value: unknown): LibraryPreviewSelection | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return;
  const candidate = value as Partial<LibraryPreviewSelection>;
  const numbers = [candidate.x, candidate.y, candidate.width, candidate.height, candidate.viewportWidth, candidate.viewportHeight];
  if (typeof candidate.path !== 'string' || typeof candidate.label !== 'string' || numbers.some((item) => typeof item !== 'number' || !Number.isFinite(item))) return;
  if ((candidate.width ?? 0) <= 0 || (candidate.height ?? 0) <= 0 || (candidate.viewportWidth ?? 0) <= 0 || (candidate.viewportHeight ?? 0) <= 0) return;
  return candidate as LibraryPreviewSelection;
}

export function resolvePreviewElement(root: HTMLElement, path: string): HTMLElement | undefined {
  if (!path) return root;
  let current: Element = root;
  for (const segment of path.split('.')) {
    if (!/^\d+$/.test(segment)) return;
    const child = current.children.item(Number(segment));
    if (!child || child.nodeType !== 1) return;
    current = child;
  }
  return current as HTMLElement;
}

export function isolatePreviewElement(root: HTMLElement, element: HTMLElement): () => void {
  const restored: Array<{ element: HTMLElement; ariaHidden: string | null }> = [];
  let current: HTMLElement | null = element;
  while (current && current !== root) {
    const parent: HTMLElement | null = current.parentElement;
    if (!parent) break;
    for (const sibling of Array.from(parent.children) as Element[]) {
      if (sibling === current || sibling.nodeType !== 1) continue;
      const siblingElement = sibling as HTMLElement;
      restored.push({ element: siblingElement, ariaHidden: siblingElement.getAttribute('aria-hidden') });
      siblingElement.setAttribute('aria-hidden', 'true');
    }
    current = parent;
  }
  return () => {
    for (const { element: sibling, ariaHidden } of restored) {
      if (ariaHidden === null) sibling.removeAttribute('aria-hidden');
      else sibling.setAttribute('aria-hidden', ariaHidden);
    }
  };
}

function firstEditableTextNode(element: HTMLElement): Text | undefined {
  const ownerWindow = element.ownerDocument.defaultView;
  const showText = ownerWindow?.NodeFilter.SHOW_TEXT ?? 4;
  const walker = element.ownerDocument.createTreeWalker(element, showText);
  let node = walker.nextNode();
  while (node) {
    const parentName = node.parentElement?.tagName.toLowerCase();
    if (node.textContent?.trim() && parentName !== 'script' && parentName !== 'style') return node as Text;
    node = walker.nextNode();
  }
  return undefined;
}

export function applyPreviewElementContent(element: HTMLElement, content: string) {
  if (!content) return;
  const tag = element.tagName.toLowerCase();
  if (tag === 'input' || tag === 'textarea') {
    const field = element as HTMLInputElement | HTMLTextAreaElement;
    field.value = content;
    field.setAttribute('value', content);
    if (field.hasAttribute('placeholder')) field.setAttribute('placeholder', content);
    return;
  }
  if (tag === 'select') return;
  const textNode = firstEditableTextNode(element);
  if (textNode) textNode.textContent = content;
}
