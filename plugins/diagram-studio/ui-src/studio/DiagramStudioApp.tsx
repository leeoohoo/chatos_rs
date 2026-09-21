import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Background,
  BackgroundVariant,
  ConnectionMode,
  Controls,
  MarkerType,
  MiniMap,
  ReactFlow,
  SelectionMode,
  useReactFlow
} from '@xyflow/react';
import type { DiagramDocument, DiagramKind, DiagramNode, DiagramProject, DiagramProjectSummary } from '../../src/schema';
import { layoutDiagram } from '../../src/layout';
import { analyzeMindMap } from '../../src/mindmap';
import { diagramToPlantUml } from '../../src/plantuml';
import { createRepository, type DiagramSummary } from './repository';
import { DiagramNodeView, LaneNodeView } from './DiagramNodes';
import { componentDragType, TemplateSidebar, type SequenceMessagePreset } from './TemplateSidebar';
import { Inspector } from './Inspector';
import { Icon } from './Icons';
import { measuredNode, type NodeMeasurementCache } from './node-measurements';
import { routingObstaclesForEdge, routingOffsetForEdge, runtimeEdgesForDocument, SequenceMessageEdge, SmartOrthogonalEdge } from './DiagramEdges';
import { createDiagramEditorActions } from './DiagramEditorActions';
import { renderDiagramStudioSheets } from './DiagramStudioSheets';
import {
  kindLabel,
  supportsPlantUml,
  plantUmlDialectLabel,
  plantUmlDescription,
  plantUmlEditorHint,
  formatUpdatedAt,
  defaultNodeSize,
  kindIcon
} from './DiagramStudioSupport';

const nodeTypes = { diagramNode: DiagramNodeView, laneNode: LaneNodeView };
const edgeTypes = { sequenceMessage: SequenceMessageEdge, smartOrthogonal: SmartOrthogonalEdge };

type Repository = Awaited<ReturnType<typeof createRepository>>;
type DeleteDocumentTarget = Pick<DiagramSummary, 'documentId' | 'title'>;

