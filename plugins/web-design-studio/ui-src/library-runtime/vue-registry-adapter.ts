import { createApp, h, reactive, type Component } from 'vue';
import type { LibraryRuntimeAdapter, MountedLibraryComponent, RuntimeMountOptions, VueModule, VueRegistryEntry } from './types';

interface VueRegistryAdapterOptions {
  library: string;
  entries: Record<string, VueRegistryEntry>;
  modules: Record<string, () => Promise<unknown>>;
}

function componentName(path: string) {
  return path.split('/').at(-1)?.replace(/\.vue$/, '') ?? '';
}

async function loadModule(modules: VueRegistryAdapterOptions['modules'], path: string): Promise<Component> {
  const loader = modules[path];
  if (!loader) throw new Error(`The synced registry does not contain ${path}. Run npm run sync:inspira.`);
  const module = await loader() as VueModule;
  if (!module.default) throw new Error(`${path} does not export a Vue component.`);
  return module.default;
}

function replaceProps(target: Record<string, unknown>, next: Record<string, unknown>) {
  for (const key of Object.keys(target)) {
    if (!(key in next)) delete target[key];
  }
  Object.assign(target, next);
}

export function createVueRegistryAdapter(options: VueRegistryAdapterOptions): LibraryRuntimeAdapter {
  return {
    library: options.library,
    async mount({ slug, props, target, emit }: RuntimeMountOptions): Promise<MountedLibraryComponent> {
      const entry = options.entries[slug];
      if (!entry) throw new Error(`${options.library} component is not synced: ${slug}`);
      const loaded = new Map<string, Component>();
      await Promise.all(entry.componentPaths.map(async (path) => loaded.set(path, await loadModule(options.modules, path))));
      const root = loaded.get(entry.rootPath);
      if (!root) throw new Error(`${options.library} root component is missing: ${entry.rootPath}`);

      const state = reactive<Record<string, unknown>>({ ...props });
      const app = createApp({
        name: 'LibraryRuntimeHost',
        render: () => h(root, state, {
          default: () => h('div', { class: 'runtime-default-slot' })
        })
      });
      for (const [path, component] of loaded) {
        const name = componentName(path);
        if (name) app.component(name, component);
      }
      app.config.errorHandler = (error) => emit('error', error instanceof Error ? error.message : String(error));
      app.mount(target);
      emit('mounted', { slug });
      return {
        updateProps(next) { replaceProps(state, next); },
        destroy() { app.unmount(); }
      };
    }
  };
}
