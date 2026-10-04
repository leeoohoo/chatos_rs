import type { ReactNode } from 'react';
import { flattenComponentTree } from '../../src/editor-model';
import { editableSlotsForUiComponent } from '../../src/library-slots';
import { uiLibraryByName, variantsForBoundComponent } from '../../src/ui-libraries';
import type {
  UiComponentDefinition,
  UiComponentVariant,
  UiLibraryCatalog,
} from '../../src/ui-library';
import { indexSceneDocument } from '../../src/v2/scene-schema';
import { officialRuntimePresentation } from '../library-runtime/registry';
import { libraryPreviewSelection } from '../library-runtime/element-selection';
import { inspectorCapabilities as resolveInspectorCapabilities } from './inspector-model';
import { editableSlotsForSceneLibraryNode } from './scene-insertion-target';
import {
  inspectableLibraryProps,
  palette,
} from './WebDesignStudioSupport';

type WebDesignWorkspaceState =
  ReturnType<typeof import('./useWebDesignStudioState').useWebDesignStudioState>;

type WebDesignWorkspaceActions =
  ReturnType<typeof import('./WebDesignCoreActions').createWebDesignCoreActions> &
  ReturnType<typeof import('./WebDesignInsertActions').createWebDesignInsertActions> &
  ReturnType<typeof import('./WebDesignCanvasActions').createWebDesignCanvasActions> &
  ReturnType<typeof import('./WebDesignViewportActions').createWebDesignViewportActions> &
  ReturnType<typeof import('./WebDesignSelectionActions').createWebDesignSelectionActions> &
  ReturnType<typeof import('./WebDesignAssetActions').createWebDesignAssetActions> &
  ReturnType<typeof import('./WebDesignDocumentActions').createWebDesignDocumentActions>;

type WebDesignWorkspaceRenderHelpers =
  ReturnType<typeof import('./WebDesignRenderHelpers').createWebDesignRenderHelpers>;

interface WebDesignWorkspaceComposition
  extends WebDesignWorkspaceState,
    WebDesignWorkspaceActions,
    WebDesignWorkspaceRenderHelpers {
  selectedSceneLibrary: UiLibraryCatalog | undefined;
  selectedSceneLibraryDefinition: UiComponentDefinition | undefined;
  selectedSceneLibraryVariants: readonly UiComponentVariant[];
  newDesignModal: ReactNode;
  deleteDesignModal: ReactNode;
  storageBadge: ReactNode;
}

/**
 * Builds the typed view model consumed by the workspace renderer.
 *
 * State, action groups and render helpers remain separate while they are
 * created. This is the single boundary that flattens them for the renderer,
 * so callers do not have to maintain a second, hand-written list of fields.
 */
