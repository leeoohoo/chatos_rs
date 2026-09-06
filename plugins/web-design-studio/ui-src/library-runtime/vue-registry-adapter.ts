import { createApp, h, reactive, ref, shallowRef, type Component } from 'vue';
import { registerVueHostComponents } from './compat/vue-host-components';
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

function declaredPropNames(component: Component) {
  const declared = (component as { props?: string[] | Record<string, unknown> }).props;
  return Array.isArray(declared) ? declared : Object.keys(declared ?? {});
}

function contentPropFor(component: Component) {
  const names = declaredPropNames(component);
  return ['text', 'label'].find((name) => names.includes(name));
}

export function propsForVueComponent(component: Component, source: Record<string, unknown>) {
  return Object.fromEntries(declaredPropNames(component)
    .filter((name) => name in source)
    .map((name) => [name, source[name]]));
}

export function renderPathForVueRegistryEntry(entry: VueRegistryEntry, props: Record<string, unknown> = {}) {
  const requestedDemo = typeof props.registryDemo === 'string' ? props.registryDemo : undefined;
  return entry.demos?.find((demo) => demo.id === requestedDemo)?.path ?? entry.previewPath ?? entry.rootPath;
}

export function createVueRegistryAdapter(options: VueRegistryAdapterOptions): LibraryRuntimeAdapter {
  return {
    library: options.library,
    async mount({ slug, props, content, target, emit }: RuntimeMountOptions): Promise<MountedLibraryComponent> {
      const entry = options.entries[slug];
      if (!entry) throw new Error(`${options.library} component is not synced: ${slug}`);
      const loaded = new Map<string, Component>();
      const runtimePaths = [...new Set([...entry.componentPaths, ...(entry.demos ?? []).map((demo) => demo.path)])];
      await Promise.all(runtimePaths.map(async (path) => loaded.set(path, await loadModule(options.modules, path))));
      const root = loaded.get(entry.rootPath);
      if (!root) throw new Error(`${options.library} root component is missing: ${entry.rootPath}`);

      const state = reactive<Record<string, unknown>>({ ...props });
      const slotContent = ref(content);
      const renderPath = renderPathForVueRegistryEntry(entry, state);
      const renderedComponent = shallowRef(loaded.get(renderPath) ?? root);
      const contentProp = contentPropFor(root);
      if (contentProp && !(contentProp in state)) state[contentProp] = content;
      const app = createApp({
        name: 'LibraryRuntimeHost',
        render: () => h('div', { class: `vue-registry-preview library-${options.library} component-${slug}` }, [
          h(renderedComponent.value, propsForVueComponent(renderedComponent.value, state), {
            default: () => slotContent.value ? h('span', { class: 'runtime-slot-content' }, slotContent.value) : h('span', { class: 'runtime-default-slot' })
          })
        ])
      });
      registerVueHostComponents(app);
      for (const [path, component] of loaded) {
        const name = componentName(path);
        if (name) app.component(name, component);
      }
      app.config.errorHandler = (error) => emit('error', error instanceof Error ? error.message : String(error));
      app.mount(target);
      emit('mounted', { slug });
      return {
        update(next, content) {
          replaceProps(state, next);
          const nextRenderPath = renderPathForVueRegistryEntry(entry, state);
          renderedComponent.value = loaded.get(nextRenderPath) ?? root;
          if (contentProp && !(contentProp in next)) state[contentProp] = content;
          slotContent.value = content;
        },
        destroy() { app.unmount(); }
      };
    }
  };
}
