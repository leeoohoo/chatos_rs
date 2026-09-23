import type { Dispatch, MutableRefObject, RefObject, SetStateAction } from 'react';
import { addEdge, applyEdgeChanges, applyNodeChanges, type Connection, type EdgeChange, type FinalConnectionState, type NodeChange } from '@xyflow/react';
import { toPng, toSvg } from 'html-to-image';
import type { DiagramDocument, DiagramEdge, DiagramKind, DiagramNode, DiagramProject } from '../../src/schema';
import { layoutDiagram } from '../../src/layout';
import { analyzeMindMap, createMindMapEdge, insertMindMapChild, layoutMindMap, mindMapNodeSize, mindMapSubtreeIds } from '../../src/mindmap';
import { nextNodeZIndex, reorderNodeLayers, type NodeLayerAction } from '../../src/layers';
import { detectPlantUmlDiagramKind, diagramToPlantUml, hasEmbeddedDiagramLayout, plantUmlToDiagram } from '../../src/plantuml';
import { parseSequenceActivationHandle, parseSequenceSlot, sequenceActivationHandleId, sequenceActivationSlotCount, sequenceLifelineSlotCount, sequenceSlotPercentage, type SequenceActivationSide } from '../../src/sequence';
import { componentDragType, type PaletteItem, type SequenceMessagePreset } from './TemplateSidebar';
import { createRepository } from './repository';
import { measuredNode, rememberNodeMeasurements, type NodeMeasurementCache } from './node-measurements';
import { absoluteNodePosition, balancedMindMapSide, closestActivationHandle, closestLifelineSlot, closestSequenceHandleAtY, connectionEndClientPosition, defaultNodeSize, downloadBlob, downloadDataUrl, findActivationAt, kindLabel, newComponentSize, resolvedCanvasColor, safeFileName, sequenceEndpointY, supportsPlantUml } from './DiagramStudioSupport';

type Repository = Awaited<ReturnType<typeof createRepository>>;

interface DiagramEditorActionContext extends Record<string, any> {
  document?: DiagramDocument;
  selectedNodeIds: Set<string>;
  selectedEdgeId?: string;
  setSelectedNodeIds: Dispatch<SetStateAction<Set<string>>>;
  setPast: Dispatch<SetStateAction<DiagramDocument[]>>;
  setFuture: Dispatch<SetStateAction<DiagramDocument[]>>;
  nodeMeasurements: MutableRefObject<NodeMeasurementCache>;
  dragSnapshot: MutableRefObject<DiagramDocument | null>;
  resizeSnapshot: MutableRefObject<DiagramDocument | null>;
  edgeMoveSnapshot: MutableRefObject<DiagramDocument | null>;
  repository?: Repository;
  activeProject?: DiagramProject;
  plantUmlSource: string;
}