export function createWebDesignWorkspaceContext(context: WebDesignWorkspaceComposition) {
  const {
    document,
    pageId,
    sceneDocument,
    selected,
    selectedSceneNode,
    activeScenePage,
    paletteQuery,
    personalSymbols,
    sceneSnippets,
    libraryTab,
    variantPickerTarget,
    selectedSceneLibrary,
  } = context;

  const layerComponents = document ? flattenComponentTree(document, pageId) : [];
  const sceneLayerNodes = sceneDocument
    ? [...indexSceneDocument(sceneDocument).values()]
      .filter((entry) => entry.pageId === pageId)
      .map((entry) => ({ node: entry.node, depth: Math.max(0, entry.path.length - 2) }))
    : [];
  const directChildCount = selected
    ? document?.components.filter((component) => component.parentId === selected.id).length ?? 0
    : 0;
  const canUngroup = Boolean(selected?.id.startsWith('group-') && directChildCount > 0);
  const canUngroupScene = Boolean(
    selectedSceneNode
      && (selectedSceneNode.type === 'group' || selectedSceneNode.type === 'frame')
      && selectedSceneNode.layout.mode === 'free'
  );
  const selectedSceneRegistryElement = selectedSceneNode?.type === 'library-instance'
    ? libraryPreviewSelection(selectedSceneNode.properties.registryElement)
    : undefined;
  const selectedSceneEditableSlots = selectedSceneNode?.type === 'library-instance'
    ? editableSlotsForSceneLibraryNode(selectedSceneNode)
    : [];
  const selectedSymbol = selected?.symbolId
    ? document?.symbols?.find((symbol) => symbol.id === selected.symbolId)
    : undefined;
  const selectedLibrary = uiLibraryByName(selected?.library?.name);
  const selectedLibraryDefinition = selected?.library
    ? selectedLibrary?.components.find((item) => item.id === selected.library?.component)
    : undefined;
  const selectedLibraryVariants = selected?.library ? variantsForBoundComponent(selected) : [];
  const selectedRegistryElement = libraryPreviewSelection(selected?.library?.props.registryElement);
  const selectedEditableSlots = selected ? editableSlotsForUiComponent(selected) : [];
  const inspectorCapabilities = selected
    ? resolveInspectorCapabilities(selected.type, {
      library: Boolean(selected.library),
      directChildCount,
      editableSlotCount: selectedEditableSlots.length,
    })
    : undefined;
  const selectedInspectableLibraryProps = selected?.library
    ? inspectableLibraryProps(selected.library.props)
    : [];
  const aiTarget = selected ?? context.editingContainer;
  const normalizedPaletteQuery = paletteQuery.trim().toLowerCase();
  const filteredPalette = palette.filter((item) => !normalizedPaletteQuery
    || `${item.label} ${item.id} ${item.keywords.join(' ')}`.toLowerCase().includes(normalizedPaletteQuery));
  const filteredPersonalSymbols = personalSymbols.filter((symbol) => !normalizedPaletteQuery
    || `${symbol.name} ${symbol.components.map((component) => component.name).join(' ')}`.toLowerCase().includes(normalizedPaletteQuery));
  const filteredSceneSnippets = sceneSnippets.filter((snippet) => !normalizedPaletteQuery
    || `${snippet.name} ${snippet.nodes.map((node) => node.name).join(' ')}`.toLowerCase().includes(normalizedPaletteQuery));
  const activeUiLibrary = libraryTab !== 'components' && libraryTab !== 'my' && libraryTab !== 'layers'
    ? uiLibraryByName(libraryTab)
    : undefined;
  const filteredUiLibraryComponents = activeUiLibrary?.components.filter((item) => !normalizedPaletteQuery
    || `${item.id} ${item.label} ${item.keywords.join(' ')}`.toLowerCase().includes(normalizedPaletteQuery)) ?? [];
  const variantPickerLibrary = variantPickerTarget
    ? uiLibraryByName(variantPickerTarget.library)
    : undefined;
  const variantPickerDefinition = variantPickerTarget
    ? variantPickerLibrary?.components.find((item) => item.id === variantPickerTarget.componentId)
    : undefined;
  const variantPickerVariants = variantPickerDefinition && variantPickerLibrary
    ? variantPickerLibrary.variants[variantPickerDefinition.id]
      ?? [{ id: 'default', label: '默认款式', props: {} }]
    : [];
  const variantPickerPresentation = variantPickerDefinition && variantPickerLibrary
    ? officialRuntimePresentation(
      variantPickerLibrary.id,
      String(variantPickerDefinition.props?.componentSlug ?? variantPickerDefinition.id)
    )
    : undefined;
  const sceneAiTarget = selectedSceneNode ?? activeScenePage?.children[0];
  const sceneAnnotationTasks = sceneDocument
    ? [...indexSceneDocument(sceneDocument).values()].flatMap((entry) => entry.node.annotations
      .filter((annotation) => annotation.status === 'open')
      .map((annotation) => ({ annotation, node: entry.node, pageId: entry.pageId })))
    : [];
  const aiQuickPrompts = selectedSceneNode
    ? ['让这个组件更精致、更有层次', '优化尺寸、间距和对齐', '给我 3 个更好看的视觉方案']
    : ['设计一个像 Apple 官网一样克制高级的页面', '统一整页的字号、间距、圆角和色彩', '检查并修复页面中不协调的视觉细节'];

  return {
    ...context,
    layerComponents,
    sceneLayerNodes,
    directChildCount,
    canUngroup,
    canUngroupScene,
    selectedSceneRegistryElement,
    selectedSceneEditableSlots,
    selectedSymbol,
    selectedLibrary,
    selectedLibraryDefinition,
    selectedLibraryVariants,
    selectedRegistryElement,
    selectedEditableSlots,
    inspectorCapabilities,
    selectedInspectableLibraryProps,
    aiTarget,
    normalizedPaletteQuery,
    filteredPalette,
    filteredPersonalSymbols,
    filteredSceneSnippets,
    activeUiLibrary,
    filteredUiLibraryComponents,
    variantPickerLibrary,
    variantPickerDefinition,
    variantPickerVariants,
    variantPickerPresentation,
    sceneAiTarget,
    sceneAnnotationTasks,
    aiQuickPrompts,
  };
}

export type WebDesignWorkspaceContext =
  ReturnType<typeof createWebDesignWorkspaceContext>;