export function DiagramStudioApp() {
  const reactFlow = useReactFlow();
  const canvasRef = useRef<HTMLDivElement>(null);
  const importInputRef = useRef<HTMLInputElement>(null);
  const dragSnapshot = useRef<DiagramDocument | null>(null);
  const resizeSnapshot = useRef<DiagramDocument | null>(null);
  const edgeMoveSnapshot = useRef<DiagramDocument | null>(null);
  const edgeMoveChanged = useRef(false);
  const nodeMeasurements = useRef<NodeMeasurementCache>(new Map());
  const lastSequenceConnect = useRef<{ source: string; target: string; sourceSlot?: number; targetSlot?: number; at: number } | undefined>(undefined);
  const [repository, setRepository] = useState<Repository>();
  const [document, setDocument] = useState<DiagramDocument>();
  const [activeProject, setActiveProject] = useState<DiagramProject>();
  const [isReady, setIsReady] = useState(false);
  const [homeVisible, setHomeVisible] = useState(true);
  const [persistedRevision, setPersistedRevision] = useState(0);
  const [documents, setDocuments] = useState<DiagramSummary[]>([]);
  const [projects, setProjects] = useState<DiagramProjectSummary[]>([]);
  const [past, setPast] = useState<DiagramDocument[]>([]);
  const [future, setFuture] = useState<DiagramDocument[]>([]);
  const [dirty, setDirty] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [sidebarVisible, setSidebarVisible] = useState(true);
  const [inspectorVisible, setInspectorVisible] = useState(true);
  const [libraryVisible, setLibraryVisible] = useState(false);
  const [newDiagramVisible, setNewDiagramVisible] = useState(false);
  const [newProjectVisible, setNewProjectVisible] = useState(false);
  const [newProjectName, setNewProjectName] = useState('');
  const [newDiagramName, setNewDiagramName] = useState('');
  const [mindmapEdit, setMindmapEdit] = useState<{ nodeId: string; value: string }>();
  const [homeConfirmVisible, setHomeConfirmVisible] = useState(false);
  const [deleteDocumentTarget, setDeleteDocumentTarget] = useState<DeleteDocumentTarget>();
  const [isDeletingDocument, setIsDeletingDocument] = useState(false);
  const [exportVisible, setExportVisible] = useState(false);
  const [plantUmlVisible, setPlantUmlVisible] = useState(false);
  const [plantUmlSource, setPlantUmlSource] = useState('');
  const [plantUmlError, setPlantUmlError] = useState<string>();
  const [selectedNodeIds, setSelectedNodeIds] = useState<Set<string>>(() => new Set());
  const [selectedEdgeId, setSelectedEdgeId] = useState<string>();
  const [sequenceMessagePreset, setSequenceMessagePreset] = useState<SequenceMessagePreset>('call');
  const [toast, setToast] = useState<string>();

  const refreshDocuments = useCallback(async (repo: Repository) => {
    setDocuments(await repo.list());
  }, []);

  const refreshProjects = useCallback(async (repo: Repository) => {
    setProjects(await repo.listProjects());
  }, []);

  useEffect(() => {
    void (async () => {
      const repo = await createRepository();
      setRepository(repo);
      const items = await repo.list();
      const projectItems = await repo.listProjects();
      setDocuments(items);
      setProjects(projectItems);
      const runtimeContext = await repo.runtimeContext();
      if (runtimeContext.defaultProjectId) {
        const project = await repo.readProject(runtimeContext.defaultProjectId);
        setActiveProject(project);
        setHomeVisible(false);
      }
      setIsReady(true);
    })().catch((error) => {
      setIsReady(true);
      showToast(error instanceof Error ? error.message : String(error));
    });
  }, [reactFlow, refreshDocuments, refreshProjects]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.matches('input, textarea, select, [contenteditable="true"]')) return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's') {
        event.preventDefault();
        void save();
      } else if ((event.metaKey || event.ctrlKey) && !event.shiftKey && event.key.toLowerCase() === 'z') {
        event.preventDefault(); undo();
      } else if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === 'z') {
        event.preventDefault(); redo();
      } else if ((event.key === 'Backspace' || event.key === 'Delete') && (selectedNodeIds.size > 0 || selectedEdgeId)) {
        event.preventDefault(); deleteSelection();
      } else if (document?.kind === 'mindmap' && selectedNodeIds.size === 1 && (event.key === 'Tab' || event.key === 'Enter')) {
        event.preventDefault();
        const selectedId = [...selectedNodeIds][0];
        addMindMapRelative(selectedId, event.key === 'Enter' ? 'sibling' : 'child');
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  });

  function showToast(message: string) {
    setToast(message);
    window.setTimeout(() => setToast((current) => current === message ? undefined : current), 2600);
  }

  function commit(next: DiagramDocument) {
    if (!document) return;
    setPast((items) => [...items.slice(-39), structuredClone(document)]);
    setFuture([]);
    setDocument(next);
    setDirty(true);
  }

  function undo() {
    if (!document || past.length === 0) return;
    const previous = past[past.length - 1];
    setPast((items) => items.slice(0, -1));
    setFuture((items) => [structuredClone(document), ...items].slice(0, 40));
    setDocument(structuredClone(previous));
    setDirty(true);
  }

  function redo() {
    if (!document || future.length === 0) return;
    const next = future[0];
    setFuture((items) => items.slice(1));
    setPast((items) => [...items.slice(-39), structuredClone(document)]);
    setDocument(structuredClone(next));
    setDirty(true);
  }

  async function save(): Promise<DiagramDocument | undefined> {
    if (!repository || !document || isSaving) return document;
    if (!dirty) return document;
    setIsSaving(true);
    try {
      const nodeIds = new Set(document.nodes.map((node) => node.id));
      const normalizedDocument = {
        ...document,
        edges: document.edges.filter((edge) => nodeIds.has(edge.source) && nodeIds.has(edge.target))
      };
      if (supportsPlantUml(normalizedDocument.kind)) {
        normalizedDocument.notation = {
          format: 'plantuml',
          dialect: normalizedDocument.kind === 'mindmap'
            ? 'mindmap'
            : normalizedDocument.kind === 'sequence'
              ? 'sequence'
              : normalizedDocument.kind === 'architecture'
              ? 'component'
              : normalizedDocument.kind === 'topology'
                ? 'deployment'
                : 'activity',
          source: diagramToPlantUml(normalizedDocument),
          opaqueBlocks: normalizedDocument.notation?.opaqueBlocks,
          lastSyncedRevision: persistedRevision + 1
        };
      }
      const saved = await repository.save(normalizedDocument, persistedRevision);
      setDocument(saved);
      setPersistedRevision(saved.revision);
      setDirty(false);
      await refreshDocuments(repository);
      showToast('已保存');
      return saved;
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
      return undefined;
    } finally {
      setIsSaving(false);
    }
  }

  async function createProject() {
    if (!repository) return;
    const name = newProjectName.trim();
    if (!name) {
      showToast('请先填写用户项目名称。');
      return;
    }
    try {
      const created = await repository.createProject(name);
      setActiveProject(created);
      setHomeVisible(false);
      setDocument(undefined);
      await refreshProjects(repository);
      setNewProjectVisible(false);
      setNewProjectName('');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function createBlankDiagram(kind: DiagramKind) {
    if (!repository || !activeProject) return;
    const title = newDiagramName.trim();
    if (!title) {
      showToast('请先填写图形名称。');
      return;
    }
    try {
      const created = await repository.createInProject(activeProject.projectId, kind, title);
      setActiveProject(await repository.readProject(activeProject.projectId));
      await Promise.all([refreshDocuments(repository), refreshProjects(repository)]);
      setNewDiagramVisible(false);
      setNewDiagramName('');
      openResolvedDocument(created);
      window.setTimeout(() => reactFlow.fitView({ padding: 0.16, duration: 320 }), 60);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function openProject(projectId: string) {
    if (!repository) return;
    try {
      setActiveProject(await repository.readProject(projectId));
      setDocument(undefined);
      setHomeVisible(false);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function openDocument(documentId: string) {
    if (!repository) return;
    try {
      openResolvedDocument(await repository.read(documentId));
      setLibraryVisible(false);
      window.setTimeout(() => reactFlow.fitView({ padding: 0.16, duration: 320 }), 60);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function requestDeleteDocument(target: DeleteDocumentTarget) {
    setDeleteDocumentTarget(target);
    setLibraryVisible(false);
  }

  async function deleteRequestedDocument() {
    if (!repository || !deleteDocumentTarget || isDeletingDocument) return;
    const target = deleteDocumentTarget;
    const deletingCurrent = document?.documentId === target.documentId;
    setIsDeletingDocument(true);
    try {
      await repository.remove(target.documentId);
      if (activeProject) setActiveProject(await repository.readProject(activeProject.projectId));
      await Promise.all([refreshDocuments(repository), refreshProjects(repository)]);
      if (deletingCurrent) {
        setDocument(undefined);
        setPersistedRevision(0);
        setDirty(false);
        setPast([]);
        setFuture([]);
        setSelectedNodeIds(new Set());
        setSelectedEdgeId(undefined);
        setPlantUmlVisible(false);
        setExportVisible(false);
      }
      setDeleteDocumentTarget(undefined);
      showToast(`已删除“${target.title}”`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    } finally {
      setIsDeletingDocument(false);
    }
  }

  function openResolvedDocument(next: DiagramDocument) {
    setDocument(next);
    setHomeVisible(false);
    setPersistedRevision(next.revision);
    setDirty(false);
    setPast([]);
    setFuture([]);
    setSelectedNodeIds(new Set());
    setSelectedEdgeId(undefined);
  }

  function openNewProjectSheet() {
    setNewProjectName('');
    setNewProjectVisible(true);
    setLibraryVisible(false);
  }

  function openNewDiagramSheet() {
    setNewDiagramName('');
    setNewDiagramVisible(true);
    setLibraryVisible(false);
  }

  function requestHome() {
    if (dirty) {
      setHomeConfirmVisible(true);
      return;
    }
    goHome();
  }

  function goHome() {
    setHomeVisible(true);
    setDocument(undefined);
    setActiveProject(undefined);
    setDirty(false);
    setPast([]);
    setFuture([]);
    setSelectedNodeIds(new Set());
    setSelectedEdgeId(undefined);
    setLibraryVisible(false);
    setExportVisible(false);
    setHomeConfirmVisible(false);
  }

  async function saveAndGoHome() {
    const saved = await save();
    if (saved) goHome();
  }

  async function autoLayout() {
    if (!repository || !document) return;
    try {
      if (repository.mode === 'server') {
        const saved = dirty ? await save() : document;
        if (!saved) return;
        const laidOut = await repository.autoLayout(saved.documentId, saved.revision);
        openResolvedDocument(laidOut);
      } else {
        commit(await layoutDiagram(document));
      }
      window.setTimeout(() => reactFlow.fitView({ padding: 0.18, duration: 360 }), 40);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function updateDocumentLive(next: DiagramDocument) {
    setDocument(next);
    setDirty(true);
  }

  const {
    onNodesChange,
    onEdgesChange,
    onConnect,
    onConnectEnd,
    addNode,
    onCanvasDrop,
    updateNode,
    addMindMapRelative,
    toggleMindMapCollapse,
    changeNodeLayer,
    updateEdge,
    deleteSelection,
    onDragStart,
    onDragStop,
    beginSequenceEdgeMove,
    moveSequenceEdge,
    finishSequenceEdgeMove,
    openPlantUmlEditor,
    applyPlantUmlSource,
    exportDiagram,
    importDiagram,
    saveImported
  } = createDiagramEditorActions({
    document,
    setDocument,
    setDirty,
    nodeMeasurements,
    selectedNodeIds,
    setSelectedNodeIds,
    setSelectedEdgeId,
    selectedEdgeId,
    setPast,
    setFuture,
    edgeMoveSnapshot,
    edgeMoveChanged,
    lastSequenceConnect,
    sequenceMessagePreset,
    commit,
    showToast,
    reactFlow,
    setMindmapEdit,
    dragSnapshot,
    resizeSnapshot,
    repository,
    canvasRef,
    openResolvedDocument,
    persistedRevision,
    refreshDocuments,
    setPlantUmlSource,
    setPlantUmlError,
    setPlantUmlVisible,
    plantUmlSource,
    activeProject
    ,updateDocumentLive
    ,setInspectorVisible
    ,setExportVisible
    ,setActiveProject
    ,refreshProjects
  });
  const selectedNodes = document?.nodes.filter((node) => selectedNodeIds.has(node.id)) ?? [];
  const selectedNode = selectedNodes.length === 1 ? selectedNodes[0] : undefined;
  const selectedEdge = document?.edges.find((edge) => edge.id === selectedEdgeId);
  const activeProjectDocuments = useMemo(() => {
    const ids = new Set(activeProject?.diagramIds ?? []);
    return documents.filter((item) => ids.has(item.documentId));
  }, [activeProject?.diagramIds, documents]);
  const flowNodes = useMemo(() => {
    if (!document) return [];
    const mindmap = document.kind === 'mindmap' ? analyzeMindMap(document) : undefined;
    return document.nodes.map((node) => measuredNode(nodeMeasurements.current, document.documentId, {
      ...node,
      hidden: mindmap?.hiddenNodeIds.has(node.id) ?? false,
      data: mindmap && ['mindmap-root', 'mindmap-topic'].includes(node.data.shape)
        ? {
            ...node.data,
            mindmapChildCount: mindmap.childrenByNode.get(node.id)?.length ?? 0,
            onMindMapToggleCollapse: () => toggleMindMapCollapse(node.id)
          }
        : node.data,
      selected: selectedNodeIds.has(node.id),
      dragHandle: node.data.shape === 'activation' ? '.activation-drag-handle' : undefined,
      style: node.type === 'laneNode'
        ? { width: node.width ?? 1120, height: node.height ?? 180 }
        : { width: node.width ?? defaultNodeSize(node).width, height: node.height ?? defaultNodeSize(node).height }
    }));
  }, [document, selectedNodeIds]);
  const flowEdges = useMemo(() => document ? runtimeEdgesForDocument(document).filter((edge) => {
    if (document.kind !== 'mindmap') return true;
    const hidden = analyzeMindMap(document).hiddenNodeIds;
    return !hidden.has(edge.source) && !hidden.has(edge.target);
  }).map((edge, edgeIndex) => {
    const isSequence = document.kind === 'sequence';
    const useSmartRouting = (document.kind === 'architecture'
      || document.kind === 'topology'
      || document.kind === 'flowchart'
      || document.kind === 'swimlane')
      && edge.type !== 'straight'
      && edge.type !== 'bezier';
    const isReturnMessage = isSequence && (edge.data?.lineStyle === 'dashed' || edge.data?.dashed);
    const marker = {
      type: isReturnMessage ? MarkerType.Arrow : MarkerType.ArrowClosed,
      width: isSequence ? 20 : 16,
      height: isSequence ? 20 : 16,
      markerUnits: isSequence ? 'userSpaceOnUse' : 'strokeWidth',
      color: edge.data?.color ?? '#77839A'
    };
    const baseStrokeWidth = edge.data?.strokeWidth ?? (isSequence ? 1.4 : 1.7);
    return {
      ...edge,
      type: isSequence ? 'sequenceMessage' : useSmartRouting ? 'smartOrthogonal' : edge.type,
      data: isSequence
        ? {
            ...edge.data,
            onVerticalMoveStart: beginSequenceEdgeMove,
            onVerticalMove: moveSequenceEdge,
            onVerticalMoveEnd: finishSequenceEdgeMove,
            onSelect: (edgeId: string) => {
              setSelectedNodeIds(new Set());
              setSelectedEdgeId(edgeId);
            }
          }
        : useSmartRouting
          ? {
              ...edge.data,
              routingOffset: routingOffsetForEdge(document, edge, edgeIndex),
              routingObstacles: routingObstaclesForEdge(document, edge)
            }
          : edge.data,
      selected: edge.id === selectedEdgeId,
      markerStart: edge.data?.startMarker === 'arrow' ? marker : undefined,
      markerEnd: edge.data?.endMarker === 'none' ? undefined : marker,
      style: {
        stroke: edge.data?.color ?? '#77839A',
        strokeWidth: selectedEdgeId === edge.id
          ? Math.max(isSequence ? 1.9 : 2.4, baseStrokeWidth)
          : baseStrokeWidth,
        strokeDasharray: edge.data?.lineStyle === 'dotted'
          ? '2 5'
          : edge.data?.lineStyle === 'dashed' || edge.data?.dashed
            ? '8 6'
            : undefined,
        strokeLinecap: 'round'
      },
      labelStyle: { fill: '#465267', fontSize: edge.data?.fontSize ?? 13, fontWeight: 600 },
      labelBgStyle: { fill: 'var(--surface-solid)', fillOpacity: 0.98 },
      labelBgPadding: [7, 5],
      labelBgBorderRadius: 6
    };
  }) : [], [document, selectedEdgeId]);

  const {
    newProjectSheet,
    newDiagramSheet,
    mindmapEditSheet,
    deleteDocumentSheet
  } = renderDiagramStudioSheets({
    newProjectVisible,
    setNewProjectVisible,
    newProjectName,
    setNewProjectName,
    createProject,
    newDiagramVisible,
    setNewDiagramVisible,
    activeProject,
    newDiagramName,
    setNewDiagramName,
    createBlankDiagram,
    mindmapEdit,
    setMindmapEdit,
    document,
    updateNode,
    deleteDocumentTarget,
    isDeletingDocument,
    setDeleteDocumentTarget,
    dirty,
    deleteRequestedDocument
  });

  const storageStatus = <span className={`runtime-badge ${repository?.mode === 'server' ? 'connected' : 'fallback'}`} title={repository?.mode === 'server' ? '项目和图形由本机 Diagram Studio 服务保存' : '当前未连接本地服务，数据仅保存在这个浏览器中'}>
    <i />{repository?.mode === 'server' ? '本地服务' : '浏览器存储'}
  </span>;

  if (homeVisible) return (
    <div className="project-home-shell">
      <header className="home-toolbar">
        <div className="traffic-lights" aria-hidden="true"><i /><i /><i /></div>
        <div className="home-brand"><span className="home-brand-icon"><Icon name="architecture" /></span><strong>Diagram Studio</strong>{storageStatus}</div>
        <div className="home-toolbar-actions">
          <button className="toolbar-button primary" onClick={openNewProjectSheet}><Icon name="plus" />新建项目</button>
        </div>
      </header>
      <main className="project-home">
        <section className="home-intro">
          <div><span className="home-eyebrow">DIAGRAM STUDIO</span><h1>用户项目</h1><p>一个项目可以包含架构图、流程图、泳道图、拓扑图和时序图。</p></div>
          <button className="home-new-button" onClick={openNewProjectSheet}><span><Icon name="plus" /></span><strong>新建用户项目</strong><small>先创建项目，再在项目中创建图</small></button>
        </section>
        <section className="projects-section">
          <div className="projects-heading"><div><h2>所有用户项目</h2><span>{projects.length} 个项目</span></div></div>
          {projects.length > 0 ? <div className="project-grid">
            {projects.map((item) => <button className="project-card" key={item.projectId} onClick={() => void openProject(item.projectId)} aria-label={`打开用户项目 ${item.name}`}>
              <span className="project-card-icon project-folder-icon"><Icon name="folder" /></span>
              <span className="project-card-copy"><strong>{item.name}</strong><small>{item.diagramCount} 张图形</small></span>
              <span className="project-card-date">{formatUpdatedAt(item.updatedAt)}</span>
              <Icon name="chevron" className="project-card-chevron" />
            </button>)}
          </div> : <div className="empty-projects">
            <span><Icon name="folder" /></span>
            <strong>还没有用户项目</strong>
            <p>项目名称由你填写。创建项目后，再进入项目新建具体图形。</p>
            <button className="toolbar-button primary" onClick={openNewProjectSheet}><Icon name="plus" />新建用户项目</button>
          </div>}
        </section>
      </main>
      {newProjectSheet}
      {toast && <div className="toast">{toast}</div>}
    </div>
  );

  if (!document && activeProject) return (
    <div className="project-home-shell">
      <header className="home-toolbar">
        <div className="traffic-lights" aria-hidden="true"><i /><i /><i /></div>
        <div className="home-brand"><button className="icon-button" onClick={goHome} aria-label="返回用户项目列表"><Icon name="home" /></button><strong>{activeProject.name}</strong>{storageStatus}</div>
        <div className="home-toolbar-actions"><button className="toolbar-button primary" onClick={openNewDiagramSheet}><Icon name="plus" />新建图形</button></div>
      </header>
      <main className="project-home project-detail">
        <section className="home-intro">
          <div><span className="home-eyebrow">用户项目</span><h1>{activeProject.name}</h1><p>项目内共有 {activeProjectDocuments.length} 张图形。</p></div>
          <button className="home-new-button" onClick={openNewDiagramSheet}><span><Icon name="plus" /></span><strong>新建图形</strong><small>选择架构图、流程图、泳道图、拓扑图、时序图或思维导图</small></button>
        </section>
        <section className="projects-section">
          <div className="projects-heading"><div><h2>项目图形</h2><span>{activeProjectDocuments.length} 张</span></div></div>
          {activeProjectDocuments.length > 0 ? <div className="diagram-grid">{activeProjectDocuments.map((item) => <div className="diagram-card" key={item.documentId}>
            <button className="diagram-card-open" onClick={() => void openDocument(item.documentId)} aria-label={`打开图形 ${item.title}`}>
              <span className={`project-card-icon kind-${item.kind}`}><Icon name={kindIcon(item.kind)} /></span>
              <span className="project-card-copy"><strong>{item.title}</strong><small><b>{kindLabel(item.kind)}</b> · {item.nodeCount} 个节点 · {item.edgeCount} 条连线</small></span>
              <span className="project-card-date">{formatUpdatedAt(item.updatedAt)}</span>
              <Icon name="chevron" className="project-card-chevron" />
            </button>
            <button className="diagram-card-delete" onClick={() => requestDeleteDocument(item)} aria-label={`删除图形 ${item.title}`} title="删除图形"><Icon name="trash" /></button>
          </div>)}</div> : <div className="empty-projects">
            <span><Icon name="architecture" /></span><strong>这个项目还没有图形</strong><p>先创建一张图，并为它单独命名。</p><button className="toolbar-button primary" onClick={openNewDiagramSheet}><Icon name="plus" />新建图形</button>
          </div>}
        </section>
      </main>
      {newDiagramSheet}
      {deleteDocumentSheet}
      {toast && <div className="toast">{toast}</div>}
    </div>
  );

  if (!document) return <div className="loading-screen"><div className="loading-spinner" /><span>正在打开项目…</span></div>;

  return (
    <div className={`studio-shell ${sidebarVisible ? 'has-sidebar' : ''} ${inspectorVisible ? 'has-inspector' : ''}`}>
      <header className="window-toolbar">
        <div className="traffic-lights" aria-hidden="true"><i /><i /><i /></div>
        <div className="toolbar-leading">
          <button className="icon-button" onClick={requestHome} aria-label="返回项目首页"><Icon name="home" /></button>
          <button className={`icon-button ${sidebarVisible ? 'active' : ''}`} onClick={() => setSidebarVisible(!sidebarVisible)} aria-label="显示或隐藏组件栏"><Icon name="sidebar" /></button>
          <div className="toolbar-separator" />
          <button className="icon-button" onClick={() => setLibraryVisible(!libraryVisible)} aria-label="打开项目内图形列表"><Icon name="folder" /></button>
          <button className={`icon-button ${newDiagramVisible ? 'active' : ''}`} disabled={!activeProject} onClick={openNewDiagramSheet} aria-label="在当前项目中新建图形"><Icon name="plus" /></button>
        </div>
        <div className="document-title-area">
          <div className="project-title-row"><span>{`项目：${activeProject?.name ?? ''}`}</span><i>/</i><input className="document-title" value={document.title} onChange={(event) => commit({ ...document, title: event.target.value })} aria-label="图形名称" /></div>
          <span className={`save-state ${dirty ? 'dirty' : ''}`}>{isSaving ? '正在保存…' : dirty ? '未保存' : `已保存 · v${persistedRevision}`} · {repository?.mode === 'server' ? '本地服务' : '浏览器存储'}</span>
        </div>
        <div className="toolbar-trailing">
          <button className="icon-button" disabled={past.length === 0} onClick={undo} aria-label="撤销"><Icon name="undo" /></button>
          <button className="icon-button" disabled={future.length === 0} onClick={redo} aria-label="重做"><Icon name="redo" /></button>
          <div className="toolbar-separator" />
          <button className="toolbar-button" onClick={() => void autoLayout()}><Icon name="layout" />自动布局</button>
          {supportsPlantUml(document.kind) && <button className={`toolbar-button ${plantUmlVisible ? 'active' : ''}`} onClick={openPlantUmlEditor}><Icon name="document" />PlantUML</button>}
          <button className="toolbar-button primary" disabled={!dirty || isSaving} onClick={() => void save()}><Icon name="save" />保存</button>
          <button className="icon-button destructive-icon" onClick={() => requestDeleteDocument(document)} aria-label="删除当前图形" title="删除当前图形"><Icon name="trash" /></button>
          <div className="export-anchor">
            <button className="toolbar-button" onClick={() => setExportVisible(!exportVisible)}><Icon name="export" />导出<Icon name="chevron" className="chevron" /></button>
            {exportVisible && <div className="popover-menu export-menu">
              <button onClick={() => void exportDiagram('png')}><strong>PNG 图像</strong><small>适合分享和文档</small></button>
              <button onClick={() => void exportDiagram('svg')}><strong>SVG 矢量图</strong><small>适合设计和印刷</small></button>
              {supportsPlantUml(document.kind) && <button onClick={() => void exportDiagram('puml')}><strong>PlantUML 源码</strong><small>标准 .puml，可双向转换</small></button>}
              <button onClick={() => void exportDiagram('json')}><strong>Diagram JSON</strong><small>保留完整可编辑结构</small></button>
            </div>}
          </div>
          <button className={`icon-button ${inspectorVisible ? 'active' : ''}`} onClick={() => setInspectorVisible(!inspectorVisible)} aria-label="显示或隐藏检查器"><Icon name="inspector" /></button>
        </div>
      </header>

      {sidebarVisible && document && <TemplateSidebar diagramKind={document.kind} onAddNode={addNode} sequenceMessagePreset={sequenceMessagePreset} onSequenceMessagePresetChange={setSequenceMessagePreset} />}

      <main className="canvas-workspace" ref={canvasRef}>
        <ReactFlow
          nodes={flowNodes as never}
          edges={flowEdges as never}
          nodeTypes={nodeTypes as never}
          edgeTypes={edgeTypes as never}
          connectionMode={ConnectionMode.Loose}
          connectionRadius={22}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onConnect={onConnect}
          onConnectEnd={onConnectEnd}
          onNodeDoubleClick={(_event, node) => {
            const resolved = document.nodes.find((candidate) => candidate.id === node.id);
            if (resolved && document.kind === 'mindmap' && ['mindmap-root', 'mindmap-topic'].includes(resolved.data.shape)) setMindmapEdit({ nodeId: resolved.id, value: resolved.data.label });
          }}
          onNodeDragStart={onDragStart}
          onNodeDragStop={onDragStop}
          onDragOver={(event) => { event.preventDefault(); event.dataTransfer.dropEffect = 'copy'; }}
          onDrop={onCanvasDrop}
          onPaneClick={() => { setSelectedNodeIds(new Set()); setSelectedEdgeId(undefined); }}
          selectionOnDrag={false}
          selectionMode={SelectionMode.Partial}
          selectionKeyCode="Shift"
          multiSelectionKeyCode="Shift"
          panOnDrag={[0, 1, 2]}
          minZoom={0.15}
          maxZoom={2.5}
          fitView
          snapToGrid
          snapGrid={[12, 12]}
          defaultEdgeOptions={{ type: 'smoothstep' }}
          deleteKeyCode={null}
          colorMode="system"
        >
          <Background variant={BackgroundVariant.Dots} gap={18} size={1.25} color="var(--grid-dot)" />
          {document.nodes.length === 0 && <div className="canvas-empty-hint"><span><Icon name={document.kind === 'mindmap' ? 'mindmap' : kindIcon(document.kind)} /></span><strong>{document.kind === 'mindmap' ? '先添加中心主题' : '空白画布'}</strong><p>{document.kind === 'mindmap' ? '从左侧拖入“中心主题”，再从主题两侧拉出分支，松到空白处即可创建下级主题。' : '从左侧组件库拖入元素开始绘制。'}</p></div>}
          <MiniMap className="apple-minimap" pannable zoomable nodeColor={(node) => (node.data as unknown as DiagramNode['data'])?.color ?? '#7D8797'} />
          <Controls className="apple-controls" showInteractive={false} />
          <div className="canvas-status">
            <span>拖动空白移动画布 · Shift 拖动框选</span><i />
            <span>{document.kind === 'mindmap' ? '从主题拉线到空白处可创建子主题' : kindLabel(document.kind)}</span><i />
            <span>{document.nodes.filter((node) => node.type !== 'laneNode').length} 个节点</span><i />
            <span>{document.edges.length} 条连线</span>
          </div>
        </ReactFlow>
      </main>

      {inspectorVisible && <Inspector node={selectedNode} selectedNodeCount={selectedNodes.length} edge={selectedEdge} onUpdateNode={updateNode} onUpdateEdge={updateEdge} onChangeNodeLayer={changeNodeLayer} onDelete={deleteSelection} onClose={() => setInspectorVisible(false)} />}

      {libraryVisible && <div className="library-popover popover-menu">
        <div className="popover-heading"><strong>{`${activeProject?.name ?? ''} · 图形`}</strong><button className="icon-button subtle" onClick={() => setLibraryVisible(false)}><Icon name="close" /></button></div>
        <div className="document-list">
          {activeProjectDocuments.map((item) => <div key={item.documentId} className={`document-list-item ${item.documentId === document.documentId ? 'active' : ''}`}>
            <button className="document-list-open" onClick={() => void openDocument(item.documentId)}>
              <span className="document-kind-icon"><Icon name={kindIcon(item.kind)} /></span>
              <span><strong>{item.title}</strong><small>{kindLabel(item.kind)} · {item.nodeCount} 个节点 · v{item.revision}</small></span>
            </button>
            <button className="document-list-delete" onClick={() => requestDeleteDocument(item)} aria-label={`删除图形 ${item.title}`} title="删除图形"><Icon name="trash" /></button>
          </div>)}
        </div>
        <div className="popover-footer">
          <button onClick={() => importInputRef.current?.click()}><Icon name="folder" />导入 JSON 或 PlantUML</button>
          <input ref={importInputRef} hidden type="file" accept=".json,.diagram.json,.puml,.plantuml,.pu,application/json,text/plain,text/vnd.plantuml" onChange={(event) => { const file = event.target.files?.[0]; if (file) void importDiagram(file); event.currentTarget.value = ''; }} />
        </div>
      </div>}

      {newDiagramSheet}
      {mindmapEditSheet}

      {plantUmlVisible && <div className="sheet-backdrop plantuml-backdrop">
        <section className="plantuml-sheet" role="dialog" aria-modal="true" aria-labelledby="plantuml-source-title">
          <div className="sheet-heading plantuml-heading">
            <div><strong id="plantuml-source-title">PlantUML 源码</strong><span>{plantUmlDescription(document.kind)}</span></div>
            <button className="icon-button subtle" onClick={() => setPlantUmlVisible(false)} aria-label="关闭 PlantUML 源码"><Icon name="close" /></button>
          </div>
          <div className="plantuml-editor-body">
            <div className="plantuml-editor-toolbar"><span>{`${plantUmlDialectLabel(document.kind)} · .PUML`}</span><button onClick={() => { setPlantUmlSource(diagramToPlantUml(document)); setPlantUmlError(undefined); }}>重新从画布生成</button></div>
            <textarea
              autoFocus
              spellCheck={false}
              value={plantUmlSource}
              onChange={(event) => { setPlantUmlSource(event.target.value); setPlantUmlError(undefined); }}
              aria-label="PlantUML 源码"
            />
            {plantUmlError && <div className="plantuml-error" role="alert">{plantUmlError}</div>}
          </div>
          <div className="plantuml-footer">
            <p>{plantUmlEditorHint(document.kind)}</p>
            <div><button className="toolbar-button" onClick={() => setPlantUmlVisible(false)}>取消</button><button className="toolbar-button primary" onClick={applyPlantUmlSource}>应用到画布</button></div>
          </div>
        </section>
      </div>}

      {homeConfirmVisible && <div className="sheet-backdrop">
        <section className="confirm-sheet" role="alertdialog" aria-modal="true" aria-labelledby="home-confirm-title">
          <span className="confirm-icon"><Icon name="home" /></span>
          <div><strong id="home-confirm-title">返回项目首页？</strong><p>“{document.title}”还有未保存的修改。</p></div>
          <div className="confirm-actions">
            <button className="toolbar-button" onClick={() => setHomeConfirmVisible(false)}>取消</button>
            <button className="toolbar-button destructive-text" onClick={goHome}>不保存</button>
            <button className="toolbar-button primary" onClick={() => void saveAndGoHome()}>保存并返回</button>
          </div>
        </section>
      </div>}

      {deleteDocumentSheet}

      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}