export function createDiagramEditorActions(context: DiagramEditorActionContext) {
  const {
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
  } = context;

  function onNodesChange(changes: NodeChange[]) {
    if (!document) return;
    rememberNodeMeasurements(nodeMeasurements.current, document.documentId, changes);
    const selectionChanges = changes.filter((change): change is Extract<NodeChange, { type: 'select' }> => change.type === 'select');
    if (selectionChanges.length > 0) {
      setSelectedNodeIds((current) => {
        const next = new Set(current);
        for (const change of selectionChanges) {
          if (change.selected) next.add(change.id);
          else next.delete(change.id);
        }
        return next;
      });
      if (selectionChanges.some((change) => change.selected)) setSelectedEdgeId(undefined);
    }
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
    const selectionChanges = changes.filter((change): change is Extract<EdgeChange, { type: 'select' }> => change.type === 'select');
    if (selectionChanges.length > 0) {
      const selected = selectionChanges.find((change) => change.selected);
      if (selected) {
        setSelectedNodeIds(new Set());
        setSelectedEdgeId(selected.id);
      } else {
        setSelectedEdgeId((current: string | undefined) => selectionChanges.some((change) => change.id === current) ? undefined : current);
      }
    }
    const structuralChanges = changes.filter((change) => change.type !== 'select');
    if (structuralChanges.length === 0) return;
    updateDocumentLive({ ...document, edges: applyEdgeChanges(structuralChanges, document.edges as never) as unknown as DiagramEdge[] });
  }

  function onConnect(connection: Connection) {
    if (!document || !connection.source || !connection.target) return;
    if (document.kind === 'mindmap') {
      const analysis = analyzeMindMap(document);
      const source = document.nodes.find((node) => node.id === connection.source);
      const target = document.nodes.find((node) => node.id === connection.target);
      if (!source || !target || !['mindmap-root', 'mindmap-topic'].includes(source.data.shape) || target.data.shape !== 'mindmap-topic') {
        showToast('思维导图只能把中心主题或分支主题连接到一个分支主题。');
        return;
      }
      if (analysis.parentByNode.has(target.id)) {
        showToast('这个主题已经有父主题；请先删除原分支线再重新连接。');
        return;
      }
      const descendants = new Set<string>();
      const collect = (nodeId: string) => {
        if (descendants.has(nodeId)) return;
        descendants.add(nodeId);
        for (const child of analysis.childrenByNode.get(nodeId) ?? []) collect(child.id);
      };
      collect(target.id);
      if (descendants.has(source.id)) {
        showToast('思维导图分支不能形成循环。');
        return;
      }
      const side = source.data.shape === 'mindmap-root'
        ? (target.position.x < source.position.x ? 'left' : 'right')
        : source.data.mindmapSide ?? 'right';
      const nextNodes = document.nodes.map((node) => node.id === target.id ? { ...node, data: { ...node.data, mindmapSide: side } } : node);
      commit(layoutMindMap({ ...document, nodes: nextNodes, edges: [...document.edges, createMindMapEdge(source.id, target.id, nextNodes)] }));
      return;
    }
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

  function onConnectEnd(event: MouseEvent | TouchEvent, connectionState: FinalConnectionState) {
    if (!document || document.kind !== 'mindmap' || connectionState.isValid || !connectionState.fromNode || connectionState.toNode) return;
    const eventTarget = event.target;
    if (eventTarget instanceof Element && eventTarget.closest('.react-flow__node')) return;
    const parent = document.nodes.find((node) => node.id === connectionState.fromNode?.id);
    if (!parent || !['mindmap-root', 'mindmap-topic'].includes(parent.data.shape)) return;
    const clientPosition = connectionEndClientPosition(event);
    if (!clientPosition) return;
    const position = reactFlow.screenToFlowPosition(clientPosition);
    const parentSize = mindMapNodeSize(parent);
    const side = parent.data.shape === 'mindmap-root'
      ? connectionState.fromHandle?.id === 'left'
        ? 'left'
        : connectionState.fromHandle?.id === 'right'
          ? 'right'
          : position.x < parent.position.x + parentSize.width / 2 ? 'left' : 'right'
      : parent.data.mindmapSide ?? 'right';
    const inserted = insertMindMapChild(document, parent.id, { position, side, label: '新主题' });
    commit(inserted.document);
    setSelectedNodeIds(new Set([inserted.node.id]));
    setSelectedEdgeId(undefined);
    setMindmapEdit({ nodeId: inserted.node.id, value: '' });
    window.setTimeout(() => reactFlow.fitView({ padding: 0.28, duration: 280, maxZoom: 1.2 }), 40);
  }

  function addNode(item: PaletteItem, droppedPosition?: { x: number; y: number }) {
    if (!document) return;
    if (document.kind === 'mindmap' && (item.shape === 'mindmap-root' || item.shape === 'mindmap-topic')) {
      const analysis = analyzeMindMap(document);
      const existingRoot = analysis.roots.find((node) => node.data.shape === 'mindmap-root');
      if (item.shape === 'mindmap-root' && existingRoot) {
        showToast('一张思维导图只能有一个中心主题。');
        setSelectedNodeIds(new Set([existingRoot.id]));
        return;
      }
      if (item.shape === 'mindmap-topic' && !existingRoot) {
        showToast('请先拖入一个中心主题。');
        return;
      }
      const parent = item.shape === 'mindmap-topic'
        ? document.nodes.find((node) => selectedNodeIds.has(node.id) && ['mindmap-root', 'mindmap-topic'].includes(node.data.shape)) ?? existingRoot
        : undefined;
      const side: 'left' | 'right' | undefined = parent?.data.shape === 'mindmap-root'
        ? (document.edges.filter((edge) => edge.source === parent.id && document.nodes.find((node) => node.id === edge.target)?.data.mindmapSide === 'right').length
          <= document.edges.filter((edge) => edge.source === parent.id && document.nodes.find((node) => node.id === edge.target)?.data.mindmapSide === 'left').length ? 'right' : 'left')
        : parent?.data.mindmapSide;
      const newNode: DiagramNode = {
        id: `mindmap-${crypto.randomUUID().slice(0, 8)}`,
        type: 'diagramNode',
        position: droppedPosition ?? { x: 0, y: 0 },
        width: item.shape === 'mindmap-root' ? 200 : 150,
        height: item.shape === 'mindmap-root' ? 64 : 46,
        zIndex: nextNodeZIndex(document.nodes),
        data: {
          label: item.shape === 'mindmap-root' ? '中心主题' : '新主题',
          category: 'mindmap',
          shape: item.shape,
          color: item.color,
          borderColor: item.color,
          fillColor: item.shape === 'mindmap-root' ? item.fillColor ?? item.color : 'transparent',
          ...(item.shape === 'mindmap-root' ? { textColor: '#FFFFFF' } : {}),
          showLabel: true,
          fontSize: item.shape === 'mindmap-root' ? 17 : 14,
          fontWeight: item.shape === 'mindmap-root' ? 700 : 620,
          ...(side ? { mindmapSide: side } : {}),
          mindmapOrder: parent ? document.edges.filter((edge) => edge.source === parent.id).length : 0
        }
      };
      const next = layoutMindMap({
        ...document,
        nodes: [...document.nodes, newNode],
        edges: parent ? [...document.edges, createMindMapEdge(parent.id, newNode.id, [...document.nodes, newNode])] : document.edges
      });
      commit(next);
      setSelectedNodeIds(new Set([newNode.id]));
      setSelectedEdgeId(undefined);
      setMindmapEdit({ nodeId: newNode.id, value: newNode.data.label });
      window.setTimeout(() => reactFlow.fitView({ padding: 0.28, duration: 280, maxZoom: 1.2 }), 40);
      return;
    }
    const position = droppedPosition ?? reactFlow.screenToFlowPosition({ x: window.innerWidth / 2, y: window.innerHeight / 2 });
    const initialSize = newComponentSize(item);
    const newNode: DiagramNode = {
      id: `${item.category}-${crypto.randomUUID().slice(0, 8)}`,
      type: item.shape === 'lane' ? 'laneNode' : 'diagramNode',
      position,
      width: initialSize.width,
      height: initialSize.height,
      zIndex: item.shape === 'lane' ? 0 : nextNodeZIndex(document.nodes),
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
    setSelectedNodeIds(new Set([newNode.id]));
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
    const previousNode = document.nodes.find((node) => node.id === nextNode.id);
    const next = { ...document, nodes: document.nodes.map((node) => node.id === nextNode.id ? nextNode : node) };
    commit(document.kind === 'mindmap' && ['mindmap-root', 'mindmap-topic'].includes(nextNode.data.shape) ? layoutMindMap(next) : next);
    if (document.kind === 'mindmap' && previousNode?.data.mindmapSide !== nextNode.data.mindmapSide) {
      window.setTimeout(() => reactFlow.fitView({ padding: 0.24, duration: 280, maxZoom: 1.2 }), 40);
    }
  }

  function addMindMapRelative(nodeId: string, relation: 'child' | 'sibling') {
    if (!document || document.kind !== 'mindmap') return;
    const analysis = analyzeMindMap(document);
    const selected = document.nodes.find((node) => node.id === nodeId);
    if (!selected || !['mindmap-root', 'mindmap-topic'].includes(selected.data.shape)) return;
    const requestedParentId = relation === 'sibling' ? analysis.parentByNode.get(selected.id) : selected.id;
    const parent = document.nodes.find((node) => node.id === requestedParentId) ?? selected;
    const side = parent.data.shape === 'mindmap-root'
      ? selected.data.shape === 'mindmap-topic' ? selected.data.mindmapSide ?? 'right' : balancedMindMapSide(document, parent.id)
      : parent.data.mindmapSide ?? 'right';
    const parentSize = mindMapNodeSize(parent);
    const position = {
      x: parent.position.x + (side === 'right' ? parentSize.width + 250 : -250),
      y: parent.position.y + parentSize.height / 2
    };
    const inserted = insertMindMapChild(document, parent.id, { position, side, label: '新主题' });
    commit(inserted.document);
    setSelectedNodeIds(new Set([inserted.node.id]));
    setSelectedEdgeId(undefined);
    setMindmapEdit({ nodeId: inserted.node.id, value: '' });
    window.setTimeout(() => reactFlow.fitView({ padding: 0.28, duration: 280, maxZoom: 1.2 }), 40);
  }

  function toggleMindMapCollapse(nodeId: string) {
    if (!document || document.kind !== 'mindmap') return;
    const nodes = document.nodes.map((node) => node.id === nodeId ? { ...node, data: { ...node.data, mindmapCollapsed: !node.data.mindmapCollapsed } } : node);
    commit(layoutMindMap({ ...document, nodes }));
  }

  function changeNodeLayer(action: NodeLayerAction) {
    if (!document || selectedNodeIds.size === 0) return;
    const nextNodes = reorderNodeLayers(document.nodes, selectedNodeIds, action);
    if (nextNodes === document.nodes) {
      showToast(action === 'front' || action === 'forward' ? '已经在最上层' : '已经在最下层');
      return;
    }
    commit({ ...document, nodes: nextNodes });
  }

  function updateEdge(nextEdge: DiagramEdge) {
    if (!document) return;
    commit({ ...document, edges: document.edges.map((edge) => edge.id === nextEdge.id ? nextEdge : edge) });
  }

  function deleteSelection() {
    if (!document) return;
    if (selectedNodeIds.size > 0) {
      const removedNodeIds = new Set(selectedNodeIds);
      if (document.kind === 'mindmap') {
        for (const nodeId of selectedNodeIds) for (const subtreeId of mindMapSubtreeIds(document, nodeId)) removedNodeIds.add(subtreeId);
      }
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
    setSelectedNodeIds(new Set());
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
    setSelectedNodeIds(new Set());
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

  async function applyPlantUmlSource() {
    if (!document) return;
    if (!supportsPlantUml(document.kind)) {
      setPlantUmlError('当前图形类型尚未接入 PlantUML 双向转换。');
      return;
    }
    try {
      const imported = plantUmlToDiagram(plantUmlSource, {
        documentId: document.documentId,
        title: document.title,
        revision: document.revision,
        createdAt: document.createdAt,
        updatedAt: document.updatedAt,
        kind: document.kind
      });
      const next = hasEmbeddedDiagramLayout(plantUmlSource)
        ? imported
        : await layoutDiagram(imported, document.kind === 'flowchart' || document.kind === 'swimlane' ? 'DOWN' : 'RIGHT');
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
      const isPlantUml = /\.(puml|plantuml|pu)$/i.test(file.name) || /^\s*@start(?:uml|mindmap)\b/im.test(text);
      if (isPlantUml) {
        const detectedKind = detectPlantUmlDiagramKind(text);
        const fallbackTitle = file.name.replace(/\.(puml|plantuml|pu)$/i, '').trim() || `导入的${kindLabel(detectedKind)}`;
        const seed = await repository.createInProject(activeProject.projectId, detectedKind, fallbackTitle);
        const parsed = plantUmlToDiagram(text, {
          documentId: seed.documentId,
          title: fallbackTitle,
          revision: seed.revision,
          createdAt: seed.createdAt,
          updatedAt: seed.updatedAt,
          kind: detectedKind
        });
        const imported = hasEmbeddedDiagramLayout(text)
          ? parsed
          : await layoutDiagram(parsed, detectedKind === 'flowchart' || detectedKind === 'swimlane' ? 'DOWN' : 'RIGHT');
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


  return {
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
  };
}
