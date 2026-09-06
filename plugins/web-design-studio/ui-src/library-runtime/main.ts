import { inspiraRuntimeAdapter } from './inspira-adapter';
import type { LibraryRuntimeAdapter, MountedLibraryComponent } from './types';
import './runtime.css';

const adapters = new Map<string, LibraryRuntimeAdapter>([
  [inspiraRuntimeAdapter.library, inspiraRuntimeAdapter]
]);
const query = new URLSearchParams(window.location.search);
const library = query.get('library') ?? '';
const slug = query.get('component') ?? '';
const instance = query.get('instance') ?? '';
const target = document.getElementById('root');
let mounted: MountedLibraryComponent | undefined;

function emit(event: string, detail?: unknown) {
  window.parent.postMessage({ source: 'web-design-library-runtime', instance, event, detail }, window.location.origin);
}

function runtimeProps(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

window.addEventListener('message', (message) => {
  if (message.origin !== window.location.origin || message.data?.source !== 'web-design-studio') return;
  if (message.data.instance !== instance) return;
  if (message.data.type === 'props') mounted?.updateProps(runtimeProps(message.data.props));
});

window.addEventListener('beforeunload', () => mounted?.destroy());

async function boot() {
  if (!target) throw new Error('Library runtime root is missing.');
  const adapter = adapters.get(library);
  if (!adapter) throw new Error(`No runtime adapter is registered for ${library}.`);
  mounted = await adapter.mount({ slug, target, props: {}, emit });
  emit('ready', { library, slug });
}

boot().catch((error) => {
  const message = error instanceof Error ? error.message : String(error);
  if (target) target.innerHTML = `<div class="runtime-error"><strong>组件运行失败</strong><span>${message.replace(/[<>&]/g, '')}</span></div>`;
  emit('error', message);
});
