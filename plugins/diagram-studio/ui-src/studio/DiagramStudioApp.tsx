import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  BaseEdge,
  Background,
  BackgroundVariant,
  ConnectionMode,
  Controls,
  MarkerType,
  MiniMap,
  ReactFlow,
  addEdge,
  applyEdgeChanges,
  applyNodeChanges,
  useReactFlow,
  type Connection,
  type EdgeChange,
  type EdgeProps,
  type NodeChange
} from '@xyflow/react';
import { toPng, toSvg } from 'html-to-image';
import type { DiagramDocument, DiagramEdge, DiagramKind, DiagramNode, DiagramProject, DiagramProjectSummary } from '../../src/schema';
import { layoutDiagram } from '../../src/layout';
import { detectPlantUmlDiagramKind, diagramToPlantUml, plantUmlToDiagram } from '../../src/plantuml';
import {
  parseSequenceActivationHandle,
  parseSequenceSlot,
  sequenceActivationHandleId,
  sequenceActivationSlotCount,
  sequenceActivationSlotPercentage,
  sequenceLifelineSlotCount,
  sequenceSlotPercentage,
  type SequenceActivationSide
} from '../../src/sequence';
import { diagramTypeCatalog } from '../../src/templates';
import { createRepository, type DiagramSummary } from './repository';
import { DiagramNodeView, LaneNodeView } from './DiagramNodes';
import { componentDragType, TemplateSidebar, type PaletteItem, type SequenceMessagePreset } from './TemplateSidebar';
import { Inspector } from './Inspector';
import { Icon } from './Icons';

const nodeTypes = { diagramNode: DiagramNodeView, laneNode: LaneNodeView };
const edgeTypes = { sequenceMessage: SequenceMessageEdge };

type Repository = Awaited<ReturnType<typeof createRepository>>;

