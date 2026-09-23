import {
  useEffect, useMemo, useReducer, useRef, useState
} from 'react';
import {
  breakpointFor, constrainComponentFrame, componentsForPage, descendantIds, fitContentCanvasToComponents,
  moveComponentsWithDescendants, resolveComponent, selectedRootIds, setSymbolOverride, snapComponentFrame,
  updateComponentFrame, type SnapGuides
} from '../../src/editor-model';
import { componentsInSlot, editableSlotsForUiComponent, slotIdForDescendant, visibleComponentsInSlot } from '../../src/library-slots';
import {
  matchArtboardSizePreset, viewportDimensions, viewportPresetsForDevice, WEB_DESIGN_ARTBOARD_SIZE_PRESETS, type WebDesignViewportOrientation
} from '../../src/viewport-presets';
import {
  pagesForDocument, tokensForDocument, type WebDesignDevice, type WebDesignDocument, type WebDesignProject,
  type WebDesignSymbol
} from '../../src/schema';
import {
  createRepository, type DesignRepository, type DesignSummary, type GenerationPlanSummary, type GenerationStepReview,
  type SceneAnnotationAiContext
} from './repository';
import { mergeComponentStyles } from './component-style';
import { DEFAULT_WORKSPACE_SHELL, parseWorkspaceShellState, workspaceShellReducer, workspaceShellShortcut } from './workspace-shell-model';
import { initialWorkspaceArtboards, reconcileWorkspaceArtboards } from './workspace-artboard-model';
import {
  fitWorkspaceRect, panWorkspaceCamera, workspaceZoomFromWheel, zoomWorkspaceCameraAt, type WorkspaceCamera
} from '../../src/v2/workspace-camera';
import type { WorkspacePlacementDocument, WorkspaceSurfaceKind } from '../../src/v2/workspace-placement-store';
import { indexSceneDocument, type SceneDocument, type SceneNode } from '../../src/v2/scene-schema';
import type { SceneHistoryStatus } from '../../src/v2/scene-store';
import { sceneArtboardContentHeight } from './SceneArtboardCanvas';
import { parseSceneSnippets, type SceneSnippet } from './scene-snippet-library';
import { normalizedSelectionRect, selectionNodesInRect, type EditorSelectionRect } from './selection-model';
import {
  SCENE_RESPONSIVE_EDITOR_RULES, slotEditorFrameBounds, ViewportSelection, studioLocationSelection, replaceStudioLocation,
  DEFAULT_VIEWPORT_SELECTIONS, Interaction, CanvasPan, CanvasMarquee, LibraryTab,
  VariantPickerTarget, VariantPickerPointerDrag, EditingSlot, SceneContentFocus, SelectionCandidatePopover,
  InspectorVisualState, InspectorTab, PERSONAL_SYMBOLS_STORAGE_KEY, SCENE_SNIPPETS_STORAGE_KEY, deviceForWorkspaceArtboard,
  workspaceArtboardContentBounds, workspaceArtboardSignature, loadPersonalSymbols
} from './WebDesignStudioSupport';

