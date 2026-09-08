import type { LibraryRuntimeAdapter, MountedLibraryComponent } from './types';
import { applyPreviewElementContent, isolatePreviewElement, libraryPreviewSelection, resolvePreviewElement, type LibraryPreviewPointerEvent } from './element-selection';
import { installRuntimePreviewPicker } from './preview-picker';
import './runtime.css';

const adapterLoaders: Record<string, () => Promise<LibraryRuntimeAdapter>> = {
  antd: () => import('./antd-adapter').then((module) => module.antdRuntimeAdapter),
  chakra: () => import('./chakra-adapter').then((module) => module.chakraRuntimeAdapter),
  inspira: () => import('./inspira-adapter').then((module) => module.inspiraRuntimeAdapter),
  magicui: () => import('./magicui-adapter').then((module) => module.magicUiRuntimeAdapter),
  shadcn: () => import('./shadcn-adapter').then((module) => module.shadcnRuntimeAdapter),
  spell: () => import('./spell-adapter').then((module) => module.spellRuntimeAdapter),
  daisyui: () => import('./daisyui-adapter').then((module) => module.daisyUiRuntimeAdapter)
};
const query = new URLSearchParams(window.location.search);
const library = query.get('library') ?? '';
const slug = query.get('component') ?? '';
const instance = query.get('instance') ?? '';
const pickerEnabled = query.get('picker') === '1';
document.documentElement.dataset.runtimeLayout = query.get('layout') === 'intrinsic' ? 'intrinsic' : 'fill';
const target = document.getElementById('root');
let mounted: MountedLibraryComponent | undefined;
let booting: Promise<void> | undefined;
let sizeObserver: ResizeObserver | undefined;
let sizeFrame = 0;
let selectionFrame = 0;
let selectionCleanup: (() => void) | undefined;
let pickerCleanup: (() => void) | undefined;

function emit(event: string, detail?: unknown) {
  window.parent.postMessage({ source: 'web-design-library-runtime', instance, event, detail }, window.location.origin);
}

function emitPreviewPointerEvent(state: LibraryPreviewPointerEvent) {
  emit('preview-pointer', state);
}

function reportContentSize() {
  cancelAnimationFrame(sizeFrame);
  sizeFrame = requestAnimationFrame(() => {
    const rootBounds = target?.getBoundingClientRect();
    emit('content-size', {
      width: Math.ceil(Math.max(document.documentElement.scrollWidth, document.body.scrollWidth, target?.scrollWidth ?? 0, rootBounds?.width ?? 0)),
      height: Math.ceil(Math.max(document.documentElement.scrollHeight, document.body.scrollHeight, target?.scrollHeight ?? 0, rootBounds?.height ?? 0))
    });
  });
}

function observeContentSize() {
  sizeObserver?.disconnect();
  if (!target || typeof ResizeObserver === 'undefined') return;
  sizeObserver = new ResizeObserver(reportContentSize);
  sizeObserver.observe(document.documentElement);
  sizeObserver.observe(document.body);
  sizeObserver.observe(target);
  reportContentSize();
}

function runtimeProps(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

function applySelectedElement(props: Record<string, unknown>, content: string) {
  cancelAnimationFrame(selectionFrame);
  selectionCleanup?.();
  selectionCleanup = undefined;
  const selection = libraryPreviewSelection(props.registryElement);
  if (!selection || !target) return;
  selectionFrame = requestAnimationFrame(() => {
    selectionFrame = requestAnimationFrame(() => {
      const element = resolvePreviewElement(target, selection.path);
      if (!element) return;
      selectionCleanup = isolatePreviewElement(target, element);
      applyPreviewElementContent(element, content);
      reportContentSize();
    });
  });
}

window.addEventListener('message', (message) => {
  if (message.origin !== window.location.origin || message.data?.source !== 'web-design-studio') return;
  if (message.data.instance !== instance) return;
  if (message.data.type === 'props') {
    const props = runtimeProps(message.data.props);
    const content = typeof message.data.content === 'string' ? message.data.content : '';
    if (mounted) {
      mounted.update(props, content);
      applySelectedElement(props, content);
    }
    else void boot(props, content);
  }
});

window.addEventListener('beforeunload', () => {
  cancelAnimationFrame(sizeFrame);
  cancelAnimationFrame(selectionFrame);
  selectionCleanup?.();
  pickerCleanup?.();
  sizeObserver?.disconnect();
  mounted?.destroy();
});

async function mount(props: Record<string, unknown>, content: string) {
  if (!target) throw new Error('Library runtime root is missing.');
  const loadAdapter = adapterLoaders[library];
  if (!loadAdapter) throw new Error(`No runtime adapter is registered for ${library}.`);
  const adapter = await loadAdapter();
  mounted = await adapter.mount({ slug, target, props, content, emit });
  if (pickerEnabled) {
    pickerCleanup?.();
    pickerCleanup = installRuntimePreviewPicker(
      target,
      slug,
      emitPreviewPointerEvent,
      (selection) => emit('preview-select', selection)
    );
  }
  applySelectedElement(props, content);
  observeContentSize();
  emit('ready', { library, slug });
}

function boot(props: Record<string, unknown>, content: string) {
  if (!booting) {
    booting = mount(props, content).catch((error) => {
      const message = error instanceof Error ? error.message : String(error);
      if (target) target.innerHTML = `<div class="runtime-error"><strong>组件运行失败</strong><span>${message.replace(/[<>&]/g, '')}</span></div>`;
      emit('error', message);
    });
  }
  return booting;
}

emit('request-props', { library, slug });
