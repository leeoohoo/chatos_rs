import { ANTD_LIBRARY } from './antd-library.js';
import { CHAKRA_LIBRARY } from './chakra-library.js';
import { SHADCN_LIBRARY } from './shadcn-library.js';
import { applyUiComponentVariant, createUiLibraryComponent, variantsForUiComponent, type UiLibraryCatalog } from './ui-library.js';
import type { WebDesignComponent, WebDesignLibraryName } from './schema.js';

export const UI_LIBRARIES: readonly UiLibraryCatalog[] = [ANTD_LIBRARY, CHAKRA_LIBRARY, SHADCN_LIBRARY];

export function uiLibraryByName(name: WebDesignLibraryName | undefined): UiLibraryCatalog | undefined {
  return UI_LIBRARIES.find((library) => library.id === name);
}

export function createComponentFromUiLibrary(libraryName: WebDesignLibraryName, definitionId: string, x: number, y: number): WebDesignComponent {
  const library = uiLibraryByName(libraryName);
  if (!library) throw new Error(`UI library not found: ${libraryName}`);
  return createUiLibraryComponent(library, definitionId, x, y);
}

export function applyUiLibraryVariant(component: WebDesignComponent, variantId: string): WebDesignComponent {
  const library = uiLibraryByName(component.library?.name);
  return library ? applyUiComponentVariant(library, component, variantId) : component;
}

export function variantsForBoundComponent(component: WebDesignComponent) {
  const library = uiLibraryByName(component.library?.name);
  return library ? variantsForUiComponent(library, component.library?.component ?? '') : [];
}
