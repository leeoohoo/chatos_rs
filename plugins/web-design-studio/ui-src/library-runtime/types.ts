import type { App, Component } from 'vue';

export interface RuntimeMountOptions {
  slug: string;
  props: Record<string, unknown>;
  target: HTMLElement;
  emit: (event: string, detail?: unknown) => void;
}

export interface MountedLibraryComponent {
  updateProps(props: Record<string, unknown>): void;
  destroy(): void;
}

export interface LibraryRuntimeAdapter {
  library: string;
  mount(options: RuntimeMountOptions): Promise<MountedLibraryComponent>;
}

export interface VueRegistryEntry {
  slug: string;
  rootPath: string;
  componentPaths: string[];
}

export type VueModule = { default: Component };
export type VueApplication = App<Element>;