export function useWebDesignStudioState() {
  const actionsRef = useRef<Record<string, any>>({});
  const [repository, setRepository] = useState<DesignRepository>();
  const [documents, setDocuments] = useState<DesignSummary[]>([]);
  const [activeProject, setActiveProject] = useState<WebDesignProject>();
  const [document, setDocument] = useState<WebDesignDocument>();
  const [sceneDocument, setSceneDocument] = useState<SceneDocument>();
  const [sceneHistory, setSceneHistory] = useState<SceneHistoryStatus>();
  const [sceneLoadState, setSceneLoadState] = useState<'idle' | 'loading' | 'ready' | 'missing'>('idle');
  const [sceneReloadToken, setSceneReloadToken] = useState(0);
  const [ready, setReady] = useState(false);
  const [screen, setScreen] = useState<'project' | 'editor'>('project');
  const [persistedRevision, setPersistedRevision] = useState(0);
  const [selectedId, setSelectedId] = useState<string>();
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [selectionCandidatePopover, setSelectionCandidatePopover] = useState<SelectionCandidatePopover>();
  const [marqueeRect, setMarqueeRect] = useState<EditorSelectionRect>();
  const [pageId, setPageId] = useState('home');
  const [clipboard, setClipboard] = useState<{ document: WebDesignDocument; componentIds: string[] }>();
  const [sceneClipboard, setSceneClipboard] = useState<SceneNode[]>([]);
  const [snapGuides, setSnapGuides] = useState<SnapGuides>({});
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [previewOverlayPageId, setPreviewOverlayPageId] = useState<string>();
  const [interactionMode, setInteractionMode] = useState(false);
  const [device, setDevice] = useState<WebDesignDevice>('desktop');
  const [viewportSelections, setViewportSelections] = useState<Record<WebDesignDevice, ViewportSelection>>(() => structuredClone(DEFAULT_VIEWPORT_SELECTIONS));
  const [workspaceCamera, setWorkspaceCamera] = useState<WorkspaceCamera>({ x: 0, y: 0, zoom: 0.82 });
  const [workspacePlacement, setWorkspacePlacement] = useState<WorkspacePlacementDocument>();
  const [activeArtboardId, setActiveArtboardId] = useState<string>();
  const [scenePreviewHeights, setScenePreviewHeights] = useState<Record<string, number>>({});
  const [newSurfaceKind, setNewSurfaceKind] = useState<WorkspaceSurfaceKind>('page');
  const [past, setPast] = useState<WebDesignDocument[]>([]);
  const [future, setFuture] = useState<WebDesignDocument[]>([]);
  const [toast, setToast] = useState<string>();
  const [annotationText, setAnnotationText] = useState('');
  const [aiInstruction, setAiInstruction] = useState('');
  const [sceneAiContext, setSceneAiContext] = useState<SceneAnnotationAiContext>();
  const [sceneAnnotationPreparingId, setSceneAnnotationPreparingId] = useState<string>();
  const [generationPlan, setGenerationPlan] = useState<GenerationPlanSummary>();
  const [generationReview, setGenerationReview] = useState<GenerationStepReview>();
  const [generationLoading, setGenerationLoading] = useState(false);
  const [generationAction, setGenerationAction] = useState<string>();
  const [generationRejectionReason, setGenerationRejectionReason] = useState('');
  const [paletteQuery, setPaletteQuery] = useState('');
  const [libraryTab, setLibraryTab] = useState<LibraryTab>('antd');
  const [personalSymbols, setPersonalSymbols] = useState<WebDesignSymbol[]>(loadPersonalSymbols);
  const [sceneSnippets, setSceneSnippets] = useState<SceneSnippet[]>(() => parseSceneSnippets(window.localStorage.getItem(SCENE_SNIPPETS_STORAGE_KEY)));
  const [sceneVariablesDraft, setSceneVariablesDraft] = useState('[]');
  const [variantPickerTarget, setVariantPickerTarget] = useState<VariantPickerTarget>();
  const [sceneContentFocus, setSceneContentFocus] = useState<SceneContentFocus>();
  const [variantPickerDrag, setVariantPickerDrag] = useState<VariantPickerPointerDrag>();
  const [themePickerOpen, setThemePickerOpen] = useState(false);
  const [projectLibraryOpen, setProjectLibraryOpen] = useState(false);
  const [newDesignOpen, setNewDesignOpen] = useState(false);
  const [newDesignName, setNewDesignName] = useState('');
  const [deleteDesignTarget, setDeleteDesignTarget] = useState<DesignSummary>();
  const [deletingDesign, setDeletingDesign] = useState(false);
  const [editingSlot, setEditingSlot] = useState<EditingSlot>();
  const [inspectorVisualState, setInspectorVisualState] = useState<InspectorVisualState>('default');
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>('design');
  const [workspaceShell, dispatchWorkspaceShell] = useReducer(
    workspaceShellReducer,
    DEFAULT_WORKSPACE_SHELL,
    () => parseWorkspaceShellState(window.localStorage.getItem('web-design-studio.workspace-shell.v1'))
  );
  const interaction = useRef<Interaction | undefined>(undefined);
  const canvasPan = useRef<CanvasPan | undefined>(undefined);
  const canvasMarquee = useRef<CanvasMarquee | undefined>(undefined);
  const spacePressed = useRef(false);
  const workspaceCameraContext = useRef<string | undefined>(undefined);
  const workspaceCameraBeforeSlot = useRef<WorkspaceCamera | undefined>(undefined);
  const slotCameraContext = useRef<string | undefined>(undefined);
  const persistedWorkspaceArtboards = useRef<string>('');
  const variantPickerDragRef = useRef<VariantPickerPointerDrag | undefined>(undefined);
  const documentRef = useRef<WebDesignDocument | undefined>(undefined);
  const sceneDocumentRef = useRef<SceneDocument | undefined>(undefined);
  const sceneCommandQueue = useRef<Promise<void>>(Promise.resolve());
  const sceneHistoryRequestId = useRef(0);
  const assetInput = useRef<HTMLInputElement | null>(null);
  const canvasStage = useRef<HTMLElement | null>(null);
  const canvasScroll = useRef<HTMLDivElement | null>(null);
  const zoom = workspaceCamera.zoom;
  const interactionZoom = useRef(zoom);
  const [canvasPanning, setCanvasPanning] = useState(false);
  const [canvasPanReady, setCanvasPanReady] = useState(false);

  useEffect(() => { documentRef.current = document; }, [document]);
  useEffect(() => { sceneDocumentRef.current = sceneDocument; }, [sceneDocument]);
  useEffect(() => { setSceneAiContext(undefined); }, [sceneDocument?.documentId, selectedId]);

  useEffect(() => {
    const stage = canvasStage.current;
    const viewport = canvasScroll.current;
    if (!stage || !viewport || screen !== 'editor') return;
    const onWheel = (event: WheelEvent) => {
      const isPinchZoom = event.ctrlKey || event.metaKey;
      const target = event.target;
      const isInsideCanvasStage = target instanceof Node && stage.contains(target);
      if (isPinchZoom) {
        event.preventDefault();
        event.stopPropagation();
      }
      if (!isInsideCanvasStage) return;
      if (!isPinchZoom && !(target instanceof Node && viewport.contains(target))) return;
      event.preventDefault();
      event.stopPropagation();
      if (isPinchZoom) {
        const bounds = viewport.getBoundingClientRect();
        const anchor = {
          x: Math.min(viewport.clientWidth, Math.max(0, event.clientX - bounds.left)),
          y: Math.min(viewport.clientHeight, Math.max(0, event.clientY - bounds.top))
        };
        setWorkspaceCamera((current) => zoomWorkspaceCameraAt(
          current,
          workspaceZoomFromWheel(current.zoom, event.deltaY),
          anchor
        ));
        return;
      }
      setWorkspaceCamera((current) => panWorkspaceCamera(current, { x: -event.deltaX, y: -event.deltaY }));
    };
    window.addEventListener('wheel', onWheel, { capture: true, passive: false });
    return () => window.removeEventListener('wheel', onWheel, { capture: true });
  }, [screen]);

  const selected = useMemo(
    () => document?.components.find((component) => component.id === selectedId),
    [document, selectedId]
  );
  const selectedSceneEntry = useMemo(() => {
    if (!sceneDocument || !selectedId) return undefined;
    return indexSceneDocument(sceneDocument).get(selectedId);
  }, [sceneDocument, selectedId]);
  const selectedSceneNode: SceneNode | undefined = selectedSceneEntry?.node;
  useEffect(() => {
    if (!sceneContentFocus || !sceneDocument) return;
    const entry = indexSceneDocument(sceneDocument).get(sceneContentFocus.nodeId);
    if (!entry || entry.pageId !== sceneContentFocus.pageId) setSceneContentFocus(undefined);
  }, [sceneContentFocus, sceneDocument]);
  const sceneResponsiveRuleSpec = device === 'desktop' ? undefined : SCENE_RESPONSIVE_EDITOR_RULES[device];
  const selectedSceneResponsiveOverride = useMemo(() => {
    if (!sceneDocument || !selectedSceneNode || !sceneResponsiveRuleSpec) return undefined;
    return sceneDocument.responsiveRules.find((rule) => rule.id === sceneResponsiveRuleSpec.ruleId)
      ?.nodeOverrides.find((override) => override.nodeId === selectedSceneNode.id);
  }, [sceneDocument, sceneResponsiveRuleSpec, selectedSceneNode]);
  const selectedScenePositionEditable = useMemo(() => {
    if (!sceneDocument || !selectedSceneEntry) return false;
    if (sceneDocument.pages.some((page) => page.id === selectedSceneEntry.parentId)) return true;
    const parent = indexSceneDocument(sceneDocument).get(selectedSceneEntry.parentId)?.node;
    return Boolean(parent && (parent.layout.mode === 'free' || selectedSceneEntry.node.layout.position === 'absolute'));
  }, [sceneDocument, selectedSceneEntry]);
  const activeScenePage = useMemo(
    () => sceneDocument?.pages.find((page) => page.id === pageId),
    [sceneDocument, pageId]
  );
  const sceneEditingActive = sceneLoadState === 'loading' || sceneLoadState === 'missing' || Boolean(sceneDocument);
  const selectedFrame = useMemo(
    () => selected ? resolveComponent(selected, device) : undefined,
    [selected, device]
  );
  const selectedIdSet = useMemo(() => new Set(selectedIds), [selectedIds]);
  const activeWorkspaceArtboard = useMemo(
    () => workspacePlacement?.artboards.find((artboard) => artboard.artboardId === activeArtboardId),
    [activeArtboardId, workspacePlacement?.artboards]
  );
  const breakpoint = useMemo(
    () => document ? breakpointFor(document, device) : { width: 1200, height: 940 },
    [document, device]
  );
  const viewportPresets = useMemo(
    () => sceneDocument ? WEB_DESIGN_ARTBOARD_SIZE_PRESETS : viewportPresetsForDevice(device),
    [device, sceneDocument]
  );
  const configuredViewportSelection = viewportSelections[device];
  const activeViewportWidth = sceneDocument && activeWorkspaceArtboard
    ? activeWorkspaceArtboard.viewportWidth
    : breakpoint.width;
  const sceneViewportMatch = sceneDocument && activeWorkspaceArtboard
    ? matchArtboardSizePreset(activeWorkspaceArtboard.viewportWidth, activeWorkspaceArtboard.viewportHeight)
    : undefined;
  const viewportSelection = sceneDocument && activeWorkspaceArtboard
    ? {
        presetId: sceneViewportMatch?.preset.id,
        orientation: sceneViewportMatch?.orientation ?? 'default' as WebDesignViewportOrientation,
        customHeight: activeWorkspaceArtboard.viewportHeight
      }
    : configuredViewportSelection;
  const viewportPreset = viewportPresets.find((preset) => preset.id === viewportSelection.presetId);
  const previewViewportHeight = sceneDocument && activeWorkspaceArtboard
    ? activeWorkspaceArtboard.viewportHeight
    : viewportPreset
    ? viewportDimensions(viewportPreset, viewportSelection.orientation).height
    : viewportSelection.customHeight;
  const renderedCanvasHeight = sceneDocument
    ? scenePreviewHeights[pageId]
      ?? sceneArtboardContentHeight(sceneDocument, pageId, activeViewportWidth, Math.max(breakpoint.height, previewViewportHeight))
    : Math.max(breakpoint.height, previewViewportHeight);
  const pages = useMemo(() => {
    if (sceneDocument) {
      const artboardsByPage = new Map(workspacePlacement?.artboards.map((artboard) => [artboard.pageId, artboard]));
      return sceneDocument.pages.map((page) => ({
        id: page.id,
        name: page.name,
        slug: `/${page.id}`,
        surfaceKind: artboardsByPage.get(page.id)?.surfaceKind ?? 'page' as WorkspaceSurfaceKind
      }));
    }
    return document ? pagesForDocument(document) : [];
  }, [document, sceneDocument, workspacePlacement?.artboards]);
  const previewOverlayPage = useMemo(() => pages.find((page) => page.id === previewOverlayPageId), [pages, previewOverlayPageId]);
  const previewOverlayArtboard = useMemo(() => workspacePlacement?.artboards.find((artboard) => artboard.pageId === previewOverlayPageId), [workspacePlacement?.artboards, previewOverlayPageId]);
  const selectedPrototypeTarget = useMemo(() => {
    const targetPageId = selectedSceneNode?.prototypeLink?.targetPageId
      ?? (selected?.interaction?.type === 'page' ? selected.interaction.target : undefined);
    return targetPageId ? pages.find((page) => page.id === targetPageId) : undefined;
  }, [pages, selected, selectedSceneNode]);
  const activeProjectDocuments = useMemo(() => {
    const ids = new Set(activeProject?.designIds ?? []);
    return documents.filter((item) => ids.has(item.documentId));
  }, [activeProject?.designIds, documents]);
  const tokens = useMemo(() => document ? tokensForDocument(document) : undefined, [document]);
  const currentPage = useMemo(() => pages.find((page) => page.id === pageId) ?? pages[0], [pages, pageId]);
  const pageComponents = useMemo(() => document && currentPage ? componentsForPage(document, currentPage.id) : [], [document, currentPage]);
  const editingContainer = useMemo(() => editingSlot ? document?.components.find((component) => component.id === editingSlot.componentId) : undefined, [document, editingSlot]);
  const editingSlotDefinition = useMemo(() => editingContainer && editingSlot
    ? editableSlotsForUiComponent(editingContainer).find((slot) => slot.id === editingSlot.slotId)
    : undefined, [editingContainer, editingSlot]);
  const editingSlotComponents = useMemo(() => document && editingSlot
    ? componentsInSlot(document, editingSlot.componentId, editingSlot.slotId)
    : [], [document, editingSlot]);
  const editingVisibleComponents = useMemo(() => document && editingSlot
    ? visibleComponentsInSlot(document, editingSlot.componentId, editingSlot.slotId)
    : [], [document, editingSlot]);
  const editingSlotCanvasSize = useMemo(() => {
    if (!editingContainer || !editingSlotDefinition) return undefined;
    const containerFrame = resolveComponent(editingContainer, device);
    return fitContentCanvasToComponents(editingVisibleComponents, device, {
      minimumWidth: editingSlotDefinition.width,
      minimumHeight: editingSlotDefinition.height,
      originX: containerFrame.x,
      originY: containerFrame.y
    });
  }, [editingContainer, editingSlotDefinition, editingVisibleComponents, device]);

  useEffect(() => {
    if (!editingSlot || !editingSlotCanvasSize || screen !== 'editor') return;
    const context = `${editingSlot.componentId}:${editingSlot.slotId}:${device}`;
    if (slotCameraContext.current === context) return;
    const frame = window.requestAnimationFrame(() => {
      const viewport = canvasScroll.current;
      if (!viewport) return;
      slotCameraContext.current = context;
      setWorkspaceCamera(fitWorkspaceRect(
        slotEditorFrameBounds(editingSlotCanvasSize),
        { width: viewport.clientWidth, height: viewport.clientHeight },
        { top: 112, right: 72, bottom: 88, left: 72 },
        2.5
      ));
    });
    return () => window.cancelAnimationFrame(frame);
  }, [screen, editingSlot?.componentId, editingSlot?.slotId, device, editingSlotCanvasSize?.width, editingSlotCanvasSize?.height]);
  const inspectedFrame = useMemo(() => {
    if (!selectedFrame || !editingContainer || !editingSlot || !selected || slotIdForDescendant(document!, selected, editingContainer.id) !== editingSlot.slotId) return selectedFrame;
    const containerFrame = resolveComponent(editingContainer, device);
    return { ...selectedFrame, x: selectedFrame.x - containerFrame.x, y: selectedFrame.y - containerFrame.y };
  }, [selectedFrame, editingContainer, editingSlot, selected, document, device]);
  const inspectedStyle = useMemo(() => inspectorVisualState === 'default'
    ? inspectedFrame?.style
    : mergeComponentStyles(inspectedFrame?.style ?? {}, selected?.states?.[inspectorVisualState]), [inspectorVisualState, inspectedFrame?.style, selected?.states]);

  useEffect(() => setInspectorVisualState('default'), [selectedId]);

  useEffect(() => {
    window.localStorage.setItem(PERSONAL_SYMBOLS_STORAGE_KEY, JSON.stringify(personalSymbols));
  }, [personalSymbols]);

  useEffect(() => {
    window.localStorage.setItem(SCENE_SNIPPETS_STORAGE_KEY, JSON.stringify(sceneSnippets));
  }, [sceneSnippets]);

  useEffect(() => {
    setSceneVariablesDraft(JSON.stringify(sceneDocument?.variableCollections ?? [], null, 2));
  }, [sceneDocument?.documentId, sceneDocument?.revision]);

  useEffect(() => {
    window.localStorage.setItem('web-design-studio.workspace-shell.v1', JSON.stringify(workspaceShell));
  }, [workspaceShell]);

  useEffect(() => {
    if (!document?.symbols?.length) return;
    setPersonalSymbols((current) => {
      const byId = new Map(current.map((symbol) => [symbol.id, symbol]));
      let changed = false;
      for (const symbol of document.symbols ?? []) {
        if (!byId.has(symbol.id)) {
          byId.set(symbol.id, structuredClone(symbol));
          changed = true;
        }
      }
      return changed ? [...byId.values()] : current;
    });
  }, [document?.documentId]);

  useEffect(() => {
    void (async () => {
      const repo = await createRepository();
      setRepository(repo);
      const [items, projectItems, runtimeContext] = await Promise.all([repo.list(), repo.listProjects(), repo.runtimeContext()]);
      setDocuments(items);
      const requested = studioLocationSelection();
      const requestedProjectId = runtimeContext.defaultProjectId ?? projectItems[0]?.projectId;
      if (requestedProjectId) {
        const project = await repo.readProject(requestedProjectId);
        setActiveProject(project);
        if (requested.documentId && project.designIds.includes(requested.documentId)) {
          actionsRef.current.openDocument(await repo.read(requested.documentId));
          setScreen('editor');
          replaceStudioLocation(project.projectId, requested.documentId);
        } else {
          setScreen('project');
          replaceStudioLocation(project.projectId);
        }
      }
      setReady(true);
    })().catch((error) => {
      setReady(true);
      actionsRef.current.showToast(error instanceof Error ? error.message : String(error));
    });
  }, []);

  useEffect(() => {
    if (!repository || !document || screen !== 'editor') {
      sceneDocumentRef.current = undefined;
      setSceneDocument(undefined);
      setSceneHistory(undefined);
      setSceneLoadState('idle');
      return;
    }
    let cancelled = false;
    setSceneLoadState('loading');
    setSceneDocument(undefined);
    setSceneHistory(undefined);
    void repository.readScene(document.documentId).then(async (nextScene) => {
      const history = await repository.readSceneHistory(document.documentId).catch(() => ({ undoCount: 0, redoCount: 0 }));
      if (cancelled) return;
      sceneDocumentRef.current = nextScene;
      setSceneDocument(nextScene);
      setSceneHistory(history);
      setSceneLoadState('ready');
    }).catch(() => {
      if (cancelled) return;
      sceneDocumentRef.current = undefined;
      setSceneDocument(undefined);
      setSceneHistory(undefined);
      setSceneLoadState('missing');
    });
    return () => { cancelled = true; };
  }, [repository, document?.documentId, screen, sceneReloadToken]);

  useEffect(() => {
    if (!repository || !document || screen !== 'editor' || repository.mode !== 'server') {
      setGenerationPlan(undefined);
      setGenerationReview(undefined);
      return;
    }
    let cancelled = false;
    const refresh = async (showLoading: boolean) => {
      if (showLoading) setGenerationLoading(true);
      try {
        const plan = await repository.readGenerationPlan(document.documentId);
        if (cancelled) return;
        setGenerationPlan(plan);
        const activeStep = plan?.activeStep;
        if (activeStep?.stepId && activeStep.activeAttemptId) {
          const review = await repository.inspectGenerationStep(document.documentId, activeStep.stepId, activeStep.activeAttemptId);
          if (!cancelled) setGenerationReview(review);
        } else if (!cancelled) {
          setGenerationReview(undefined);
        }
      } catch (error) {
        if (!cancelled && showLoading) actionsRef.current.showToast(error instanceof Error ? error.message : String(error));
      } finally {
        if (!cancelled && showLoading) setGenerationLoading(false);
      }
    };
    void refresh(true);
    const timer = window.setInterval(() => void refresh(false), 3000);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [repository, document?.documentId, screen]);

  useEffect(() => {
    const onMove = (event: PointerEvent) => {
      const activePan = canvasPan.current;
      if (activePan) {
        setWorkspaceCamera(panWorkspaceCamera(activePan.camera, {
          x: event.clientX - activePan.pointerX,
          y: event.clientY - activePan.pointerY
        }));
        return;
      }
      const activeMarquee = canvasMarquee.current;
      if (activeMarquee && event.pointerId === activeMarquee.pointerId) {
        const distance = Math.hypot(event.clientX - activeMarquee.startClientX, event.clientY - activeMarquee.startClientY);
        if (!activeMarquee.moved && distance < 4) return;
        activeMarquee.moved = true;
        const bounds = activeMarquee.canvas.getBoundingClientRect();
        const scaleX = bounds.width / Math.max(1, activeMarquee.canvas.offsetWidth);
        const scaleY = bounds.height / Math.max(1, activeMarquee.canvas.offsetHeight);
        const point = {
          x: (event.clientX - bounds.left) / Math.max(scaleX, .0001),
          y: (event.clientY - bounds.top) / Math.max(scaleY, .0001)
        };
        const rect = normalizedSelectionRect(activeMarquee.startPoint, point);
        const matchedIds = selectionNodesInRect(activeMarquee.nodes, rect).map((node) => node.id);
        const nextIds = activeMarquee.additive
          ? [...activeMarquee.initialIds, ...matchedIds.filter((id) => !activeMarquee.initialIds.includes(id))]
          : matchedIds;
        setMarqueeRect(rect);
        setSelectedIds(nextIds);
        setSelectedId(matchedIds.at(-1) ?? activeMarquee.initialPrimaryId);
        return;
      }
      const active = interaction.current;
      if (!active) return;
      const dx = (event.clientX - active.pointerX) / active.scale;
      const dy = (event.clientY - active.pointerY) / active.scale;
      if (active.kind === 'move') {
        const movingIds = selectedRootIds(active.snapshot, active.selectedIds).flatMap((id) => [id, ...descendantIds(active.snapshot, id)]);
        const candidate = { ...active.frame, x: active.frame.x + dx, y: active.frame.y + dy };
        const snapped = active.scoped
          ? { frame: candidate, guides: {} as SnapGuides }
          : snapComponentFrame(active.snapshot, active.componentId, device, candidate, movingIds);
        setSnapGuides(snapped.guides);
        actionsRef.current.changeLiveWithCanvasGrowth(() => {
          const moved = moveComponentsWithDescendants(
            active.snapshot,
            active.selectedIds,
            device,
            snapped.frame.x - active.frame.x,
            snapped.frame.y - active.frame.y
          );
          const moving = new Set(movingIds);
          return { ...moved, components: moved.components.map((component) => moving.has(component.id) ? setSymbolOverride(component, 'frame', true) : component) };
        });
      } else {
        actionsRef.current.changeLiveWithCanvasGrowth(() => ({
          ...active.snapshot,
          components: active.snapshot.components.map((component) => component.id === active.componentId
            ? setSymbolOverride(updateComponentFrame(component, device, constrainComponentFrame(component, device, {
              width: Math.max(24, active.frame.width + dx),
              height: Math.max(24, active.frame.height + dy)
            })), 'frame', true)
            : component)
        }));
      }
    };
    const onUp = (event: PointerEvent) => {
      if (canvasPan.current) {
        canvasPan.current = undefined;
        setCanvasPanning(false);
      }
      const activeMarquee = canvasMarquee.current;
      if (activeMarquee && event.pointerId === activeMarquee.pointerId) {
        if (!activeMarquee.moved && !activeMarquee.additive) {
          setSelectedId(undefined);
          setSelectedIds([]);
        }
        canvasMarquee.current = undefined;
        setMarqueeRect(undefined);
        return;
      }
      const active = interaction.current;
      if (!active) return;
      setPast((items) => [...items.slice(-59), active.snapshot]);
      setFuture([]);
      interaction.current = undefined;
      setSnapGuides({});
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('pointercancel', onUp);
    return () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('pointercancel', onUp);
    };
  }, [device, zoom, pageId, editingSlot, workspaceCamera.zoom]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.matches('input, textarea, select, [contenteditable="true"]')) return;
      if (event.code === 'Space') {
        event.preventDefault();
        spacePressed.current = true;
        setCanvasPanReady(true);
        return;
      }
      const command = event.metaKey || event.ctrlKey;
      const shellAction = workspaceShellShortcut(event.key, command);
      if (event.key === 'Escape' && selectionCandidatePopover) {
        event.preventDefault();
        setSelectionCandidatePopover(undefined);
      } else if (event.key === 'Escape' && interactionMode) {
        event.preventDefault();
        actionsRef.current.toggleInteractionMode();
      } else if (command && event.key.toLowerCase() === 's') {
        event.preventDefault();
        void actionsRef.current.save();
      } else if (command && !event.shiftKey && event.key.toLowerCase() === 'z') {
        event.preventDefault();
        actionsRef.current.undo();
      } else if (command && event.shiftKey && event.key.toLowerCase() === 'z') {
        event.preventDefault();
        actionsRef.current.redo();
      } else if (command && event.shiftKey && event.key.toLowerCase() === 'g') {
        event.preventDefault();
        actionsRef.current.ungroupSelected();
      } else if (command && event.key.toLowerCase() === 'g') {
        event.preventDefault();
        actionsRef.current.groupSelected();
      } else if (command && event.key.toLowerCase() === 'd') {
        event.preventDefault();
        actionsRef.current.duplicateSelected();
      } else if (command && event.key.toLowerCase() === 'c' && selectedIds.length > 0) {
        event.preventDefault();
        actionsRef.current.copySelected();
      } else if (command && event.key.toLowerCase() === 'v' && (sceneEditingActive ? sceneClipboard.length > 0 : Boolean(clipboard))) {
        event.preventDefault();
        actionsRef.current.pasteClipboard();
      } else if (event.key === 'Enter' && selectedId && !event.shiftKey) {
        event.preventDefault();
        actionsRef.current.selectSelectionChild();
      } else if (event.key === 'Enter' && selectedId && event.shiftKey) {
        event.preventDefault();
        actionsRef.current.selectSelectionParent();
      } else if (shellAction) {
        event.preventDefault();
        if (shellAction.type === 'select-tool') actionsRef.current.activateWorkspaceTool(shellAction.tool);
        else dispatchWorkspaceShell(shellAction);
      } else if ((event.key === 'Backspace' || event.key === 'Delete') && selectedIds.length > 0) {
        event.preventDefault();
        if (sceneEditingActive) void actionsRef.current.deleteSceneSelection();
        else actionsRef.current.deleteSelected();
      } else if (selectedIds.length > 0 && ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) {
        event.preventDefault();
        const amount = event.shiftKey ? 10 : 1;
        const dx = event.key === 'ArrowLeft' ? -amount : event.key === 'ArrowRight' ? amount : 0;
        const dy = event.key === 'ArrowUp' ? -amount : event.key === 'ArrowDown' ? amount : 0;
        if (sceneEditingActive) void actionsRef.current.nudgeSceneSelection(dx, dy);
        else actionsRef.current.nudgeSelected(dx, dy);
      }
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.code !== 'Space') return;
      spacePressed.current = false;
      setCanvasPanReady(false);
    };
    const onBlur = () => {
      spacePressed.current = false;
      canvasPan.current = undefined;
      setCanvasPanReady(false);
      setCanvasPanning(false);
    };
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
    window.addEventListener('blur', onBlur);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
      window.removeEventListener('blur', onBlur);
    };
  }, [selectedId, selectedIds, selectionCandidatePopover, past, future, dirty, saving, repository, persistedRevision, device, clipboard, sceneClipboard, sceneEditingActive, sceneDocument, pageId, interactionMode, zoom, breakpoint.width, editingSlot]);

  useEffect(() => {
    if (screen !== 'editor' || !document || !repository) {
      workspaceCameraContext.current = undefined;
      setWorkspacePlacement(undefined);
      return;
    }
    if (sceneLoadState === 'idle' || sceneLoadState === 'loading') return;
    if (editingSlot) return;
    const context = document.documentId;
    if (workspaceCameraContext.current === context) return;
    let cancelled = false;
    void repository.readWorkspace(document.documentId).then(async (storedPlacement) => {
      if (cancelled) return;
      const activeScene = sceneLoadState === 'ready' ? sceneDocument : undefined;
      const reconciledArtboards = storedPlacement.artboards.length === 0
        ? initialWorkspaceArtboards(document, activeScene)
        : reconcileWorkspaceArtboards(document, storedPlacement.artboards, activeScene);
      const placementChanged = workspaceArtboardSignature(reconciledArtboards) !== workspaceArtboardSignature(storedPlacement.artboards);
      const placement = placementChanged
        ? await repository.saveWorkspaceArtboards(document.documentId, reconciledArtboards)
        : storedPlacement;
      if (cancelled) return;
      const viewport = canvasScroll.current;
      const first = placement.artboards[0];
      const camera = first && viewport
        ? fitWorkspaceRect(
            workspaceArtboardContentBounds(document, { ...first, x: 0, y: 0 }, sceneDocumentRef.current),
            { width: viewport.clientWidth, height: viewport.clientHeight },
            { top: 92, right: 64, bottom: 92, left: 64 }
          )
        : placement.camera;
      workspaceCameraContext.current = context;
      persistedWorkspaceArtboards.current = workspaceArtboardSignature(placement.artboards);
      setWorkspacePlacement(placement);
      setActiveArtboardId(first?.artboardId);
      if (first) {
        setDevice(deviceForWorkspaceArtboard(document, first));
        setPageId(first.pageId);
      }
      setWorkspaceCamera(camera);
    }).catch((error) => actionsRef.current.showToast(error instanceof Error ? error.message : String(error)));
    return () => { cancelled = true; };
  }, [screen, editingSlot, document?.documentId, repository, sceneDocument, sceneLoadState]);

  useEffect(() => {
    if (!document || screen !== 'editor' || editingSlot) return;
    if (!repository) return;
    const context = document.documentId;
    if (workspaceCameraContext.current !== context) return;
    const timeout = window.setTimeout(() => {
      void repository.saveWorkspaceCamera(document.documentId, workspaceCamera)
        .catch((error) => actionsRef.current.showToast(error instanceof Error ? error.message : String(error)));
    }, 120);
    return () => window.clearTimeout(timeout);
  }, [document?.documentId, repository, screen, editingSlot, workspaceCamera]);

  useEffect(() => {
    if (!document || !repository || !workspacePlacement || screen !== 'editor' || editingSlot) return;
    if (workspaceCameraContext.current !== document.documentId) return;
    const signature = workspaceArtboardSignature(workspacePlacement.artboards);
    if (signature === persistedWorkspaceArtboards.current) return;
    const timeout = window.setTimeout(() => {
      void repository.saveWorkspaceArtboards(document.documentId, workspacePlacement.artboards).then((saved) => {
        persistedWorkspaceArtboards.current = workspaceArtboardSignature(saved.artboards);
        setWorkspacePlacement((current) => current ? { ...current, revision: saved.revision, updatedAt: saved.updatedAt } : current);
      }).catch((error) => actionsRef.current.showToast(error instanceof Error ? error.message : String(error)));
    }, 120);
    return () => window.clearTimeout(timeout);
  }, [document?.documentId, repository, workspacePlacement?.artboards, screen, editingSlot]);

  return {
    actionsRef, repository, setRepository, documents, setDocuments, activeProject,
    setActiveProject, document, setDocument, sceneDocument, setSceneDocument, sceneHistory,
    setSceneHistory, sceneLoadState, setSceneLoadState, sceneReloadToken, setSceneReloadToken, ready,
    setReady, screen, setScreen, persistedRevision, setPersistedRevision, selectedId,
    setSelectedId, selectedIds, setSelectedIds, selectionCandidatePopover, setSelectionCandidatePopover, marqueeRect,
    setMarqueeRect, pageId, setPageId, clipboard, setClipboard, sceneClipboard,
    setSceneClipboard, snapGuides, setSnapGuides, dirty, setDirty, saving,
    setSaving, previewOverlayPageId, setPreviewOverlayPageId, interactionMode, setInteractionMode, device,
    setDevice, viewportSelections, setViewportSelections, workspaceCamera, setWorkspaceCamera, workspacePlacement,
    setWorkspacePlacement, activeArtboardId, setActiveArtboardId, scenePreviewHeights, setScenePreviewHeights, newSurfaceKind,
    setNewSurfaceKind, past, setPast, future, setFuture, toast,
    setToast, annotationText, setAnnotationText, aiInstruction, setAiInstruction, sceneAiContext,
    setSceneAiContext, sceneAnnotationPreparingId, setSceneAnnotationPreparingId, generationPlan, setGenerationPlan, generationReview,
    setGenerationReview, generationLoading, setGenerationLoading, generationAction, setGenerationAction, generationRejectionReason,
    setGenerationRejectionReason, paletteQuery, setPaletteQuery, libraryTab, setLibraryTab, personalSymbols,
    setPersonalSymbols, sceneSnippets, setSceneSnippets, sceneVariablesDraft, setSceneVariablesDraft, variantPickerTarget,
    setVariantPickerTarget, sceneContentFocus, setSceneContentFocus, variantPickerDrag, setVariantPickerDrag, themePickerOpen,
    setThemePickerOpen, projectLibraryOpen, setProjectLibraryOpen, newDesignOpen, setNewDesignOpen, newDesignName,
    setNewDesignName, deleteDesignTarget, setDeleteDesignTarget, deletingDesign, setDeletingDesign, editingSlot,
    setEditingSlot, inspectorVisualState, setInspectorVisualState, inspectorTab, setInspectorTab, workspaceShell,
    dispatchWorkspaceShell, interaction, canvasPan, canvasMarquee, spacePressed, workspaceCameraContext,
    workspaceCameraBeforeSlot, slotCameraContext, persistedWorkspaceArtboards, variantPickerDragRef, documentRef, sceneDocumentRef,
    sceneCommandQueue, sceneHistoryRequestId, assetInput, canvasStage, canvasScroll, zoom,
    interactionZoom, canvasPanning, setCanvasPanning, canvasPanReady, setCanvasPanReady, selected,
    selectedSceneEntry, selectedSceneNode, sceneResponsiveRuleSpec, selectedSceneResponsiveOverride, selectedScenePositionEditable, activeScenePage,
    sceneEditingActive, selectedFrame, selectedIdSet, activeWorkspaceArtboard, breakpoint, viewportPresets,
    configuredViewportSelection, activeViewportWidth, sceneViewportMatch, viewportSelection, viewportPreset, previewViewportHeight,
    renderedCanvasHeight, pages, previewOverlayPage, previewOverlayArtboard, selectedPrototypeTarget, activeProjectDocuments,
    tokens, currentPage, pageComponents, editingContainer, editingSlotDefinition, editingSlotComponents,
    editingVisibleComponents, editingSlotCanvasSize, inspectedFrame, inspectedStyle
  };
}
