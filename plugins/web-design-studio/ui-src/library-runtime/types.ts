import type { App, Component } from 'vue';

export interface RuntimeMountOptions {
  slug: string;
  props: Record<string, unknown>;
  content: string;
  target: HTMLElement;
  emit: (event: string, detail?: unknown) => void;
}

export interface MountedLibraryComponent {
  update(props: Record<string, unknown>, content: string): void;
  destroy(): void;
}

export interface LibraryRuntimeAdapter {
  library: string;
  mount(options: RuntimeMountOptions): Promise<MountedLibraryComponent>;
}

export interface VueRegistryEntry {
  slug: string;
  section?: string;
  rootPath: string;
  previewPath?: string;
  demos?: Array<{ id: string; label: string; path: string }>;
  componentPaths: string[];
}

export interface ReactRegistryEntry {
  slug: string;
  title: string;
  description: string;
  rootPath: string;
  rootExport?: string;
  previewPath: string;
  previewExport?: string;
  demo?: string;
  demos?: Array<{
    id: string;
    label: string;
    path?: string;
    export?: string;
    type?: string;
    composition?: ReactRegistryCompositionNode;
  }>;
  acceptsChildren?: boolean;
  layout?: 'fill' | 'intrinsic';
  previewHeight?: number;
  previewSpan?: 'normal' | 'wide';
  docsUrl: string;
}

export interface DomRegistryDemo {
  id: string;
  label: string;
  html?: string;
}

export interface DomRegistryEntry {
  slug: string;
  title: string;
  description: string;
  demos: readonly DomRegistryDemo[];
  layout?: 'fill' | 'intrinsic';
  previewHeight?: number;
  previewSpan?: 'normal' | 'wide';
  docsUrl: string;
}

export type ReactRegistryCompositionValue =
  | string
  | number
  | boolean
  | null
  | ReactRegistryCompositionValue[]
  | { [key: string]: ReactRegistryCompositionValue }
  | { $ref: 'content'; fallback?: string }
  | { $ref: 'prop'; name: string; fallback?: ReactRegistryCompositionValue };

export interface ReactRegistryCompositionNode {
  element?: string;
  module?: string;
  export?: string;
  props?: Record<string, ReactRegistryCompositionValue>;
  children?: Array<ReactRegistryCompositionNode | ReactRegistryCompositionValue>;
}

export type ReactModule = Record<string, unknown> & { default?: unknown };

export type VueModule = { default: Component };
export type VueApplication = App<Element>;