export function DiagramStudioApp() {
  const reactFlow = useReactFlow();
  const canvasRef = useRef<HTMLDivElement>(null);
  const importInputRef = useRef<HTMLInputElement>(null);
  const dragSnapshot = useRef<DiagramDocument | null>(null);
  const resizeSnapshot = useRef<DiagramDocument | null>(null);
  const edgeMoveSnapshot = useRef<DiagramDocument | null>(null);
  const edgeMoveChanged = useRef(false);
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
  const [homeConfirmVisible, setHomeConfirmVisible] = useState(false);
  const [exportVisible, setExportVisible] = useState(false);
  const [plantUmlVisible, setPlantUmlVisible] = useState(false);
  const [plantUmlSource, setPlantUmlSource] = useState('');
  const [plantUmlError, setPlantUmlError] = useState<string>();
  const [selectedNodeId, setSelectedNodeId] = useState<string>();
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
      setDocuments(items);
      setProjects(await repo.listProjects());
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
      } else if ((event.key === 'Backspace' || event.key === 'Delete') && (selectedNodeId || selectedEdgeId)) {
        event.preventDefault(); deleteSelection();
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
          dialect: normalizedDocument.kind === 'sequence'
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

  function openResolvedDocument(next: DiagramDocument) {
    setDocument(next);
    setHomeVisible(false);
    setPersistedRevision(next.revision);
    setDirty(false);
    setPast([]);
    setFuture([]);
    setSelectedNodeId(undefined);
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
    setSelectedNodeId(undefined);
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

  function onNodesChange(changes: NodeChange[]) {
    if (!document) return;
    const resizeChanges = changes.filter((change): change is Extract<NodeChange, { type: 'dimensions' }> =>
      change.type === 'dimensions' && change.resizing !== undefined
    );
    if (resizeChanges.some((change) => change.resizing) && !resizeSnapshot.current) {
      resizeSnapshot.current = structuredClone(document);
    }
    const structuralChanges = changes.filter((change) =>
      change.type !== 'select' && (change.type !== 'dimensions' || change.resizing !== undefined)
    );
    if (structuralChanges.length === 0) return;
    const changedNodes = applyNodeChanges(structuralChanges, document.nodes as never) as unknown as DiagramNode[];
    const nextNodes = changedNodes.map((node) => {
      if (node.data.shape !== 'activation' || !node.parentId) return node;
      const previous = document.nodes.find((candidate) => candidate.id === node.id);
      return previous ? { ...node, position: { ...node.position, x: previous.position.x } } : node;
    });
    const next = { ...document, nodes: nextNodes };
    updateDocumentLive(next);
    if (resizeChanges.some((change) => change.resizing === false) && resizeSnapshot.current) {
      setPast((items) => [...items.slice(-39), resizeSnapshot.current!]);
      setFuture([]);
      resizeSnapshot.current = null;
    }
  }

  function onEdgesChange(changes: EdgeChange[]) {
    if (!document) return;
    const structuralChanges = changes.filter((change) => change.type !== 'select');
    if (structuralChanges.length === 0) return;
    updateDocumentLive({ ...document, edges: applyEdgeChanges(structuralChanges, document.edges as never) as unknown as DiagramEdge[] });
  }

  function onConnect(connection: Connection) {
    if (!document || !connection.source || !connection.target) return;
    if (document.kind === 'sequence') {
      const now = Date.now();
      const sourceSlot = parseSequenceSlot(connection.sourceHandle);
      const targetSlot = parseSequenceSlot(connection.targetHandle);
      const previous = lastSequenceConnect.current;
      const repeatedDrag = previous
        && now - previous.at < 250
        && previous.source === connection.source
        && previous.target === connection.target
        && Math.abs((previous.sourceSlot ?? -100) - (sourceSlot ?? 100)) <= 2
        && Math.abs((previous.targetSlot ?? -100) - (targetSlot ?? 100)) <= 2;
      if (repeatedDrag) return;
      lastSequenceConnect.current = { source: connection.source, target: connection.target, sourceSlot, targetSlot, at: now };
    }
    const newEdge: DiagramEdge = {
      id: `edge-${crypto.randomUUID().slice(0, 8)}`,
      source: connection.source,
      target: connection.target,
      sourceHandle: connection.sourceHandle ?? undefined,
      targetHandle: connection.targetHandle ?? undefined,
      type: document.kind === 'sequence' ? 'straight' : 'smoothstep',
      data: {
        lineStyle: document.kind === 'sequence' && sequenceMessagePreset === 'return' ? 'dashed' : 'solid',
        startMarker: 'none',
        endMarker: 'arrow',
        strokeWidth: document.kind === 'sequence' ? 1.4 : 1.7,
        fontSize: 13
      }
    };
    let nextNodes = document.nodes;
    if (document.kind === 'sequence') {
      nextNodes = [...document.nodes];
      const messageY = sequenceEndpointY(document.nodes, connection.source, connection.sourceHandle)
        ?? sequenceEndpointY(document.nodes, connection.target, connection.targetHandle);
      const endpoints = [
        { role: 'source' as const, nodeId: connection.source, handleId: connection.sourceHandle },
        { role: 'target' as const, nodeId: connection.target, handleId: connection.targetHandle }
      ];
      for (const endpoint of endpoints) {
        const lifeline = nextNodes.find((node) => node.id === endpoint.nodeId);
        let slot = parseSequenceSlot(endpoint.handleId);
        if (lifeline?.data.shape === 'lifeline' && messageY !== undefined) {
          slot = closestLifelineSlot(lifeline, nextNodes, messageY);
          endpoint.handleId = `slot-${slot}`;
        }
        if (lifeline?.data.shape !== 'lifeline' || slot === undefined) continue;
        let activation = findActivationAt(nextNodes, lifeline, slot);
        if (!activation && sequenceMessagePreset === 'call') {
          const lifelineWidth = lifeline.width ?? 160;
          const lifelineHeight = lifeline.height ?? 560;
          const activationHeight = 96;
          activation = {
            id: `activation-${crypto.randomUUID().slice(0, 8)}`,
            type: 'diagramNode',
            parentId: lifeline.id,
            extent: 'parent',
            position: {
              x: lifelineWidth / 2 - 7,
              y: Math.min(lifelineHeight - activationHeight, lifelineHeight * sequenceSlotPercentage(slot) / 100)
            },
            width: 14,
            height: activationHeight,
            zIndex: 4,
            data: {
              label: '激活条',
              category: 'process',
              shape: 'activation',
              color: lifeline.data.color ?? '#4E7CC7',
              borderColor: lifeline.data.borderColor ?? lifeline.data.color ?? '#4E7CC7',
              fillColor: lifeline.data.fillColor ?? '#E8F1FF',
              showLabel: false,
              sequenceOwnerId: lifeline.id,
              sequenceSlot: slot
            }
          };
          nextNodes.push(activation);
        }
        if (activation) endpoint.nodeId = activation.id;
      }

      const sourceNode = nextNodes.find((node) => node.id === endpoints[0].nodeId);
      const targetNode = nextNodes.find((node) => node.id === endpoints[1].nodeId);
      if (sourceNode && targetNode) {
        const sourcePosition = absoluteNodePosition(nextNodes, sourceNode);
        const targetPosition = absoluteNodePosition(nextNodes, targetNode);
        const sourceWidth = sourceNode.width ?? defaultNodeSize(sourceNode).width;
        const targetWidth = targetNode.width ?? defaultNodeSize(targetNode).width;
        const sourceCenterX = sourcePosition.x + sourceWidth / 2;
        const targetCenterX = targetPosition.x + targetWidth / 2;
        const sourceSide: SequenceActivationSide = sourceCenterX <= targetCenterX ? 'right' : 'left';
        const targetSide: SequenceActivationSide = sourceSide === 'right' ? 'left' : 'right';
        for (const [endpoint, node, side] of [
          [endpoints[0], sourceNode, sourceSide],
          [endpoints[1], targetNode, targetSide]
        ] as const) {
          if (node.data.shape !== 'activation') continue;
          endpoint.handleId = closestActivationHandle(node, nextNodes, side, messageY);
        }
      }
      newEdge.source = endpoints[0].nodeId;
      newEdge.sourceHandle = endpoints[0].handleId ?? undefined;
      newEdge.target = endpoints[1].nodeId;
      newEdge.targetHandle = endpoints[1].handleId ?? undefined;
    }
    commit({
      ...document,
      nodes: nextNodes,
      edges: addEdge(newEdge as never, document.edges as never) as unknown as DiagramEdge[]
    });
  }

  function addNode(item: PaletteItem, droppedPosition?: { x: number; y: number }) {
    if (!document) return;
    const position = droppedPosition ?? reactFlow.screenToFlowPosition({ x: window.innerWidth / 2, y: window.innerHeight / 2 });
    const initialSize = newComponentSize(item);
    const newNode: DiagramNode = {
      id: `${item.category}-${crypto.randomUUID().slice(0, 8)}`,
      type: item.shape === 'lane' ? 'laneNode' : 'diagramNode',
      position,
      width: initialSize.width,
      height: initialSize.height,
      data: {
        label: item.label,
        category: item.category,
        shape: item.shape,
        color: item.color,
        borderColor: item.color,
        borderStyle: item.borderStyle,
        fillColor: item.fillColor,
        icon: item.icon,
        showLabel: item.showLabel ?? item.shape === 'text',
        fontSize: item.shape === 'text' ? 16 : 14,
        fontWeight: item.shape === 'text' ? 500 : 650
      }
    };
    commit({ ...document, nodes: [...document.nodes, newNode] });
    setSelectedNodeId(newNode.id);
    setSelectedEdgeId(undefined);
    setInspectorVisible(true);
  }

  function onCanvasDrop(event: React.DragEvent) {
    event.preventDefault();
    const payload = event.dataTransfer.getData(componentDragType);
    if (!payload) return;
    try {
      const item = JSON.parse(payload) as PaletteItem;
      if (!item.id || !item.label || !item.category || !item.shape || !item.color) throw new Error('invalid component');
      addNode(item, reactFlow.screenToFlowPosition({ x: event.clientX, y: event.clientY }));
    } catch {
      showToast('无法添加这个组件。');
    }
  }

  function updateNode(nextNode: DiagramNode) {
    if (!document) return;
    commit({ ...document, nodes: document.nodes.map((node) => node.id === nextNode.id ? nextNode : node) });
  }

  function updateEdge(nextEdge: DiagramEdge) {
    if (!document) return;
    commit({ ...document, edges: document.edges.map((edge) => edge.id === nextEdge.id ? nextEdge : edge) });
  }

  function deleteSelection() {
    if (!document) return;
    if (selectedNodeId) {
      const removedNodeIds = new Set([selectedNodeId]);
      let foundChild = true;
      while (foundChild) {
        foundChild = false;
        for (const node of document.nodes) {
          if (node.parentId && removedNodeIds.has(node.parentId) && !removedNodeIds.has(node.id)) {
            removedNodeIds.add(node.id);
            foundChild = true;
          }
        }
      }
      commit({
        ...document,
        nodes: document.nodes.filter((node) => !removedNodeIds.has(node.id)),
        edges: document.edges.filter((edge) => !removedNodeIds.has(edge.source) && !removedNodeIds.has(edge.target))
      });
    } else if (selectedEdgeId) {
      commit({ ...document, edges: document.edges.filter((edge) => edge.id !== selectedEdgeId) });
    }
    setSelectedNodeId(undefined);
    setSelectedEdgeId(undefined);
  }

  function onDragStart() {
    if (document && !dragSnapshot.current) dragSnapshot.current = structuredClone(document);
  }

  function onDragStop() {
    if (!document || !dragSnapshot.current) return;
    setPast((items) => [...items.slice(-39), dragSnapshot.current!]);
    setFuture([]);
    dragSnapshot.current = null;
    setDirty(true);
  }

  function beginSequenceEdgeMove(edgeId: string) {
    if (!document) return;
    if (!edgeMoveSnapshot.current) edgeMoveSnapshot.current = structuredClone(document);
    edgeMoveChanged.current = false;
    setSelectedNodeId(undefined);
    setSelectedEdgeId(edgeId);
  }

  function moveSequenceEdge(edgeId: string, clientY: number) {
    if (!document) return;
    const edge = document.edges.find((candidate) => candidate.id === edgeId);
    if (!edge) return;
    const flowY = reactFlow.screenToFlowPosition({ x: 0, y: clientY }).y;
    const sourceNode = document.nodes.find((node) => node.id === edge.source);
    const targetNode = document.nodes.find((node) => node.id === edge.target);
    if (!sourceNode || !targetNode) return;
    const sourceHandle = closestSequenceHandleAtY(sourceNode, document.nodes, edge.sourceHandle, flowY);
    const targetHandle = closestSequenceHandleAtY(targetNode, document.nodes, edge.targetHandle, flowY);
    if (sourceHandle === edge.sourceHandle && targetHandle === edge.targetHandle) return;
    edgeMoveChanged.current = true;
    updateDocumentLive({
      ...document,
      edges: document.edges.map((candidate) => candidate.id === edgeId
        ? { ...candidate, sourceHandle, targetHandle }
        : candidate)
    });
  }

  function finishSequenceEdgeMove() {
    const snapshot = edgeMoveSnapshot.current;
    if (snapshot && edgeMoveChanged.current) {
      setPast((items) => [...items.slice(-39), snapshot]);
      setFuture([]);
      setDirty(true);
    }
    edgeMoveSnapshot.current = null;
    edgeMoveChanged.current = false;
  }

  const selectNode = useCallback((_event: React.MouseEvent, node: { id: string }) => {
    setSelectedNodeId((current) => current === node.id ? current : node.id);
    setSelectedEdgeId(undefined);
  }, []);

  const selectEdge = useCallback((_event: React.MouseEvent, edge: { id: string }) => {
    setSelectedNodeId(undefined);
    setSelectedEdgeId((current) => current === edge.id ? current : edge.id);
  }, []);

  function openPlantUmlEditor() {
    if (!document) return;
    if (!supportsPlantUml(document.kind)) {
      showToast('当前图形类型尚未接入 PlantUML 双向编辑。');
      return;
    }
    try {
      setPlantUmlSource(diagramToPlantUml(document));
      setPlantUmlError(undefined);
      setPlantUmlVisible(true);
      setExportVisible(false);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function applyPlantUmlSource() {
    if (!document) return;
    if (!supportsPlantUml(document.kind)) {
      setPlantUmlError('当前图形类型尚未接入 PlantUML 双向转换。');
      return;
    }
    try {
      const next = plantUmlToDiagram(plantUmlSource, {
        documentId: document.documentId,
        title: document.title,
        revision: document.revision,
        createdAt: document.createdAt,
        updatedAt: document.updatedAt,
        kind: document.kind
      });
      commit(next);
      setPlantUmlError(undefined);
      setPlantUmlVisible(false);
      window.setTimeout(() => reactFlow.fitView({ padding: 0.16, duration: 320 }), 60);
      showToast('已将 PlantUML 应用到画布');
    } catch (error) {
      setPlantUmlError(error instanceof Error ? error.message : String(error));
    }
  }

  async function exportDiagram(format: 'json' | 'svg' | 'png' | 'puml') {
    if (!document) return;
    setExportVisible(false);
    try {
      if (format === 'puml') {
        downloadBlob(new Blob([diagramToPlantUml(document)], { type: 'text/vnd.plantuml;charset=utf-8' }), `${safeFileName(document.title)}.puml`);
      } else if (format === 'json') {
        downloadBlob(new Blob([JSON.stringify(document, null, 2)], { type: 'application/vnd.chatos.diagram+json' }), `${safeFileName(document.title)}.diagram.json`);
      } else {
        const target = canvasRef.current?.querySelector('.react-flow') as HTMLElement | null;
        if (!target) throw new Error('画布尚未准备好。');
        const dataUrl = format === 'png'
          ? await toPng(target, { backgroundColor: resolvedCanvasColor(), pixelRatio: 2, cacheBust: true })
          : await toSvg(target, { backgroundColor: resolvedCanvasColor(), cacheBust: true });
        downloadDataUrl(dataUrl, `${safeFileName(document.title)}.${format}`);
      }
      showToast(format === 'puml' ? '已导出 PlantUML' : `已导出 ${format.toUpperCase()}`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function importDiagram(file: File) {
    if (!repository || !activeProject) {
      showToast('请先进入一个项目，再导入图形。');
      return;
    }
    try {
      const text = await file.text();
      const isPlantUml = /\.(puml|plantuml|pu)$/i.test(file.name) || /^\s*@startuml\b/im.test(text);
      if (isPlantUml) {
        const detectedKind = detectPlantUmlDiagramKind(text);
        const fallbackTitle = file.name.replace(/\.(puml|plantuml|pu)$/i, '').trim() || `导入的${kindLabel(detectedKind)}`;
        const seed = await repository.createInProject(activeProject.projectId, detectedKind, fallbackTitle);
        const imported = plantUmlToDiagram(text, {
          documentId: seed.documentId,
          title: fallbackTitle,
          revision: seed.revision,
          createdAt: seed.createdAt,
          updatedAt: seed.updatedAt,
          kind: detectedKind
        });
        openResolvedDocument(imported);
        setDirty(true);
        await saveImported(repository, imported, seed.revision);
        setActiveProject(await repository.readProject(activeProject.projectId));
        await refreshProjects(repository);
        window.setTimeout(() => reactFlow.fitView({ padding: 0.16, duration: 320 }), 60);
        return;
      }
      const imported = JSON.parse(text) as DiagramDocument;
      if (!imported.kind || !Array.isArray(imported.nodes) || !Array.isArray(imported.edges)) throw new Error('文件不是有效的 Diagram Studio 文档。');
      const seed = await repository.createInProject(activeProject.projectId, imported.kind, imported.title);
      const next = {
        ...imported,
        documentId: seed.documentId,
        revision: seed.revision,
        createdAt: seed.createdAt,
        updatedAt: seed.updatedAt,
        title: `${imported.title || '导入的图'} 副本`
      };
      openResolvedDocument(next);
      setDirty(true);
      await saveImported(repository, next, seed.revision);
      setActiveProject(await repository.readProject(activeProject.projectId));
      await refreshProjects(repository);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function saveImported(repo: Repository, imported: DiagramDocument, revision: number) {
    const saved = await repo.save(imported, revision);
    openResolvedDocument(saved);
    await refreshDocuments(repo);
    showToast('已导入');
  }

  const selectedNode = document?.nodes.find((node) => node.id === selectedNodeId);
  const selectedEdge = document?.edges.find((edge) => edge.id === selectedEdgeId);
  const activeProjectDocuments = useMemo(() => {
    const ids = new Set(activeProject?.diagramIds ?? []);
    return documents.filter((item) => ids.has(item.documentId));
  }, [activeProject?.diagramIds, documents]);
  const flowNodes = useMemo(() => document?.nodes.map((node) => ({
    ...node,
    selected: node.id === selectedNodeId,
    dragHandle: node.data.shape === 'activation' ? '.activation-drag-handle' : undefined,
    style: node.type === 'laneNode'
      ? { width: node.width ?? 1120, height: node.height ?? 180 }
      : { width: node.width ?? defaultNodeSize(node).width, height: node.height ?? defaultNodeSize(node).height }
  })) ?? [], [document?.nodes, selectedNodeId]);
  const flowEdges = useMemo(() => document?.edges.map((edge) => {
    const isSequence = document.kind === 'sequence';
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
      type: isSequence ? 'sequenceMessage' : edge.type,
      data: isSequence ? {
        ...edge.data,
        onVerticalMoveStart: beginSequenceEdgeMove,
        onVerticalMove: moveSequenceEdge,
        onVerticalMoveEnd: finishSequenceEdgeMove,
        onSelect: (edgeId: string) => {
          setSelectedNodeId(undefined);
          setSelectedEdgeId(edgeId);
        }
      } : edge.data,
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
      labelBgStyle: { fill: 'var(--surface)', fillOpacity: 0.95 }
    };
  }) ?? [], [document?.edges, document?.kind, document?.nodes, selectedEdgeId]);

  const newProjectSheet = newProjectVisible && <div className="sheet-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) setNewProjectVisible(false); }}>
    <section className="new-project-sheet" role="dialog" aria-modal="true" aria-labelledby="new-project-title">
      <div className="sheet-heading">
        <div><strong id="new-project-title">新建用户项目</strong><span>项目用于归类和管理多张图形</span></div>
        <button className="icon-button subtle" onClick={() => setNewProjectVisible(false)} aria-label="关闭新建项目"><Icon name="close" /></button>
      </div>
      <div className="project-name-section">
        <label htmlFor="new-project-name">项目名称</label>
        <input id="new-project-name" autoFocus value={newProjectName} onChange={(event) => setNewProjectName(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter' && newProjectName.trim()) void createProject(); }} placeholder="例如：Chatos 客户端" maxLength={240} />
      </div>
      <div className="project-create-footer"><button className="toolbar-button" onClick={() => setNewProjectVisible(false)}>取消</button><button className="toolbar-button primary" disabled={!newProjectName.trim()} onClick={() => void createProject()}>创建项目</button></div>
    </section>
  </div>;

  const newDiagramSheet = newDiagramVisible && <div className="sheet-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) setNewDiagramVisible(false); }}>
    <section className="new-diagram-sheet" role="dialog" aria-modal="true" aria-labelledby="new-diagram-title">
      <div className="sheet-heading">
        <div><strong id="new-diagram-title">新建图形</strong><span>图形将保存在“{activeProject?.name}”项目中</span></div>
        <button className="icon-button subtle" onClick={() => setNewDiagramVisible(false)} aria-label="关闭新建图形"><Icon name="close" /></button>
      </div>
      <div className="project-name-section">
        <label htmlFor="new-diagram-name">图形名称</label>
        <input id="new-diagram-name" autoFocus value={newDiagramName} onChange={(event) => setNewDiagramName(event.target.value)} placeholder="例如：登录认证时序" maxLength={240} />
      </div>
      <div className="template-section-heading"><strong>选择图形类型</strong><span>创建后进入空白画布</span></div>
      <div className="template-grid">
        {diagramTypeCatalog.map((diagramType) => (
          <button key={diagramType.kind} disabled={!newDiagramName.trim()} onClick={() => void createBlankDiagram(diagramType.kind)}>
            <span className={`new-template-icon kind-${diagramType.kind}`}><Icon name={kindIcon(diagramType.kind)} /></span>
            <span><strong>{diagramType.title}</strong><small>{diagramType.subtitle}</small></span>
            <Icon name="chevron" className="template-chevron" />
          </button>
        ))}
      </div>
      <p className="sheet-footnote">只设置图形类型，不会自动生成任何节点、文字或连线。</p>
    </section>
  </div>;

  if (!isReady) return <div className="loading-screen"><div className="loading-spinner" /><span>正在准备 Diagram Studio…</span></div>;

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
          <button className="home-new-button" onClick={openNewDiagramSheet}><span><Icon name="plus" /></span><strong>新建图形</strong><small>选择架构图、流程图、泳道图、拓扑图或时序图</small></button>
        </section>
        <section className="projects-section">
          <div className="projects-heading"><div><h2>项目图形</h2><span>{activeProjectDocuments.length} 张</span></div></div>
          {activeProjectDocuments.length > 0 ? <div className="diagram-grid">{activeProjectDocuments.map((item) => <button className="diagram-card" key={item.documentId} onClick={() => void openDocument(item.documentId)} aria-label={`打开图形 ${item.title}`}>
            <span className={`project-card-icon kind-${item.kind}`}><Icon name={kindIcon(item.kind)} /></span>
            <span className="project-card-copy"><strong>{item.title}</strong><small><b>{kindLabel(item.kind)}</b> · {item.nodeCount} 个节点 · {item.edgeCount} 条连线</small></span>
            <span className="project-card-date">{formatUpdatedAt(item.updatedAt)}</span>
            <Icon name="chevron" className="project-card-chevron" />
          </button>)}</div> : <div className="empty-projects">
            <span><Icon name="architecture" /></span><strong>这个项目还没有图形</strong><p>先创建一张图，并为它单独命名。</p><button className="toolbar-button primary" onClick={openNewDiagramSheet}><Icon name="plus" />新建图形</button>
          </div>}
        </section>
      </main>
      {newDiagramSheet}
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
          onNodeDragStart={onDragStart}
          onNodeDragStop={onDragStop}
          onNodeClick={selectNode}
          onEdgeClick={selectEdge}
          onDragOver={(event) => { event.preventDefault(); event.dataTransfer.dropEffect = 'copy'; }}
          onDrop={onCanvasDrop}
          onPaneClick={() => { setSelectedNodeId(undefined); setSelectedEdgeId(undefined); }}
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
          <MiniMap className="apple-minimap" pannable zoomable nodeColor={(node) => (node.data as unknown as DiagramNode['data'])?.color ?? '#7D8797'} />
          <Controls className="apple-controls" showInteractive={false} />
          <div className="canvas-status">
            <span>{kindLabel(document.kind)}</span><i />
            <span>{document.nodes.filter((node) => node.type !== 'laneNode').length} 个节点</span><i />
            <span>{document.edges.length} 条连线</span>
          </div>
        </ReactFlow>
      </main>

      {inspectorVisible && <Inspector node={selectedNode} edge={selectedEdge} onUpdateNode={updateNode} onUpdateEdge={updateEdge} onDelete={deleteSelection} onClose={() => setInspectorVisible(false)} />}

      {libraryVisible && <div className="library-popover popover-menu">
        <div className="popover-heading"><strong>{`${activeProject?.name ?? ''} · 图形`}</strong><button className="icon-button subtle" onClick={() => setLibraryVisible(false)}><Icon name="close" /></button></div>
        <div className="document-list">
          {activeProjectDocuments.map((item) => <button key={item.documentId} className={item.documentId === document.documentId ? 'active' : ''} onClick={() => void openDocument(item.documentId)}>
            <span className="document-kind-icon"><Icon name={kindIcon(item.kind)} /></span>
            <span><strong>{item.title}</strong><small>{kindLabel(item.kind)} · {item.nodeCount} 个节点 · v{item.revision}</small></span>
          </button>)}
        </div>
        <div className="popover-footer">
          <button onClick={() => importInputRef.current?.click()}><Icon name="folder" />导入 JSON 或 PlantUML</button>
          <input ref={importInputRef} hidden type="file" accept=".json,.diagram.json,.puml,.plantuml,.pu,application/json,text/plain,text/vnd.plantuml" onChange={(event) => { const file = event.target.files?.[0]; if (file) void importDiagram(file); event.currentTarget.value = ''; }} />
        </div>
      </div>}

      {newDiagramSheet}

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

      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}

function kindLabel(kind: DiagramKind): string {
  return ({ architecture: '架构图', flowchart: '流程图', swimlane: '泳道图', topology: '拓扑图', sequence: '时序图' })[kind];
}

function supportsPlantUml(_kind: DiagramKind): boolean {
  return true;
}

function plantUmlDialectLabel(kind: DiagramKind): string {
  return kind === 'sequence'
    ? 'SEQUENCE'
    : kind === 'swimlane'
      ? 'ACTIVITY · PARTITION'
      : kind === 'architecture'
        ? 'COMPONENT'
        : kind === 'topology'
          ? 'DEPLOYMENT'
          : 'ACTIVITY';
}

function plantUmlDescription(kind: DiagramKind): string {
  return kind === 'sequence'
    ? '时序语义与当前画布双向转换'
    : kind === 'swimlane'
      ? 'Activity Partition 与泳道画布双向转换'
      : kind === 'architecture'
        ? 'Component Diagram 与架构画布双向转换'
        : kind === 'topology'
          ? 'Deployment Diagram 与拓扑画布双向转换'
          : 'Activity Diagram 与流程画布双向转换';
}

function plantUmlEditorHint(kind: DiagramKind): string {
  return kind === 'sequence'
    ? '修改参与者、消息、激活和组合片段后应用。外部 PlantUML 没有布局信息时会自动排版。'
    : kind === 'swimlane'
      ? '修改泳道、活动和判断分支后应用。partition 或 |泳道| 语法会生成可拖拽的泳道结构。'
      : kind === 'architecture'
        ? '修改 actor、component、interface、database、queue 和依赖关系后应用。外部源码会转换成可拖拽的架构组件。'
        : kind === 'topology'
          ? '修改 node、cloud、database、storage、artifact 和网络关系后应用。外部源码会转换成可编辑的拓扑节点。'
          : '修改活动、判断与分支后应用。start、if/else/endif 和 stop 会生成对应的可编辑流程组件。';
}

function kindIcon(kind: DiagramKind): 'architecture' | 'flowchart' | 'swimlane' | 'topology' | 'sequence' {
  return kind;
}

function formatUpdatedAt(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '最近更新';
  const today = new Date();
  const sameDay = date.getFullYear() === today.getFullYear()
    && date.getMonth() === today.getMonth()
    && date.getDate() === today.getDate();
  return sameDay
    ? `今天 ${date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}`
    : date.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
}

function defaultNodeSize(node: DiagramNode): { width: number; height: number } {
  if (node.data.shape === 'lifeline') return { width: 160, height: 560 };
  if (node.data.shape === 'activation') return { width: 14, height: 120 };
  if (node.data.shape === 'fragment') return { width: 620, height: 220 };
  if (node.data.shape === 'lane') return { width: 900, height: 180 };
  if (node.data.icon && node.data.showLabel === false) return { width: 58, height: 58 };
  if (node.data.shape === 'text') return { width: 120, height: 34 };
  if (node.data.showLabel === false) {
    if (node.data.shape === 'circle') return { width: 72, height: 72 };
    if (node.data.shape === 'diamond') return { width: 96, height: 72 };
    if (node.data.shape === 'cylinder') return { width: 120, height: 58 };
    return { width: 132, height: 56 };
  }
  if (node.data.shape === 'circle') return { width: 104, height: 104 };
  if (node.data.shape === 'diamond') return { width: 138, height: 100 };
  if (node.data.shape === 'cylinder') return { width: 164, height: 82 };
  return { width: 168, height: 68 };
}

function newComponentSize(item: PaletteItem): { width: number; height: number } {
  if (item.width && item.height) return { width: item.width, height: item.height };
  if (item.icon) return { width: 58, height: 58 };
  if (item.shape === 'text') return { width: 120, height: 34 };
  if (item.shape === 'circle') return { width: 72, height: 72 };
  if (item.shape === 'diamond') return { width: 96, height: 72 };
  if (item.shape === 'cylinder') return { width: 120, height: 58 };
  return { width: 132, height: 56 };
}

function findActivationAt(nodes: DiagramNode[], lifeline: DiagramNode, slot: number): DiagramNode | undefined {
  const lifelinePosition = absoluteNodePosition(nodes, lifeline);
  const lifelineWidth = lifeline.width ?? 160;
  const lifelineHeight = lifeline.height ?? 560;
  const connectionPoint = {
    x: lifelinePosition.x + lifelineWidth / 2,
    y: lifelinePosition.y + lifelineHeight * sequenceSlotPercentage(slot) / 100
  };
  return nodes.find((node) => {
    if (node.data.shape !== 'activation') return false;
    const position = absoluteNodePosition(nodes, node);
    const width = node.width ?? 14;
    const height = node.height ?? 96;
    return connectionPoint.x >= position.x - 4
      && connectionPoint.x <= position.x + width + 4
      && connectionPoint.y >= position.y - 4
      && connectionPoint.y <= position.y + height + 4;
  });
}

function sequenceEndpointY(nodes: DiagramNode[], nodeId: string, handleId?: string | null): number | undefined {
  const node = nodes.find((candidate) => candidate.id === nodeId);
  if (!node) return undefined;
  const position = absoluteNodePosition(nodes, node);
  const height = node.height ?? defaultNodeSize(node).height;
  if (node.data.shape === 'lifeline') {
    const slot = parseSequenceSlot(handleId);
    return position.y + height * (slot === undefined ? 50 : sequenceSlotPercentage(slot)) / 100;
  }
  if (node.data.shape === 'activation') {
    const handle = parseSequenceActivationHandle(handleId);
    return position.y + height * (handle ? sequenceActivationSlotPercentage(handle.slot, handle.version) : 50) / 100;
  }
  return position.y + height / 2;
}

function closestActivationHandle(
  activation: DiagramNode,
  nodes: DiagramNode[],
  side: SequenceActivationSide,
  connectionY?: number
): string {
  if (connectionY === undefined) return sequenceActivationHandleId(side, Math.floor(sequenceActivationSlotCount / 2));
  const position = absoluteNodePosition(nodes, activation);
  const height = activation.height ?? 96;
  const percentage = Math.max(0, Math.min(100, (connectionY - position.y) * 100 / height));
  const slot = Math.round(percentage * (sequenceActivationSlotCount - 1) / 100);
  return sequenceActivationHandleId(side, slot);
}

function closestLifelineSlot(lifeline: DiagramNode, nodes: DiagramNode[], connectionY: number): number {
  const position = absoluteNodePosition(nodes, lifeline);
  const height = lifeline.height ?? 560;
  const percentage = Math.max(12, Math.min(98, (connectionY - position.y) * 100 / height));
  return Math.round((percentage - 12) * (sequenceLifelineSlotCount - 1) / 86);
}

function closestSequenceHandleAtY(
  node: DiagramNode,
  nodes: DiagramNode[],
  currentHandle: string | undefined,
  connectionY: number
): string | undefined {
  if (node.data.shape === 'lifeline') return `slot-${closestLifelineSlot(node, nodes, connectionY)}`;
  if (node.data.shape === 'activation') {
    const current = parseSequenceActivationHandle(currentHandle);
    return closestActivationHandle(node, nodes, current?.side ?? 'left', connectionY);
  }
  return currentHandle;
}

type SequenceMessageRuntimeData = DiagramEdge['data'] & {
  onVerticalMoveStart?: (edgeId: string) => void;
  onVerticalMove?: (edgeId: string, clientY: number) => void;
  onVerticalMoveEnd?: () => void;
  onSelect?: (edgeId: string) => void;
};

function SequenceMessageEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  markerStart,
  markerEnd,
  style,
  label,
  labelStyle,
  labelBgStyle,
  labelBgPadding,
  labelBgBorderRadius,
  interactionWidth,
  data
}: EdgeProps) {
  const edgePath = `M ${sourceX} ${sourceY} L ${targetX} ${sourceY}`;
  const labelX = (sourceX + targetX) / 2;
  const runtime = data as SequenceMessageRuntimeData | undefined;
  const runtimeRef = useRef(runtime);
  const dragCleanupRef = useRef<(() => void) | undefined>(undefined);
  const [dragging, setDragging] = useState(false);
  runtimeRef.current = runtime;
  useEffect(() => () => dragCleanupRef.current?.(), []);
  return <>
    <BaseEdge
      id={id}
      path={edgePath}
      labelX={labelX}
      labelY={sourceY}
      markerStart={markerStart}
      markerEnd={markerEnd}
      style={style}
      label={label}
      labelStyle={labelStyle}
      labelBgStyle={labelBgStyle}
      labelBgPadding={labelBgPadding}
      labelBgBorderRadius={labelBgBorderRadius}
      interactionWidth={interactionWidth}
    />
    <path
      className={`sequence-edge-drag-zone nodrag nopan ${dragging ? 'dragging' : ''}`}
      d={edgePath}
      onMouseDown={(event) => {
        event.preventDefault();
        event.stopPropagation();
        dragCleanupRef.current?.();
        setDragging(true);
        runtime?.onVerticalMoveStart?.(id);
        const onMouseMove = (moveEvent: MouseEvent) => runtimeRef.current?.onVerticalMove?.(id, moveEvent.clientY);
        const finish = () => {
          window.removeEventListener('mousemove', onMouseMove);
          window.removeEventListener('mouseup', finish);
          dragCleanupRef.current = undefined;
          setDragging(false);
          runtimeRef.current?.onVerticalMoveEnd?.();
        };
        dragCleanupRef.current = () => {
          window.removeEventListener('mousemove', onMouseMove);
          window.removeEventListener('mouseup', finish);
        };
        window.addEventListener('mousemove', onMouseMove);
        window.addEventListener('mouseup', finish, { once: true });
      }}
      onClick={(event) => {
        event.stopPropagation();
        runtime?.onSelect?.(id);
      }}
      aria-label="上下拖动消息线"
      role="button"
      tabIndex={0}
    />
    <rect
      className={`sequence-edge-drag-indicator ${dragging ? 'dragging' : ''}`}
      x={labelX - 14}
      y={sourceY - 2}
      width={28}
      height={4}
      rx={2}
    />
  </>;
}

function absoluteNodePosition(nodes: DiagramNode[], node: DiagramNode): { x: number; y: number } {
  if (!node.parentId) return node.position;
  const parent = nodes.find((candidate) => candidate.id === node.parentId);
  if (!parent) return node.position;
  const parentPosition = absoluteNodePosition(nodes, parent);
  return { x: parentPosition.x + node.position.x, y: parentPosition.y + node.position.y };
}

function safeFileName(value: string): string {
  return value.trim().replace(/[\\/:*?"<>|]+/g, '-').replace(/\s+/g, ' ') || 'diagram';
}

function downloadBlob(blob: Blob, fileName: string) {
  const url = URL.createObjectURL(blob);
  downloadDataUrl(url, fileName);
  window.setTimeout(() => URL.revokeObjectURL(url), 2000);
}

function downloadDataUrl(url: string, fileName: string) {
  const anchor = window.document.createElement('a');
  anchor.href = url;
  anchor.download = fileName;
  anchor.click();
}

function resolvedCanvasColor(): string {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? '#13151A' : '#FBFCFF';
}
